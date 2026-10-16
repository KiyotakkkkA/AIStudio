use std::sync::Arc;

use serde_json::{Map, Value, json};
use zvs_contract::{
    GraphNode, RetryPolicy, RunGraph, RunKind, StartRunInput, SystemDemoJobInput,
    SystemDemoStreamInput, SystemDemoStreamOutput, SystemDeviceInput, SystemNativePingInput,
    SystemPingInput,
};

use super::IpcServer;
use crate::services::Services;

pub const DEMO_JOB_TITLE: &str = "Демонстрационная задача";

pub fn demo_job(input: &SystemDemoJobInput) -> StartRunInput {
    let mut params = Map::new();
    params.insert("steps".to_owned(), json!(input.steps));
    if let Some(interval) = input.interval_ms {
        params.insert("intervalMs".to_owned(), json!(interval));
    }
    let node = GraphNode {
        dependencies: Some(Vec::new()),
        bindings: Some(Default::default()),
        retry: Some(RetryPolicy {
            max_attempts: 1,
            backoff_ms: 0,
        }),
        input: Some(json!({ "job": "job.sleep", "params": Value::Object(params) })),
        ..GraphNode::new("sleep", "job.run")
    };
    StartRunInput {
        title: Some(DEMO_JOB_TITLE.to_owned()),
        input: Some(Value::Null),
        concurrency: Some(1),
        ..StartRunInput::new(RunKind::Job, RunGraph::new(vec![node]))
    }
}

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "system.ping",
            |services: Arc<Services>, input: SystemPingInput| async move {
                Ok(services.system.ping(input.sent_at))
            },
        )
        .handle(
            "system.demoStream",
            |services: Arc<Services>, input: SystemDemoStreamInput| async move {
                let stream_id = services.system.demo_stream(input.steps)?;
                Ok(SystemDemoStreamOutput { stream_id })
            },
        )
        .handle(
            "system.demoJob",
            |services: Arc<Services>, input: SystemDemoJobInput| async move {
                services.runs.start(demo_job(&input))
            },
        )
        .handle(
            "system.nativePing",
            |services: Arc<Services>, input: SystemNativePingInput| async move {
                services.system.native_ping(input.text).await
            },
        )
        .handle(
            "system.device",
            |services: Arc<Services>, input: SystemDeviceInput| async move {
                services.system.device_profile(input.refresh()).await
            },
        )
        .handle(
            "system.resources",
            |services: Arc<Services>, _input: ()| async move {
                services.system.resource_sample().await
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use zvs_contract::{HostEventDraft, StreamId};

    use zvs_contract::AppError;

    use super::*;
    use crate::drivers::core::{ChunkConfig, SectionChunk, TextChunk, TextCore, TextSegment};
    use crate::platform::clock::FakeClock;
    use crate::platform::device::{DeviceProbe, DeviceProbeOptions, DeviceReadings};
    use crate::platform::events::EventBus;
    use crate::platform::gpu::{GpuAdapter, GpuProbe, VENDOR_NVIDIA};
    use crate::platform::logger::silent_logger;
    use crate::platform::metrics::{
        MemoryReading, ResourceCounters, ResourceMonitor, ResourceMonitorOptions,
    };
    use crate::services::SystemService;
    use crate::testing::{MemorySink, TestServices, test_services};

    fn server(events: EventBus, interval: Duration) -> (TestServices, IpcServer) {
        serve(SystemService::new(
            Arc::new(FakeClock::new(1_250)),
            events,
            interval,
        ))
    }

    fn serve(system: SystemService) -> (TestServices, IpcServer) {
        let host = test_services(system);
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        (host, server)
    }

    fn system() -> SystemService {
        SystemService::new(
            Arc::new(FakeClock::new(1_250)),
            EventBus::default(),
            Duration::ZERO,
        )
    }

    struct CountingCore(std::sync::Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl TextCore for CountingCore {
        async fn chunk(
            &self,
            text: String,
            config: ChunkConfig,
        ) -> Result<Vec<TextChunk>, AppError> {
            assert_eq!(config, ChunkConfig::default());
            self.0.lock().expect("calls").push(text.clone());
            Ok(vec![TextChunk {
                byte_end: u32::try_from(text.len()).expect("small"),
                text,
                byte_start: 0,
                token_count: 1,
            }])
        }

        async fn chunk_segments(
            &self,
            _segments: Vec<TextSegment>,
            _config: ChunkConfig,
        ) -> Result<Vec<SectionChunk>, AppError> {
            unreachable!("unused")
        }

        async fn hash(&self, _bytes: Vec<u8>) -> Result<String, AppError> {
            unreachable!("unused")
        }
    }

    #[tokio::test]
    async fn native_ping_delegates_through_the_core_port_and_satisfies_the_contract() {
        let core = Arc::new(CountingCore(std::sync::Mutex::new(Vec::new())));
        let (_host, server) = serve(system().with_core(core.clone()));
        assert_eq!(
            server
                .dispatch("system.nativePing", json!({ "text": "sample" }))
                .await,
            json!({ "ok": true, "data": { "count": 1 } })
        );
        assert_eq!(*core.0.lock().expect("calls"), ["sample"]);
        let refused = server
            .dispatch("system.nativePing", json!({ "text": 3 }))
            .await;
        assert_eq!(refused["error"]["code"], json!("VALIDATION_FAILED"));
    }

    #[tokio::test]
    async fn native_ping_counts_chunks_in_process() {
        let (_host, server) = serve(system());
        assert_eq!(
            server
                .dispatch("system.nativePing", json!({ "text": "word ".repeat(600) }))
                .await,
            json!({ "ok": true, "data": { "count": 3 } })
        );
    }

    struct Adapters;

    impl GpuProbe for Adapters {
        fn adapters(&self) -> Result<Vec<GpuAdapter>, String> {
            Ok(vec![GpuAdapter {
                vendor_id: VENDOR_NVIDIA,
                device_id: 0x2786,
                description: "NVIDIA GeForce RTX 4070".to_owned(),
                dedicated_video_memory: 12 * 1024 * 1024 * 1024,
                software: false,
            }])
        }
    }

    #[tokio::test]
    async fn the_device_profile_is_cached_until_a_refresh_is_asked_for() {
        let clock = Arc::new(FakeClock::new(5_000));
        let device = Arc::new(DeviceProbe::new(DeviceProbeOptions {
            readings: Arc::new(|| DeviceReadings {
                platform: "win32".to_owned(),
                arch: "x64".to_owned(),
                cpu_model: "Test CPU".to_owned(),
                cpu_cores: 8,
                total_memory_bytes: 16 * 1024 * 1024 * 1024,
                free_memory_bytes: 4 * 1024 * 1024 * 1024,
            }),
            gpu: Some(Arc::new(Adapters)),
            free_disk: Some(Arc::new(|| Ok(42))),
            ..DeviceProbeOptions::new(clock.clone())
        }));
        let (_host, server) = serve(system().with_device(device));
        let first = server.dispatch("system.device", json!({})).await;
        assert_eq!(first["ok"], json!(true));
        assert_eq!(
            first["data"]["gpu"],
            json!({
                "vendor": "NVIDIA",
                "model": "NVIDIA GeForce RTX 4070",
                "vramBytes": 12_u64 * 1024 * 1024 * 1024,
                "discrete": true
            })
        );
        assert_eq!(first["data"]["tier"], json!("high"));
        assert_eq!(first["data"]["freeDiskBytes"], json!(42));
        assert_eq!(first["data"]["measuredAt"], json!(5_000));
        clock.advance(10);
        assert_eq!(
            server
                .dispatch("system.device", json!({ "refresh": false }))
                .await["data"]["measuredAt"],
            json!(5_000)
        );
        assert_eq!(
            server
                .dispatch("system.device", json!({ "refresh": true }))
                .await["data"]["measuredAt"],
            json!(5_010)
        );
    }

    struct Quiet;

    impl ResourceCounters for Quiet {
        fn cpu_percent(&mut self) -> f64 {
            12.34
        }

        fn memory(&mut self) -> MemoryReading {
            MemoryReading {
                free_bytes: 1_000,
                total_bytes: 4_000,
            }
        }

        fn process_memory(&mut self) -> u64 {
            500
        }
    }

    #[tokio::test]
    async fn a_resource_sample_is_one_reading_of_the_machine() {
        let resources = Arc::new(ResourceMonitor::new(ResourceMonitorOptions {
            counters: Box::new(Quiet),
            gpu: None,
            clock: Arc::new(FakeClock::new(7_000)),
            gpu_ttl: Duration::ZERO,
        }));
        let (_host, server) = serve(system().with_resources(resources));
        assert_eq!(
            server.dispatch("system.resources", json!(null)).await,
            json!({ "ok": true, "data": {
                "at": 7_000,
                "cpuPercent": 12.3,
                "memoryUsedBytes": 3_000,
                "memoryTotalBytes": 4_000,
                "processMemoryBytes": 500,
                "gpuName": null,
                "gpuPercent": null,
                "vramUsedBytes": null,
                "vramTotalBytes": null
            } })
        );
    }

    #[tokio::test]
    async fn system_ping_answers_with_host_time_and_round_trip_hint() {
        let (_host, server) = server(EventBus::default(), Duration::ZERO);
        assert_eq!(
            server
                .dispatch("system.ping", json!({ "sentAt": 1_000 }))
                .await,
            json!({ "ok": true, "data": { "pong": true, "hostTime": 1_250, "roundTripHint": 250.0 } })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn system_demo_stream_counts_to_the_requested_total_and_terminates() {
        let events = EventBus::default();
        let sink = MemorySink::shared();
        events.attach(sink.clone());
        let (_host, server) = server(events.clone(), Duration::from_millis(200));
        let response = server
            .dispatch("system.demoStream", json!({ "steps": 10 }))
            .await;
        assert_eq!(response["ok"], json!(true));
        let stream_id = StreamId::parse(response["data"]["streamId"].as_str().expect("an id"))
            .expect("a stream id");
        assert_eq!(events.open(), 1);

        while events.open() > 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let sent = sink.events();
        assert_eq!(
            sent.iter()
                .filter_map(|event| match event.body {
                    HostEventDraft::Progress { done, total } => {
                        assert_eq!(total, 10);
                        Some(done)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            (1..=10).collect::<Vec<u64>>()
        );
        assert!(matches!(
            sent.last().map(|event| &event.body),
            Some(HostEventDraft::End { .. })
        ));
        assert!(sent.iter().all(|event| event.stream_id == stream_id));
    }

    #[tokio::test]
    async fn system_demo_stream_validates_the_step_count() {
        let events = EventBus::default();
        let (_host, server) = server(events.clone(), Duration::ZERO);
        for steps in [0, 101] {
            let response = server
                .dispatch("system.demoStream", json!({ "steps": steps }))
                .await;
            assert_eq!(response["error"]["code"], json!("VALIDATION_FAILED"));
        }
        assert_eq!(events.open(), 0);
    }
}
