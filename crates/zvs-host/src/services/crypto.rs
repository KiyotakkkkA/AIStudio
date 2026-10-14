use std::fmt;
use std::sync::Arc;

use zeroize::Zeroizing;
use zvs_contract::{AppError, AppErrorCode, SecretInput};

pub const UNAVAILABLE_MESSAGE: &str = "Хранилище ключей операционной системы недоступно, поэтому секрет нельзя ни зашифровать, ни расшифровать. Открытые значения не сохраняются никогда.";
pub const ENCRYPT_FAILED_MESSAGE: &str = "Не удалось зашифровать секрет";
pub const DECRYPT_FAILED_MESSAGE: &str = "Не удалось расшифровать секрет";

#[derive(Clone, PartialEq, Eq)]
pub struct Plaintext(Zeroizing<String>);

impl Plaintext {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<&SecretInput> for Plaintext {
    fn from(input: &SecretInput) -> Self {
        Self::new(input.expose())
    }
}

impl fmt::Debug for Plaintext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Plaintext([redacted])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct BackendFailure(pub String);

impl BackendFailure {
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }
}

pub trait CryptoBackend: Send + Sync {
    fn is_available(&self) -> bool;

    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, BackendFailure>;

    fn decrypt(&self, cipher: &[u8]) -> Result<Plaintext, BackendFailure>;
}

pub type SharedCryptoBackend = Arc<dyn CryptoBackend>;

#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableBackend;

impl CryptoBackend for UnavailableBackend {
    fn is_available(&self) -> bool {
        false
    }

    fn encrypt(&self, _plaintext: &str) -> Result<Vec<u8>, BackendFailure> {
        Err(BackendFailure::new("no verified keychain on this platform"))
    }

    fn decrypt(&self, _cipher: &[u8]) -> Result<Plaintext, BackendFailure> {
        Err(BackendFailure::new("no verified keychain on this platform"))
    }
}

pub struct CryptoService {
    backend: SharedCryptoBackend,
}

impl CryptoService {
    pub fn new(backend: SharedCryptoBackend) -> Self {
        Self { backend }
    }

    pub fn is_available(&self) -> bool {
        self.backend.is_available()
    }

    pub fn encrypt(&self, plaintext: &Plaintext) -> Result<Vec<u8>, AppError> {
        self.require_backend()?;
        self.backend
            .encrypt(plaintext.expose())
            .map_err(|_| AppError::new(AppErrorCode::SecretDecryptFailed, ENCRYPT_FAILED_MESSAGE))
    }

    pub fn decrypt(&self, cipher: &[u8]) -> Result<Plaintext, AppError> {
        self.require_backend()?;
        self.backend
            .decrypt(cipher)
            .map_err(|_| AppError::new(AppErrorCode::SecretDecryptFailed, DECRYPT_FAILED_MESSAGE))
    }

    fn require_backend(&self) -> Result<(), AppError> {
        if self.is_available() {
            Ok(())
        } else {
            Err(AppError::new(
                AppErrorCode::SecretDecryptFailed,
                UNAVAILABLE_MESSAGE,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeCrypto;

    #[test]
    fn a_working_backend_round_trips_and_counts_its_calls() {
        let backend = FakeCrypto::shared();
        let crypto = CryptoService::new(backend.clone());
        assert!(crypto.is_available());
        let cipher = crypto
            .encrypt(&Plaintext::new("osk_live_value"))
            .expect("encrypts");
        assert_ne!(cipher, b"osk_live_value");
        assert_eq!(
            crypto.decrypt(&cipher).expect("decrypts").expose(),
            "osk_live_value"
        );
        assert_eq!((backend.encrypt_calls(), backend.decrypt_calls()), (1, 1));
    }

    #[test]
    fn an_unavailable_backend_is_refused_before_it_is_called() {
        let backend = FakeCrypto::shared();
        backend.set_available(false);
        let crypto = CryptoService::new(backend.clone());
        assert!(!crypto.is_available());
        for error in [
            crypto.encrypt(&Plaintext::new("x")).expect_err("refuses"),
            crypto.decrypt(b"x").expect_err("refuses"),
        ] {
            assert_eq!(error.code, AppErrorCode::SecretDecryptFailed);
            assert_eq!(error.message, UNAVAILABLE_MESSAGE);
        }
        assert_eq!((backend.encrypt_calls(), backend.decrypt_calls()), (0, 0));
    }

    #[test]
    fn a_failing_backend_maps_to_secret_decrypt_failed_with_the_direction_named() {
        let backend = FakeCrypto::shared();
        let crypto = CryptoService::new(backend.clone());
        backend.fail_next();
        let error = crypto.encrypt(&Plaintext::new("x")).expect_err("fails");
        assert_eq!(
            (error.code, error.message.as_str()),
            (AppErrorCode::SecretDecryptFailed, ENCRYPT_FAILED_MESSAGE)
        );
        backend.fail_next();
        let error = crypto.decrypt(b"x").expect_err("fails");
        assert_eq!(
            (error.code, error.message.as_str()),
            (AppErrorCode::SecretDecryptFailed, DECRYPT_FAILED_MESSAGE)
        );
        assert_eq!(error.details, None);
    }

    #[test]
    fn the_platform_fallback_reports_itself_unavailable_and_never_stores_plaintext() {
        let crypto = CryptoService::new(Arc::new(UnavailableBackend));
        assert!(!crypto.is_available());
        assert!(UnavailableBackend.encrypt("x").is_err());
        assert!(UnavailableBackend.decrypt(b"x").is_err());
        assert_eq!(
            crypto
                .encrypt(&Plaintext::new("x"))
                .expect_err("refuses")
                .message,
            UNAVAILABLE_MESSAGE
        );
    }

    #[test]
    fn a_plaintext_never_prints_its_value() {
        let plaintext = Plaintext::new("sk-or-v1-planted");
        assert_eq!(format!("{plaintext:?}"), "Plaintext([redacted])");
        assert_eq!(
            format!("{:?}", Some(&plaintext)),
            "Some(Plaintext([redacted]))"
        );
    }
}
