use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use zvs_contract::{AppError, AppErrorCode};

use crate::services::SecretService;
use crate::services::crypto::Plaintext;
use crate::services::settings::SettingService;

pub const PACK_KEY_SETTING: &str = "exec.policy.packKey";
pub const MAC_EXTENSION: &str = "mac";
pub const KEY_UNAVAILABLE: &str = "Ключ подписи правил недоступен: хранилище ключей не отвечает";
const KEY_CONTEXT: &str = "zvs-ai-studio 2026-11 policy pack key";

#[derive(Clone, PartialEq, Eq)]
pub struct PackKey([u8; 32]);

impl std::fmt::Debug for PackKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PackKey([redacted])")
    }
}

impl PackKey {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn generate() -> Self {
        let mut seed = Vec::with_capacity(32);
        seed.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        seed.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self(blake3::derive_key(KEY_CONTEXT, &seed))
    }

    fn hex(&self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }

    fn from_hex(text: &str) -> Option<Self> {
        blake3::Hash::from_hex(text.trim())
            .ok()
            .map(|hash| Self(*hash.as_bytes()))
    }

    pub fn sign(&self, text: &[u8]) -> String {
        blake3::keyed_hash(&self.0, text).to_hex().to_string()
    }

    pub fn verify(&self, text: &[u8], mac: &str) -> bool {
        blake3::Hash::from_hex(mac.trim())
            .is_ok_and(|expected| blake3::keyed_hash(&self.0, text) == expected)
    }
}

pub trait PackKeys: Send + Sync {
    fn key(&self) -> Result<PackKey, AppError>;
}

impl PackKeys for PackKey {
    fn key(&self) -> Result<PackKey, AppError> {
        Ok(self.clone())
    }
}

pub type SharedPackKeys = Arc<dyn PackKeys>;

pub struct SealedPackKeys {
    settings: SettingService,
    secrets: Arc<SecretService>,
    cached: Mutex<Option<PackKey>>,
}

impl SealedPackKeys {
    pub fn new(settings: SettingService, secrets: Arc<SecretService>) -> Self {
        Self {
            settings,
            secrets,
            cached: Mutex::new(None),
        }
    }

    fn stored(&self) -> Result<Option<PackKey>, AppError> {
        let Some(setting) = self.settings.get(PACK_KEY_SETTING)? else {
            return Ok(None);
        };
        let Some(sealed) = setting
            .value
            .as_str()
            .and_then(|text| STANDARD.decode(text).ok())
        else {
            return Ok(None);
        };
        let opened = self.secrets.open(&sealed)?;
        Ok(PackKey::from_hex(opened.expose()))
    }

    fn created(&self) -> Result<PackKey, AppError> {
        let key = PackKey::generate();
        let sealed = self.secrets.seal(&Plaintext::new(key.hex()))?;
        self.settings
            .set(PACK_KEY_SETTING, &json!(STANDARD.encode(sealed)))?;
        Ok(key)
    }
}

impl PackKeys for SealedPackKeys {
    fn key(&self) -> Result<PackKey, AppError> {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(key) = cached.as_ref() {
            return Ok(key.clone());
        }
        if !self.secrets.is_encryption_available() {
            return Err(AppError::new(AppErrorCode::Unknown, KEY_UNAVAILABLE));
        }
        let key = match self.stored()? {
            Some(key) => key,
            None => self.created()?,
        };
        *cached = Some(key.clone());
        Ok(key)
    }
}

pub fn mac_file(pack: &Path) -> PathBuf {
    let mut name = pack.as_os_str().to_owned();
    name.push(".");
    name.push(MAC_EXTENSION);
    PathBuf::from(name)
}

pub fn signed(pack: &Path, text: &[u8], key: Option<&PackKey>) -> bool {
    let Some(key) = key else {
        return false;
    };
    std::fs::read_to_string(mac_file(pack)).is_ok_and(|mac| key.verify(text, &mac))
}

pub fn sign(pack: &Path, text: &[u8], key: &PackKey) -> std::io::Result<()> {
    let file = mac_file(pack);
    let staged = file.with_extension("mac.tmp");
    std::fs::write(&staged, format!("{}\n", key.sign(text)))?;
    std::fs::rename(&staged, &file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_verifies_only_the_signed_text_under_the_same_key() {
        let key = PackKey::new([7; 32]);
        let mac = key.sign(b"pack");
        assert!(key.verify(b"pack", &mac));
        assert!(!key.verify(b"pack ", &mac));
        assert!(!PackKey::new([8; 32]).verify(b"pack", &mac));
        assert!(!key.verify(b"pack", "not hex"));
    }

    #[test]
    fn a_pack_is_signed_beside_itself() {
        let folder = tempfile::tempdir().expect("dir");
        let pack = folder.path().join("user.json");
        let key = PackKey::new([1; 32]);
        sign(&pack, b"{}", &key).expect("signs");
        assert!(folder.path().join("user.json.mac").is_file());
        assert!(signed(&pack, b"{}", Some(&key)));
        assert!(!signed(&pack, b"{ }", Some(&key)));
        assert!(!signed(&pack, b"{}", None));
    }

    #[test]
    fn a_generated_key_survives_hex() {
        let key = PackKey::generate();
        assert_eq!(PackKey::from_hex(&key.hex()), Some(key));
    }
}
