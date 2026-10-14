#[cfg(windows)]
pub mod windows;

use std::sync::Arc;

use zvs_host::services::SharedCryptoBackend;

#[cfg(windows)]
pub fn crypto_backend() -> SharedCryptoBackend {
    Arc::new(windows::dpapi::DpapiBackend::probe())
}

#[cfg(not(windows))]
pub fn crypto_backend() -> SharedCryptoBackend {
    Arc::new(zvs_host::services::UnavailableBackend)
}
