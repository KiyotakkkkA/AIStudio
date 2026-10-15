use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde_json::json;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use zvs_contract::{AppError, AppErrorCode, DeviceGpuDto, DeviceProfileDto, DeviceTier};

use super::clock::SharedClock;
use super::gpu::{SharedGpuProbe, fit_utf16, select_gpu};
use super::logger::{LogLevel, SharedLogger, silent_logger};

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceReadings {
    pub platform: String,
    pub arch: String,
    pub cpu_model: String,
    pub cpu_cores: u64,
    pub total_memory_bytes: u64,
    pub free_memory_bytes: u64,
}

pub fn node_platform(os: &str) -> &str {
    match os {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

pub fn node_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "powerpc64" => "ppc64",
        other => other,
    }
}

pub fn os_readings() -> DeviceReadings {
    let system = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    let model = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim())
        .filter(|brand| !brand.is_empty())
        .unwrap_or("unknown");
    DeviceReadings {
        platform: fit_utf16(node_platform(std::env::consts::OS), 32),
        arch: fit_utf16(node_arch(std::env::consts::ARCH), 32),
        cpu_model: fit_utf16(model, 200),
        cpu_cores: system.cpus().len() as u64,
        total_memory_bytes: system.total_memory(),
        free_memory_bytes: system.available_memory(),
    }
}

pub fn device_tier(readings: &DeviceReadings, gpu: Option<&DeviceGpuDto>) -> DeviceTier {
    let memory_gib = readings.total_memory_bytes as f64 / GIB;
    let base = if memory_gib >= 30.0 && readings.cpu_cores >= 12 {
        DeviceTier::High
    } else if memory_gib >= 14.0 && readings.cpu_cores >= 6 {
        DeviceTier::Medium
    } else {
        DeviceTier::Low
    };
    if !gpu.is_some_and(|gpu| gpu.discrete) {
        return base;
    }
    match base {
        DeviceTier::Low => DeviceTier::Medium,
        DeviceTier::Medium | DeviceTier::High => DeviceTier::High,
    }
}

pub type ReadingsSource = Arc<dyn Fn() -> DeviceReadings + Send + Sync>;
pub type FreeDiskSource = Arc<dyn Fn() -> Result<u64, String> + Send + Sync>;

#[derive(Clone)]
pub struct DeviceProbeOptions {
    pub readings: ReadingsSource,
    pub gpu: Option<SharedGpuProbe>,
    pub free_disk: Option<FreeDiskSource>,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl DeviceProbeOptions {
    pub fn new(clock: SharedClock) -> Self {
        Self {
            readings: Arc::new(os_readings),
            gpu: None,
            free_disk: None,
            clock,
            logger: silent_logger(),
        }
    }
}

pub struct DeviceProbe {
    options: DeviceProbeOptions,
    cached: tokio::sync::Mutex<Option<DeviceProfileDto>>,
}

impl DeviceProbe {
    pub fn new(options: DeviceProbeOptions) -> Self {
        Self {
            options,
            cached: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn profile(&self, refresh: bool) -> Result<DeviceProfileDto, AppError> {
        let mut cached = self.cached.lock().await;
        if !refresh && let Some(profile) = cached.as_ref() {
            return Ok(profile.clone());
        }
        let options = self.options.clone();
        let profile = tokio::task::spawn_blocking(move || measure(&options))
            .await
            .map_err(|_| {
                AppError::new(
                    AppErrorCode::NativeError,
                    "Не удалось определить параметры устройства",
                )
            })?;
        *cached = Some(profile.clone());
        Ok(profile)
    }
}

fn gpu(options: &DeviceProbeOptions) -> Option<DeviceGpuDto> {
    let probe = options.gpu.as_ref()?;
    let adapters = catch_unwind(AssertUnwindSafe(|| probe.adapters()))
        .unwrap_or_else(|_| Err("the GPU probe panicked".to_owned()));
    match adapters {
        Ok(adapters) => select_gpu(&adapters),
        Err(error) => {
            options.logger.log(
                LogLevel::Warn,
                "system",
                "Could not read the GPU information",
                &[("error", json!(error))],
            );
            None
        }
    }
}

fn free_disk(options: &DeviceProbeOptions) -> u64 {
    let Some(source) = options.free_disk.as_ref() else {
        return 0;
    };
    catch_unwind(AssertUnwindSafe(|| source()))
        .ok()
        .and_then(Result::ok)
        .unwrap_or(0)
}

fn measure(options: &DeviceProbeOptions) -> DeviceProfileDto {
    let readings = (options.readings)();
    let gpu = gpu(options);
    let free_disk_bytes = free_disk(options);
    let tier = device_tier(&readings, gpu.as_ref());
    DeviceProfileDto {
        platform: readings.platform,
        arch: readings.arch,
        cpu_model: readings.cpu_model,
        cpu_cores: readings.cpu_cores,
        total_memory_bytes: readings.total_memory_bytes,
        free_memory_bytes: readings.free_memory_bytes,
        free_disk_bytes,
        gpu,
        tier,
        measured_at: options.clock.now(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use zvs_contract::Validate;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::gpu::{GpuAdapter, GpuProbe, VENDOR_NVIDIA};
    use crate::testing::MemoryLogger;

    const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

    fn readings() -> DeviceReadings {
        DeviceReadings {
            platform: "win32".to_owned(),
            arch: "x64".to_owned(),
            cpu_model: "Test CPU".to_owned(),
            cpu_cores: 8,
            total_memory_bytes: 16 * BYTES_PER_GIB,
            free_memory_bytes: 4 * BYTES_PER_GIB,
        }
    }

    fn gpu(vendor: &str, discrete: bool) -> DeviceGpuDto {
        DeviceGpuDto {
            vendor: vendor.to_owned(),
            model: "RTX".to_owned(),
            vram_bytes: None,
            discrete,
        }
    }

    #[test]
    fn the_tier_follows_memory_and_cores_and_a_discrete_gpu_lifts_it_one_step() {
        let discrete = gpu("NVIDIA", true);
        let integrated = gpu("Intel", false);
        assert_eq!(device_tier(&readings(), None), DeviceTier::Medium);
        assert_eq!(
            device_tier(&readings(), Some(&integrated)),
            DeviceTier::Medium
        );
        assert_eq!(device_tier(&readings(), Some(&discrete)), DeviceTier::High);
        let weak = DeviceReadings {
            cpu_cores: 4,
            total_memory_bytes: 8 * BYTES_PER_GIB,
            ..readings()
        };
        assert_eq!(device_tier(&weak, None), DeviceTier::Low);
        assert_eq!(device_tier(&weak, Some(&discrete)), DeviceTier::Medium);
        let strong = DeviceReadings {
            cpu_cores: 16,
            total_memory_bytes: 64 * BYTES_PER_GIB,
            ..readings()
        };
        assert_eq!(device_tier(&strong, None), DeviceTier::High);
        assert_eq!(device_tier(&strong, Some(&discrete)), DeviceTier::High);
    }

    #[test]
    fn rust_target_names_read_like_node_platform_names() {
        assert_eq!(node_platform("windows"), "win32");
        assert_eq!(node_platform("macos"), "darwin");
        assert_eq!(node_platform("linux"), "linux");
        assert_eq!(node_arch("x86_64"), "x64");
        assert_eq!(node_arch("aarch64"), "arm64");
        assert_eq!(node_arch("x86"), "ia32");
    }

    struct FailingGpu;

    impl GpuProbe for FailingGpu {
        fn adapters(&self) -> Result<Vec<GpuAdapter>, String> {
            Err("no GPU service".to_owned())
        }
    }

    struct OneCard;

    impl GpuProbe for OneCard {
        fn adapters(&self) -> Result<Vec<GpuAdapter>, String> {
            Ok(vec![GpuAdapter {
                vendor_id: VENDOR_NVIDIA,
                device_id: 0x2786,
                description: "NVIDIA GeForce RTX 4070".to_owned(),
                dedicated_video_memory: 12 * BYTES_PER_GIB,
                software: false,
            }])
        }
    }

    #[tokio::test]
    async fn the_profile_is_measured_once_and_a_failing_probe_costs_only_its_field() {
        let clock = Arc::new(FakeClock::default());
        let reads = Arc::new(AtomicUsize::new(0));
        let logger = MemoryLogger::shared();
        let probe = DeviceProbe::new(DeviceProbeOptions {
            readings: {
                let reads = Arc::clone(&reads);
                Arc::new(move || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    readings()
                })
            },
            gpu: Some(Arc::new(FailingGpu)),
            free_disk: Some(Arc::new(|| Ok(123))),
            clock: clock.clone(),
            logger: logger.clone(),
        });
        let first = probe.profile(false).await.expect("a profile");
        assert_eq!(first.gpu, None);
        assert_eq!(first.free_disk_bytes, 123);
        assert_eq!(first.tier, DeviceTier::Medium);
        assert_eq!(first.cpu_model, "Test CPU");
        assert!(first.validate().is_ok());
        assert_eq!(
            logger.matching("Could not read the GPU information").len(),
            1
        );

        clock.advance(1);
        assert_eq!(
            probe.profile(false).await.expect("cached").measured_at,
            first.measured_at
        );
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_ne!(
            probe.profile(true).await.expect("refreshed").measured_at,
            first.measured_at
        );
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_discrete_card_reports_real_vram_and_lifts_the_tier() {
        let probe = DeviceProbe::new(DeviceProbeOptions {
            readings: Arc::new(readings),
            gpu: Some(Arc::new(OneCard)),
            free_disk: Some(Arc::new(|| Err("statfs failed".to_owned()))),
            ..DeviceProbeOptions::new(Arc::new(FakeClock::default()))
        });
        let profile = probe.profile(false).await.expect("a profile");
        let gpu = profile.gpu.expect("a gpu");
        assert_eq!(gpu.vram_bytes, Some(12 * BYTES_PER_GIB));
        assert!(gpu.discrete);
        assert_eq!(profile.tier, DeviceTier::High);
        assert_eq!(profile.free_disk_bytes, 0);
    }

    #[tokio::test]
    async fn the_real_machine_reads_as_a_valid_profile() {
        let probe = DeviceProbe::new(DeviceProbeOptions::new(Arc::new(FakeClock::default())));
        let profile = probe.profile(false).await.expect("a profile");
        assert!(profile.cpu_cores > 0);
        assert!(profile.total_memory_bytes >= profile.free_memory_bytes);
        assert_eq!(profile.platform, node_platform(std::env::consts::OS));
        assert!(profile.validate().is_ok());
    }
}
