#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
mod site_stub;

use std::sync::Arc;

use zvs_host::platform::gpu::SharedGpuProbe;
use zvs_host::services::SharedCryptoBackend;

#[cfg(windows)]
pub use windows::webview2 as site;

#[cfg(not(windows))]
pub use site_stub as site;

#[cfg(windows)]
pub fn crypto_backend() -> SharedCryptoBackend {
    Arc::new(windows::dpapi::DpapiBackend::probe())
}

#[cfg(not(windows))]
pub fn crypto_backend() -> SharedCryptoBackend {
    Arc::new(zvs_host::services::UnavailableBackend)
}

#[cfg(windows)]
pub fn gpu_probe() -> SharedGpuProbe {
    Arc::new(windows::dxgi::DxgiProbe)
}

#[cfg(not(windows))]
pub fn gpu_probe() -> SharedGpuProbe {
    Arc::new(zvs_host::platform::gpu::NoGpu)
}
