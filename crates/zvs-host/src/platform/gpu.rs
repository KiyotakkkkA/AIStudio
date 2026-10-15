use std::sync::Arc;

use zvs_contract::DeviceGpuDto;

pub const VENDOR_NVIDIA: u32 = 0x10de;
pub const VENDOR_AMD: u32 = 0x1002;
pub const VENDOR_INTEL: u32 = 0x8086;
pub const VENDOR_APPLE: u32 = 0x106b;
pub const VENDOR_QUALCOMM: u32 = 0x4d4f_4351;
pub const VENDOR_MICROSOFT: u32 = 0x1414;

pub const DISCRETE_VRAM_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_MODEL_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuAdapter {
    pub vendor_id: u32,
    pub device_id: u32,
    pub description: String,
    pub dedicated_video_memory: u64,
    pub software: bool,
}

pub trait GpuProbe: Send + Sync {
    fn adapters(&self) -> Result<Vec<GpuAdapter>, String>;
}

pub type SharedGpuProbe = Arc<dyn GpuProbe>;

#[derive(Debug, Clone, Copy, Default)]
pub struct NoGpu;

impl GpuProbe for NoGpu {
    fn adapters(&self) -> Result<Vec<GpuAdapter>, String> {
        Ok(Vec::new())
    }
}

pub fn fit_utf16(text: &str, max: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|character| {
            used += character.len_utf16();
            used <= max
        })
        .collect()
}

pub fn vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        VENDOR_NVIDIA => "NVIDIA",
        VENDOR_AMD => "AMD",
        VENDOR_INTEL => "Intel",
        VENDOR_APPLE => "Apple",
        VENDOR_QUALCOMM => "Qualcomm",
        _ => "unknown",
    }
}

pub fn select_gpu(adapters: &[GpuAdapter]) -> Option<DeviceGpuDto> {
    let hardware: Vec<&GpuAdapter> = adapters
        .iter()
        .filter(|adapter| !adapter.software && adapter.vendor_id != VENDOR_MICROSOFT)
        .collect();
    let chosen = hardware.iter().copied().reduce(|best, candidate| {
        if candidate.dedicated_video_memory > best.dedicated_video_memory {
            candidate
        } else {
            best
        }
    })?;
    let hybrid = hardware
        .iter()
        .any(|other| other.dedicated_video_memory < chosen.dedicated_video_memory);
    let discrete = chosen.vendor_id == VENDOR_NVIDIA
        || chosen.dedicated_video_memory >= DISCRETE_VRAM_BYTES
        || hybrid;
    let model = fit_utf16(chosen.description.trim(), MAX_MODEL_CHARS);
    Some(DeviceGpuDto {
        vendor: vendor_name(chosen.vendor_id).to_owned(),
        model: if model.is_empty() {
            "unknown".to_owned()
        } else {
            model
        },
        vram_bytes: (chosen.dedicated_video_memory > 0).then_some(chosen.dedicated_video_memory),
        discrete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn adapter(vendor_id: u32, description: &str, dedicated: u64) -> GpuAdapter {
        GpuAdapter {
            vendor_id,
            device_id: 1,
            description: description.to_owned(),
            dedicated_video_memory: dedicated,
            software: false,
        }
    }

    fn basic_render() -> GpuAdapter {
        GpuAdapter {
            software: true,
            ..adapter(VENDOR_MICROSOFT, "Microsoft Basic Render Driver", 0)
        }
    }

    #[test]
    fn no_hardware_adapter_means_no_gpu() {
        assert_eq!(select_gpu(&[]), None);
        assert_eq!(select_gpu(&[basic_render()]), None);
        assert!(NoGpu.adapters().expect("lists").is_empty());
    }

    #[test]
    fn a_discrete_card_reports_its_dedicated_memory() {
        let gpu = select_gpu(&[
            adapter(VENDOR_NVIDIA, "  NVIDIA GeForce RTX 4070  ", 12 * GIB),
            basic_render(),
        ])
        .expect("a gpu");
        assert_eq!(
            gpu,
            DeviceGpuDto {
                vendor: "NVIDIA".to_owned(),
                model: "NVIDIA GeForce RTX 4070".to_owned(),
                vram_bytes: Some(12 * GIB),
                discrete: true,
            }
        );
    }

    #[test]
    fn a_hybrid_laptop_picks_the_card_with_more_memory_even_when_it_is_listed_second() {
        let gpu = select_gpu(&[
            adapter(VENDOR_INTEL, "Intel(R) UHD Graphics", 128 * MIB),
            adapter(VENDOR_NVIDIA, "NVIDIA GeForce RTX 3050 Laptop GPU", 4 * GIB),
        ])
        .expect("a gpu");
        assert_eq!(gpu.vendor, "NVIDIA");
        assert_eq!(gpu.vram_bytes, Some(4 * GIB));
        assert!(gpu.discrete);

        let amd = select_gpu(&[
            adapter(VENDOR_INTEL, "Intel(R) Iris(R) Xe Graphics", 128 * MIB),
            adapter(VENDOR_AMD, "AMD Radeon RX 6500M", GIB),
        ])
        .expect("a gpu");
        assert_eq!(amd.vendor, "AMD");
        assert!(amd.discrete);
    }

    #[test]
    fn integrated_graphics_alone_are_not_discrete() {
        for (vendor, name, memory) in [
            (VENDOR_INTEL, "Intel(R) UHD Graphics 770", 128 * MIB),
            (VENDOR_AMD, "AMD Radeon(TM) Graphics", 512 * MIB),
            (VENDOR_QUALCOMM, "Qualcomm(R) Adreno(TM) X1-85 GPU", 0),
        ] {
            let gpu = select_gpu(&[adapter(vendor, name, memory), basic_render()]).expect("a gpu");
            assert!(!gpu.discrete, "{name}");
            assert_eq!(gpu.vram_bytes, (memory > 0).then_some(memory));
        }
    }

    #[test]
    fn a_large_card_of_any_vendor_is_discrete_and_ties_keep_the_first() {
        let arc =
            select_gpu(&[adapter(VENDOR_INTEL, "Intel(R) Arc(TM) A770", 16 * GIB)]).expect("a gpu");
        assert!(arc.discrete);
        let twins = select_gpu(&[
            adapter(VENDOR_AMD, "First", 8 * GIB),
            adapter(VENDOR_AMD, "Second", 8 * GIB),
        ])
        .expect("a gpu");
        assert_eq!(twins.model, "First");
    }

    #[test]
    fn unknown_vendors_and_blank_or_long_names_stay_within_the_contract() {
        let gpu = select_gpu(&[adapter(0xabcd, "   ", 0)]).expect("a gpu");
        assert_eq!(gpu.vendor, "unknown");
        assert_eq!(gpu.model, "unknown");
        assert_eq!(gpu.vram_bytes, None);
        let long = select_gpu(&[adapter(VENDOR_NVIDIA, &"ж".repeat(300), GIB)]).expect("a gpu");
        assert_eq!(long.model.chars().count(), 200);
    }
}
