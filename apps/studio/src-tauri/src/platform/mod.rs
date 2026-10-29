#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
mod site_stub;

use std::path::Path;
use std::sync::Arc;

use zvs_host::platform::external::SharedFileLauncher;
use zvs_host::platform::gpu::SharedGpuProbe;
use zvs_host::platform::logger::SharedLogger;
use zvs_host::services::SharedCryptoBackend;

use crate::toast::{ActivationSlot, Toaster};

#[cfg(windows)]
pub use windows::webview2 as site;

#[cfg(not(windows))]
pub use site_stub as site;

#[cfg(windows)]
pub fn cli_installer(binary: std::path::PathBuf) -> zvs_host::cli::SharedCliInstaller {
    Arc::new(windows::cli_path::UserPathInstaller::new(binary))
}

#[cfg(not(windows))]
pub fn cli_installer(_binary: std::path::PathBuf) -> zvs_host::cli::SharedCliInstaller {
    Arc::new(zvs_host::cli::NoCliInstaller)
}

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

#[cfg(windows)]
pub fn file_launcher() -> SharedFileLauncher {
    Arc::new(windows::assoc::ShellLauncher)
}

#[cfg(not(windows))]
pub fn file_launcher() -> SharedFileLauncher {
    zvs_host::platform::external::system_file_launcher()
}

#[cfg(windows)]
pub fn toaster(
    app_id: &str,
    display_name: &str,
    icon: Option<&Path>,
    activation: ActivationSlot,
    logger: SharedLogger,
) -> Arc<dyn Toaster> {
    Arc::new(windows::toast::WindowsToaster::start(
        app_id,
        display_name,
        icon,
        activation,
        logger,
    ))
}

#[cfg(not(windows))]
pub fn toaster(
    _app_id: &str,
    _display_name: &str,
    _icon: Option<&Path>,
    _activation: ActivationSlot,
    _logger: SharedLogger,
) -> Arc<dyn Toaster> {
    Arc::new(crate::toast::NoToaster)
}
