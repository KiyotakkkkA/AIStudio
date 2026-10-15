use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::json;
use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, ResourceSampleDto};

use super::clock::SharedClock;
use super::gpu::fit_utf16;
use super::logger::{LogLevel, SharedLogger};

const MIB: f64 = 1024.0 * 1024.0;
pub const NVIDIA_SMI_TIMEOUT: Duration = Duration::from_secs(4);
pub const GPU_SAMPLE_LIMIT: Duration = Duration::from_secs(5);
pub const DEFAULT_GPU_TTL: Duration = Duration::from_secs(1);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq)]
pub struct GpuReading {
    pub name: String,
    pub utilisation_percent: Option<f64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryReading {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

pub trait ResourceCounters: Send {
    fn cpu_percent(&mut self) -> f64;
    fn memory(&mut self) -> MemoryReading;
    fn process_memory(&mut self) -> u64;
}

pub struct SysinfoCounters {
    system: System,
    pid: Option<Pid>,
}

impl SysinfoCounters {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();
        Self {
            system,
            pid: sysinfo::get_current_pid().ok(),
        }
    }
}

impl Default for SysinfoCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceCounters for SysinfoCounters {
    fn cpu_percent(&mut self) -> f64 {
        self.system.refresh_cpu_usage();
        f64::from(self.system.global_cpu_usage())
    }

    fn memory(&mut self) -> MemoryReading {
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        MemoryReading {
            free_bytes: self.system.available_memory(),
            total_bytes: self.system.total_memory(),
        }
    }

    fn process_memory(&mut self) -> u64 {
        let Some(pid) = self.pid else {
            return 0;
        };
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            false,
            ProcessRefreshKind::nothing().with_memory(),
        );
        self.system.process(pid).map_or(0, sysinfo::Process::memory)
    }
}

#[async_trait]
pub trait GpuSampler: Send + Sync {
    async fn sample(&self, cancel: &CancellationToken) -> Result<Option<GpuReading>, String>;
}

pub type SharedGpuSampler = Arc<dyn GpuSampler>;
pub type SmiRunner =
    Arc<dyn Fn(CancellationToken) -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

pub struct NvidiaSmi {
    runner: SmiRunner,
    unavailable: AtomicBool,
    logger: SharedLogger,
}

impl NvidiaSmi {
    pub fn new(logger: SharedLogger) -> Self {
        Self::with_runner(Arc::new(|cancel| run_nvidia_smi(cancel).boxed()), logger)
    }

    pub fn with_runner(runner: SmiRunner, logger: SharedLogger) -> Self {
        Self {
            runner,
            unavailable: AtomicBool::new(false),
            logger,
        }
    }

    pub fn disabled(&self) -> bool {
        self.unavailable.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl GpuSampler for NvidiaSmi {
    async fn sample(&self, cancel: &CancellationToken) -> Result<Option<GpuReading>, String> {
        if self.disabled() {
            return Ok(None);
        }
        match (self.runner)(cancel.clone()).await {
            Ok(stdout) => Ok(parse_nvidia_smi(&stdout)),
            Err(error) => {
                if !cancel.is_cancelled() {
                    self.unavailable.store(true, Ordering::SeqCst);
                    self.logger.log(
                        LogLevel::Debug,
                        "system",
                        "No NVIDIA counters on this machine",
                        &[("error", json!(error))],
                    );
                }
                Ok(None)
            }
        }
    }
}

async fn run_nvidia_smi(cancel: CancellationToken) -> Result<String, String> {
    let mut command = Command::new("nvidia-smi");
    command
        .args([
            "--query-gpu=name,utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    tokio::select! {
        output = tokio::time::timeout(NVIDIA_SMI_TIMEOUT, command.output()) => match output {
            Err(_) => Err("nvidia-smi timed out".to_owned()),
            Ok(Err(error)) => Err(error.to_string()),
            Ok(Ok(output)) if !output.status.success() => {
                Err(format!("nvidia-smi exited with {}", output.status))
            }
            Ok(Ok(output)) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        },
        () = cancel.cancelled() => Err("cancelled".to_owned()),
    }
}

fn number(raw: Option<&str>) -> Option<f64> {
    raw?.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn mebibytes(value: Option<f64>) -> Option<u64> {
    value
        .filter(|value| *value >= 0.0)
        .map(|value| (value * MIB).round() as u64)
}

pub fn parse_nvidia_smi(stdout: &str) -> Option<GpuReading> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    let mut parts = line.split(',').map(str::trim);
    let name = parts
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("NVIDIA");
    let utilisation = number(parts.next()).filter(|value| (0.0..=100.0).contains(value));
    Some(GpuReading {
        name: fit_utf16(name, 200),
        utilisation_percent: utilisation,
        memory_used_bytes: mebibytes(number(parts.next())),
        memory_total_bytes: mebibytes(number(parts.next())),
    })
}

pub fn clamp_percent(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    ((value * 10.0).round() / 10.0).clamp(0.0, 100.0)
}

pub struct ResourceMonitorOptions {
    pub counters: Box<dyn ResourceCounters>,
    pub gpu: Option<SharedGpuSampler>,
    pub clock: SharedClock,
    pub gpu_ttl: Duration,
}

impl ResourceMonitorOptions {
    pub fn new(clock: SharedClock) -> Self {
        Self {
            counters: Box::new(SysinfoCounters::new()),
            gpu: None,
            clock,
            gpu_ttl: DEFAULT_GPU_TTL,
        }
    }
}

#[derive(Default)]
struct GpuCache {
    reading: Option<GpuReading>,
    read_at: Option<i64>,
}

pub struct ResourceMonitor {
    counters: Arc<Mutex<Box<dyn ResourceCounters>>>,
    gpu: Option<SharedGpuSampler>,
    clock: SharedClock,
    gpu_ttl: Duration,
    cache: tokio::sync::Mutex<GpuCache>,
}

pub struct ResourceWatch {
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl ResourceWatch {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for ResourceWatch {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl ResourceMonitor {
    pub fn new(options: ResourceMonitorOptions) -> Self {
        Self {
            counters: Arc::new(Mutex::new(options.counters)),
            gpu: options.gpu,
            clock: options.clock,
            gpu_ttl: options.gpu_ttl,
            cache: tokio::sync::Mutex::new(GpuCache::default()),
        }
    }

    pub async fn sample(&self, cancel: &CancellationToken) -> Result<ResourceSampleDto, AppError> {
        let gpu = self.read_gpu(cancel).await;
        let counters = Arc::clone(&self.counters);
        let (cpu, memory, process) = tokio::task::spawn_blocking(move || {
            let mut counters = counters.lock().unwrap_or_else(PoisonError::into_inner);
            (
                counters.cpu_percent(),
                counters.memory(),
                counters.process_memory(),
            )
        })
        .await
        .map_err(|_| {
            AppError::new(
                AppErrorCode::NativeError,
                "Не удалось измерить загрузку системы",
            )
        })?;
        Ok(ResourceSampleDto {
            at: self.clock.now(),
            cpu_percent: clamp_percent(cpu),
            memory_used_bytes: memory.total_bytes.saturating_sub(memory.free_bytes),
            memory_total_bytes: memory.total_bytes,
            process_memory_bytes: process,
            gpu_name: gpu.as_ref().map(|gpu| gpu.name.clone()),
            gpu_percent: gpu.as_ref().and_then(|gpu| gpu.utilisation_percent),
            vram_used_bytes: gpu.as_ref().and_then(|gpu| gpu.memory_used_bytes),
            vram_total_bytes: gpu.as_ref().and_then(|gpu| gpu.memory_total_bytes),
        })
    }

    pub fn watch(
        self: &Arc<Self>,
        interval: Duration,
        on_sample: impl Fn(ResourceSampleDto) + Send + Sync + 'static,
    ) -> ResourceWatch {
        let monitor = Arc::clone(self);
        let cancel = CancellationToken::new();
        let task = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        () = cancel.cancelled() => return,
                        () = tokio::time::sleep(interval) => {}
                    }
                    if let Ok(sample) = monitor.sample(&cancel).await
                        && !cancel.is_cancelled()
                    {
                        on_sample(sample);
                    }
                }
            })
        };
        ResourceWatch { cancel, task }
    }

    async fn read_gpu(&self, cancel: &CancellationToken) -> Option<GpuReading> {
        let sampler = self.gpu.as_ref()?;
        let mut cache = self.cache.lock().await;
        let ttl = i64::try_from(self.gpu_ttl.as_millis()).unwrap_or(i64::MAX);
        if let Some(at) = cache.read_at
            && self.clock.now() - at < ttl
        {
            return cache.reading.clone();
        }
        match tokio::time::timeout(GPU_SAMPLE_LIMIT, sampler.sample(cancel)).await {
            Ok(Ok(reading)) => {
                cache.reading.clone_from(&reading);
                cache.read_at = Some(self.clock.now());
                reading
            }
            Ok(Err(_)) | Err(_) => None,
        }
    }
}

pub fn nvidia_monitor(clock: SharedClock, logger: SharedLogger) -> ResourceMonitor {
    ResourceMonitor::new(ResourceMonitorOptions {
        gpu: Some(Arc::new(NvidiaSmi::new(logger))),
        ..ResourceMonitorOptions::new(clock)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use zvs_contract::Validate;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::logger::silent_logger;

    const GIB: u64 = 1024 * 1024 * 1024;

    struct Scripted {
        cpu: Vec<f64>,
        index: usize,
    }

    impl ResourceCounters for Scripted {
        fn cpu_percent(&mut self) -> f64 {
            let value = self.cpu[self.index.min(self.cpu.len() - 1)];
            self.index += 1;
            value
        }

        fn memory(&mut self) -> MemoryReading {
            MemoryReading {
                free_bytes: 4 * GIB,
                total_bytes: 16 * GIB,
            }
        }

        fn process_memory(&mut self) -> u64 {
            512 * 1024 * 1024
        }
    }

    struct FixedGpu {
        reading: Result<Option<GpuReading>, String>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl GpuSampler for FixedGpu {
        async fn sample(&self, _cancel: &CancellationToken) -> Result<Option<GpuReading>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reading.clone()
        }
    }

    fn fixed(reading: Result<Option<GpuReading>, String>) -> Arc<FixedGpu> {
        Arc::new(FixedGpu {
            reading,
            calls: AtomicUsize::new(0),
        })
    }

    fn monitor(cpu: &[f64], gpu: Option<SharedGpuSampler>, ttl: Duration) -> ResourceMonitor {
        ResourceMonitor::new(ResourceMonitorOptions {
            counters: Box::new(Scripted {
                cpu: cpu.to_vec(),
                index: 0,
            }),
            gpu,
            clock: Arc::new(FakeClock::default()),
            gpu_ttl: ttl,
        })
    }

    fn rtx() -> GpuReading {
        GpuReading {
            name: "NVIDIA GeForce RTX 4070".to_owned(),
            utilisation_percent: Some(62.0),
            memory_used_bytes: Some(3 * GIB),
            memory_total_bytes: Some(12 * GIB),
        }
    }

    #[tokio::test]
    async fn cpu_load_is_clamped_and_rounded_and_memory_is_used_over_total() {
        let monitor = monitor(&[75.04, 120.0, -3.0, f64::NAN], None, Duration::ZERO);
        let cancel = CancellationToken::new();
        let sample = monitor.sample(&cancel).await.expect("a sample");
        assert_eq!(sample.cpu_percent, 75.0);
        assert_eq!(sample.memory_used_bytes, 12 * GIB);
        assert_eq!(sample.memory_total_bytes, 16 * GIB);
        assert_eq!(sample.process_memory_bytes, 512 * 1024 * 1024);
        assert!(sample.validate().is_ok());
        for expected in [100.0, 0.0, 0.0] {
            assert_eq!(
                monitor.sample(&cancel).await.expect("a sample").cpu_percent,
                expected
            );
        }
    }

    #[tokio::test]
    async fn a_machine_with_no_readable_gpu_reports_null_never_zero() {
        let sample = monitor(&[1.0], None, Duration::ZERO)
            .sample(&CancellationToken::new())
            .await
            .expect("a sample");
        assert_eq!(sample.gpu_name, None);
        assert_eq!(sample.gpu_percent, None);
        assert_eq!(sample.vram_used_bytes, None);
        assert_eq!(sample.vram_total_bytes, None);
    }

    #[tokio::test]
    async fn a_gpu_reading_is_carried_through_in_bytes() {
        let sample = monitor(&[1.0], Some(fixed(Ok(Some(rtx())))), Duration::ZERO)
            .sample(&CancellationToken::new())
            .await
            .expect("a sample");
        assert_eq!(sample.gpu_name.as_deref(), Some("NVIDIA GeForce RTX 4070"));
        assert_eq!(sample.gpu_percent, Some(62.0));
        assert_eq!(sample.vram_used_bytes, Some(3 * GIB));
        assert_eq!(sample.vram_total_bytes, Some(12 * GIB));
    }

    #[tokio::test]
    async fn a_gpu_probe_that_fails_costs_the_sample_nothing() {
        let sample = monitor(
            &[5.0],
            Some(fixed(Err("no smi".to_owned()))),
            Duration::ZERO,
        )
        .sample(&CancellationToken::new())
        .await
        .expect("a sample");
        assert_eq!(sample.gpu_percent, None);
        assert_eq!(sample.cpu_percent, 5.0);
    }

    #[tokio::test]
    async fn a_gpu_reading_is_reused_within_its_time_to_live() {
        let gpu = fixed(Ok(Some(rtx())));
        let monitor = monitor(&[1.0], Some(gpu.clone()), Duration::from_secs(1));
        let cancel = CancellationToken::new();
        monitor.sample(&cancel).await.expect("a sample");
        monitor.sample(&cancel).await.expect("a sample");
        assert_eq!(gpu.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn nvidia_smi_output_is_parsed_into_mebibyte_scaled_bytes() {
        assert_eq!(
            parse_nvidia_smi("\nNVIDIA GeForce RTX 4070, 47, 3072, 12282\n"),
            Some(GpuReading {
                name: "NVIDIA GeForce RTX 4070".to_owned(),
                utilisation_percent: Some(47.0),
                memory_used_bytes: Some(3072 * 1024 * 1024),
                memory_total_bytes: Some(12282 * 1024 * 1024),
            })
        );
        assert_eq!(
            parse_nvidia_smi("Tesla T4, [N/A], [N/A], 15360"),
            Some(GpuReading {
                name: "Tesla T4".to_owned(),
                utilisation_percent: None,
                memory_used_bytes: None,
                memory_total_bytes: Some(15360 * 1024 * 1024),
            })
        );
        assert_eq!(parse_nvidia_smi("  \n"), None);
        assert_eq!(parse_nvidia_smi(", 1").expect("a reading").name, "NVIDIA");
    }

    #[tokio::test]
    async fn the_sampler_runs_nvidia_smi_and_parses_its_answer() {
        let sampler = NvidiaSmi::with_runner(
            Arc::new(|_| {
                async { Ok("NVIDIA GeForce RTX 4070, 47, 3072, 12282\n".to_owned()) }.boxed()
            }),
            silent_logger(),
        );
        let reading = sampler
            .sample(&CancellationToken::new())
            .await
            .expect("samples")
            .expect("a reading");
        assert_eq!(reading.utilisation_percent, Some(47.0));
        assert!(!sampler.disabled());
    }

    #[tokio::test]
    async fn a_machine_without_nvidia_smi_is_probed_once_then_left_alone() {
        let calls = Arc::new(AtomicUsize::new(0));
        let sampler = NvidiaSmi::with_runner(
            {
                let calls = Arc::clone(&calls);
                Arc::new(move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Err("program not found".to_owned()) }.boxed()
                })
            },
            silent_logger(),
        );
        let cancel = CancellationToken::new();
        assert_eq!(sampler.sample(&cancel).await, Ok(None));
        assert_eq!(sampler.sample(&cancel).await, Ok(None));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(sampler.disabled());
    }

    #[tokio::test]
    async fn a_cancelled_probe_does_not_disable_the_sampler() {
        let sampler = NvidiaSmi::with_runner(
            Arc::new(|_| async { Err("cancelled".to_owned()) }.boxed()),
            silent_logger(),
        );
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(sampler.sample(&cancel).await, Ok(None));
        assert!(!sampler.disabled());
    }

    #[tokio::test(start_paused = true)]
    async fn watching_stops_sampling_when_stopped() {
        let monitor = Arc::new(monitor(&[1.0], None, Duration::ZERO));
        let samples = Arc::new(AtomicUsize::new(0));
        let watch = {
            let samples = Arc::clone(&samples);
            monitor.watch(Duration::from_millis(10), move |_| {
                samples.fetch_add(1, Ordering::SeqCst);
            })
        };
        while samples.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        watch.stop();
        let taken = samples.load(Ordering::SeqCst);
        while !watch.is_finished() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(samples.load(Ordering::SeqCst), taken);
    }

    #[tokio::test]
    async fn the_real_machine_reads_as_a_valid_sample() {
        let monitor =
            ResourceMonitor::new(ResourceMonitorOptions::new(Arc::new(FakeClock::default())));
        let sample = monitor
            .sample(&CancellationToken::new())
            .await
            .expect("a sample");
        assert!(sample.memory_total_bytes > 0);
        assert!(sample.process_memory_bytes > 0);
        assert!(sample.validate().is_ok());
    }
}
