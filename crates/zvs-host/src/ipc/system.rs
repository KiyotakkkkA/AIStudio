use std::sync::Arc;

use zvs_contract::{SystemDemoStreamInput, SystemDemoStreamOutput, SystemPingInput};

use super::IpcServer;
use crate::services::Services;

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
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use zvs_contract::{HostEventDraft, StreamId};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::{MemorySink, TestServices, test_services};

    fn server(events: EventBus, interval: Duration) -> (TestServices, IpcServer) {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::new(1_250)),
            events,
            interval,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        (host, server)
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
