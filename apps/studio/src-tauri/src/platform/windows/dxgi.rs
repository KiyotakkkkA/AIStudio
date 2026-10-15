#![allow(unsafe_code)]

use ::windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
    IDXGIFactory1,
};
use zvs_host::platform::gpu::{GpuAdapter, GpuProbe};

#[derive(Debug, Clone, Copy, Default)]
pub struct DxgiProbe;

fn failure(step: &str, error: &::windows::core::Error) -> String {
    format!("DXGI {step} failed: {}", error.code())
}

fn adapter(desc: &DXGI_ADAPTER_DESC1) -> GpuAdapter {
    let length = desc
        .Description
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(desc.Description.len());
    GpuAdapter {
        vendor_id: desc.VendorId,
        device_id: desc.DeviceId,
        description: String::from_utf16_lossy(&desc.Description[..length]),
        dedicated_video_memory: desc.DedicatedVideoMemory as u64,
        software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0.cast_unsigned() != 0,
    }
}

impl GpuProbe for DxgiProbe {
    fn adapters(&self) -> Result<Vec<GpuAdapter>, String> {
        let factory: IDXGIFactory1 =
            unsafe { CreateDXGIFactory1() }.map_err(|error| failure("factory", &error))?;
        let mut adapters = Vec::new();
        for index in 0_u32.. {
            match unsafe { factory.EnumAdapters1(index) } {
                Ok(found) => {
                    let desc = unsafe { found.GetDesc1() }
                        .map_err(|error| failure("adapter description", &error))?;
                    adapters.push(adapter(&desc));
                }
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(failure("adapter enumeration", &error)),
            }
        }
        Ok(adapters)
    }
}

#[cfg(test)]
mod tests {
    use zvs_host::platform::gpu::{VENDOR_MICROSOFT, VENDOR_NVIDIA, select_gpu};

    use super::*;

    fn describe(name: &str, vendor: u32, dedicated: usize, flags: u32) -> DXGI_ADAPTER_DESC1 {
        let mut desc = DXGI_ADAPTER_DESC1 {
            VendorId: vendor,
            DeviceId: 7,
            DedicatedVideoMemory: dedicated,
            Flags: flags,
            ..DXGI_ADAPTER_DESC1::default()
        };
        for (slot, unit) in desc.Description.iter_mut().zip(name.encode_utf16()) {
            *slot = unit;
        }
        desc
    }

    #[test]
    fn a_description_is_read_up_to_its_terminator_and_flags_mark_software() {
        let card = adapter(&describe(
            "NVIDIA GeForce RTX 4070",
            VENDOR_NVIDIA,
            1 << 33,
            0,
        ));
        assert_eq!(card.description, "NVIDIA GeForce RTX 4070");
        assert_eq!(card.dedicated_video_memory, 1 << 33);
        assert!(!card.software);
        let basic = adapter(&describe(
            "Microsoft Basic Render Driver",
            VENDOR_MICROSOFT,
            0,
            DXGI_ADAPTER_FLAG_SOFTWARE.0.cast_unsigned(),
        ));
        assert!(basic.software);
        let full = adapter(&describe(&"x".repeat(200), VENDOR_NVIDIA, 0, 0));
        assert_eq!(full.description.len(), 128);
    }

    #[test]
    fn the_real_adapters_are_enumerated() {
        let adapters = DxgiProbe.adapters().expect("DXGI answers");
        assert!(!adapters.is_empty());
        if let Some(gpu) = select_gpu(&adapters) {
            assert_ne!(gpu.model, "");
        }
    }
}
