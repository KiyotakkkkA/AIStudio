use regex::Regex;
use zvs_contract::{Accelerator, DeviceProfileDto, ModelFormat, RuntimeEngine};

use crate::platform::device::{node_arch, node_platform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: &'static str,
    pub arch: &'static str,
}

impl Platform {
    pub fn current() -> Self {
        Self {
            os: node_platform(std::env::consts::OS),
            arch: node_arch(std::env::consts::ARCH),
        }
    }

    pub const fn new(os: &'static str, arch: &'static str) -> Self {
        Self { os, arch }
    }
}

#[derive(Debug, Clone)]
pub struct AssetPlan {
    pub primary: Regex,
    companion: Option<String>,
}

impl AssetPlan {
    fn new(primary: &str) -> Self {
        Self {
            primary: Regex::new(primary).expect("a valid asset pattern"),
            companion: None,
        }
    }

    fn with_companion(primary: &str, companion: &str) -> Self {
        Self {
            companion: Some(companion.to_owned()),
            ..Self::new(primary)
        }
    }

    pub fn companion(&self, variant: &str) -> Option<Regex> {
        self.companion.as_ref().map(|template| {
            Regex::new(&template.replace("{variant}", &variant.replace('.', r"\.")))
                .expect("a valid companion pattern")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDefinition {
    pub id: &'static str,
    pub engine: RuntimeEngine,
    pub accelerator: Accelerator,
    pub display_name: &'static str,
    pub description: &'static str,
    pub formats: &'static [ModelFormat],
    pub approximate_bytes: u64,
}

fn arch_suffix(arch: &str) -> Option<&'static str> {
    match arch {
        "x64" => Some("x64"),
        "arm64" => Some("arm64"),
        _ => None,
    }
}

fn llama_asset(platform: Platform, backend: &str) -> Option<String> {
    let suffix = arch_suffix(platform.arch)?;
    match platform.os {
        "win32" => Some(format!(r"^llama-b\d+-bin-win-{backend}-{suffix}\.zip$")),
        "linux" => Some(format!(
            r"^llama-b\d+-bin-ubuntu-{backend}-{suffix}\.tar\.gz$"
        )),
        _ => None,
    }
}

impl RuntimeDefinition {
    pub fn assets(&self, platform: Platform) -> Option<AssetPlan> {
        match self.id {
            "llama-cpp-cpu" => match platform.os {
                "darwin" => arch_suffix(platform.arch).map(|suffix| {
                    AssetPlan::new(&format!(r"^llama-b\d+-bin-macos-{suffix}\.tar\.gz$"))
                }),
                "linux" => arch_suffix(platform.arch).map(|suffix| {
                    AssetPlan::new(&format!(r"^llama-b\d+-bin-ubuntu-{suffix}\.tar\.gz$"))
                }),
                _ => llama_asset(platform, "cpu").map(|primary| AssetPlan::new(&primary)),
            },
            "llama-cpp-vulkan" => {
                llama_asset(platform, "vulkan").map(|primary| AssetPlan::new(&primary))
            }
            "llama-cpp-cuda" => {
                let suffix = arch_suffix(platform.arch)?;
                match platform.os {
                    "win32" => Some(AssetPlan::with_companion(
                        &format!(r"^llama-b\d+-bin-win-cuda-([\d.]+)-{suffix}\.zip$"),
                        &format!(r"^cudart-llama-bin-win-cuda-{{variant}}-{suffix}\.zip$"),
                    )),
                    "linux" => Some(AssetPlan::with_companion(
                        &format!(r"^llama-b\d+-bin-ubuntu-cuda-([\d.]+)-{suffix}\.tar\.gz$"),
                        &format!(
                            r"^cudart-llama-b\d+-bin-ubuntu-cuda-{{variant}}-{suffix}\.tar\.gz$"
                        ),
                    )),
                    _ => None,
                }
            }
            "llama-cpp-metal" => (platform.os == "darwin" && platform.arch == "arm64")
                .then(|| AssetPlan::new(r"^llama-b\d+-bin-macos-arm64\.tar\.gz$")),
            _ => None,
        }
    }

    pub fn supported(&self, platform: Platform) -> bool {
        self.assets(platform).is_some()
    }
}

pub const RUNTIME_DEFINITIONS: &[RuntimeDefinition] = &[
    RuntimeDefinition {
        id: "llama-cpp-cpu",
        engine: RuntimeEngine::LlamaCpp,
        accelerator: Accelerator::Cpu,
        display_name: "llama.cpp · CPU",
        description: "Движок для моделей в формате GGUF, считает на процессоре. Работает на любой машине, но эмбеддинги большого корпуса займут заметно больше времени.",
        formats: &[ModelFormat::Gguf],
        approximate_bytes: 19_000_000,
    },
    RuntimeDefinition {
        id: "llama-cpp-vulkan",
        engine: RuntimeEngine::LlamaCpp,
        accelerator: Accelerator::Vulkan,
        display_name: "llama.cpp · Vulkan (GPU)",
        description: "Тот же движок с расчётом на видеокарте через Vulkan. Подходит NVIDIA, AMD и встроенной графике Intel, не требует установки CUDA и весит около 30 МБ. Рекомендуемый вариант для любой дискретной видеокарты.",
        formats: &[ModelFormat::Gguf],
        approximate_bytes: 32_000_000,
    },
    RuntimeDefinition {
        id: "llama-cpp-cuda",
        engine: RuntimeEngine::LlamaCpp,
        accelerator: Accelerator::Cuda,
        display_name: "llama.cpp · CUDA (NVIDIA)",
        description: "Сборка под CUDA: самая быстрая на видеокартах NVIDIA. Скачивается вместе с библиотеками CUDA, поэтому занимает несколько сотен мегабайт. Имеет смысл, только если карта — NVIDIA.",
        formats: &[ModelFormat::Gguf],
        approximate_bytes: 545_000_000,
    },
    RuntimeDefinition {
        id: "llama-cpp-metal",
        engine: RuntimeEngine::LlamaCpp,
        accelerator: Accelerator::Metal,
        display_name: "llama.cpp · Metal (Apple)",
        description: "Сборка для Apple Silicon: расчёт идёт на встроенном графическом ядре через Metal. Отдельно ничего ставить не нужно.",
        formats: &[ModelFormat::Gguf],
        approximate_bytes: 12_000_000,
    },
];

pub fn definition_of(id: &str) -> Option<&'static RuntimeDefinition> {
    RUNTIME_DEFINITIONS
        .iter()
        .find(|definition| definition.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceleratorPlan {
    pub accelerator: Accelerator,
    pub reason: String,
}

pub fn plan_accelerator(device: Option<&DeviceProfileDto>, platform: Platform) -> AcceleratorPlan {
    if platform.os == "darwin" && platform.arch == "arm64" {
        return AcceleratorPlan {
            accelerator: Accelerator::Metal,
            reason: "Apple Silicon — расчёт идёт через Metal.".to_owned(),
        };
    }
    let Some(gpu) = device.and_then(|device| device.gpu.as_ref()) else {
        return AcceleratorPlan {
            accelerator: Accelerator::Cpu,
            reason: "Видеокарту определить не удалось, поэтому предлагается сборка для процессора."
                .to_owned(),
        };
    };
    if !gpu.discrete {
        return AcceleratorPlan {
            accelerator: Accelerator::Cpu,
            reason: format!(
                "Обнаружена встроенная графика ({}), ускорение даст немного — предлагается сборка для процессора.",
                gpu.vendor
            ),
        };
    }
    AcceleratorPlan {
        accelerator: Accelerator::Vulkan,
        reason: format!(
            "Обнаружена дискретная видеокарта {} {}. Vulkan ускоряет расчёт и не требует CUDA.",
            gpu.vendor, gpu.model
        ),
    }
}

pub fn format_of(file_name: &str) -> Option<ModelFormat> {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".gguf") {
        Some(ModelFormat::Gguf)
    } else if lower.ends_with(".onnx") {
        Some(ModelFormat::Onnx)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::{DeviceGpuDto, DeviceTier};

    use super::*;

    fn device(gpu: Option<DeviceGpuDto>) -> DeviceProfileDto {
        DeviceProfileDto {
            platform: "win32".to_owned(),
            arch: "x64".to_owned(),
            cpu_model: "Test CPU".to_owned(),
            cpu_cores: 16,
            total_memory_bytes: 32 << 30,
            free_memory_bytes: 16 << 30,
            free_disk_bytes: 500 << 30,
            gpu,
            tier: DeviceTier::High,
            measured_at: 1,
        }
    }

    fn gpu(vendor: &str, model: &str, discrete: bool) -> Option<DeviceGpuDto> {
        Some(DeviceGpuDto {
            vendor: vendor.to_owned(),
            model: model.to_owned(),
            vram_bytes: None,
            discrete,
        })
    }

    const WINDOWS: Platform = Platform::new("win32", "x64");

    #[test]
    fn the_recommended_build_follows_what_was_detected_and_says_why() {
        assert_eq!(
            plan_accelerator(Some(&device(None)), WINDOWS).accelerator,
            Accelerator::Cpu
        );
        let integrated = plan_accelerator(Some(&device(gpu("Intel", "UHD 770", false))), WINDOWS);
        assert_eq!(integrated.accelerator, Accelerator::Cpu);
        assert!(integrated.reason.contains("Intel"));
        let discrete = plan_accelerator(Some(&device(gpu("NVIDIA", "RTX 4070", true))), WINDOWS);
        assert_eq!(discrete.accelerator, Accelerator::Vulkan);
        assert!(discrete.reason.contains("RTX 4070"));
        assert_eq!(
            plan_accelerator(None, Platform::new("darwin", "arm64")).accelerator,
            Accelerator::Metal
        );
        assert_eq!(
            plan_accelerator(None, WINDOWS).accelerator,
            Accelerator::Cpu
        );
    }

    #[test]
    fn only_the_builds_that_exist_for_a_platform_are_offered() {
        let offered = |platform: Platform| -> Vec<&str> {
            RUNTIME_DEFINITIONS
                .iter()
                .filter(|definition| definition.supported(platform))
                .map(|definition| definition.id)
                .collect()
        };
        assert_eq!(
            offered(WINDOWS),
            ["llama-cpp-cpu", "llama-cpp-vulkan", "llama-cpp-cuda"]
        );
        assert_eq!(
            offered(Platform::new("darwin", "arm64")),
            ["llama-cpp-cpu", "llama-cpp-metal"]
        );
        assert_eq!(
            offered(Platform::new("linux", "x64")),
            ["llama-cpp-cpu", "llama-cpp-vulkan", "llama-cpp-cuda"]
        );
        assert!(offered(Platform::new("win32", "ia32")).is_empty());
        assert!(offered(Platform::new("freebsd", "x64")).is_empty());
    }

    #[test]
    fn the_asset_patterns_match_the_upstream_names_and_pin_the_cuda_variant() {
        let vulkan = definition_of("llama-cpp-vulkan")
            .and_then(|definition| definition.assets(WINDOWS))
            .expect("a plan");
        assert!(
            vulkan
                .primary
                .is_match("llama-b10970-bin-win-vulkan-x64.zip")
        );
        assert!(
            !vulkan
                .primary
                .is_match("llama-b10970-bin-win-vulkan-arm64.zip")
        );
        assert!(vulkan.companion("x").is_none());

        let cuda = definition_of("llama-cpp-cuda")
            .and_then(|definition| definition.assets(WINDOWS))
            .expect("a plan");
        let captured = cuda
            .primary
            .captures("llama-b10970-bin-win-cuda-13.3-x64.zip")
            .and_then(|captures| captures.get(1))
            .map(|variant| variant.as_str().to_owned());
        assert_eq!(captured.as_deref(), Some("13.3"));
        let companion = cuda.companion("13.3").expect("a companion");
        assert!(companion.is_match("cudart-llama-bin-win-cuda-13.3-x64.zip"));
        assert!(!companion.is_match("cudart-llama-bin-win-cuda-13x3-x64.zip"));
        assert!(!companion.is_match("cudart-llama-bin-win-cuda-12.4-x64.zip"));

        let linux_cpu = definition_of("llama-cpp-cpu")
            .and_then(|definition| definition.assets(Platform::new("linux", "x64")))
            .expect("a plan");
        assert!(linux_cpu.primary.is_match("llama-b1-bin-ubuntu-x64.tar.gz"));
    }

    #[test]
    fn a_file_format_is_read_from_its_extension() {
        assert_eq!(format_of("bge-m3.onnx"), Some(ModelFormat::Onnx));
        assert_eq!(format_of("bge-m3-FP16.GGUF"), Some(ModelFormat::Gguf));
        assert_eq!(format_of("notes.txt"), None);
        assert!(definition_of("llama-cpp-nope").is_none());
    }
}
