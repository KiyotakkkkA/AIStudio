use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zvs_contract::{AppError, AppErrorCode};

use crate::boot::StateRootLock;
use crate::data::schema::AccountRow;
use crate::drivers::ai::transport::SecretResolver;
use crate::platform::paths::SharedPaths;
use crate::services::crypto::{CryptoService, Plaintext};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialReference {
    pub id: uuid::Uuid,
    pub provider: String,
    pub account_id: Option<String>,
    pub display_name: Option<String>,
}

impl CredentialReference {
    pub(crate) fn openai(account: &AccountRow) -> Result<Self, AppError> {
        Ok(Self {
            id: account
                .token_secret_id
                .as_deref()
                .and_then(|id| uuid::Uuid::parse_str(id).ok())
                .ok_or_else(|| unavailable("Missing credential reference"))?,
            provider: zvs_contract::AccountFamily::Openai.as_str().to_owned(),
            account_id: Some(account.external_id.clone()),
            display_name: account.display_name.clone(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Credential {
    reference: CredentialReference,
    cipher: Vec<u8>,
}

pub struct SharedCredentials {
    root: PathBuf,
    crypto: Arc<CryptoService>,
}

impl SharedCredentials {
    pub fn new(paths: &SharedPaths, crypto: Arc<CryptoService>) -> Self {
        Self {
            root: paths.metadata.join("credentials"),
            crypto,
        }
    }

    pub fn put(&self, reference: CredentialReference, value: &Plaintext) -> Result<(), AppError> {
        let path = self.path(reference.id);
        let _lock = StateRootLock::blocking_exclusive(&path).map_err(unavailable)?;
        self.write(&path, reference, value)
    }

    pub(crate) fn adopt(
        &self,
        reference: CredentialReference,
        cipher: &[u8],
    ) -> Result<(), AppError> {
        let path = self.path(reference.id);
        let _lock = StateRootLock::blocking_exclusive(&path).map_err(unavailable)?;
        if let Some(existing) = read(&path)? {
            if existing.reference.id != reference.id
                || existing.reference.provider != reference.provider
                || existing.reference.account_id != reference.account_id
            {
                return Err(unavailable("Credential reference conflict"));
            }
            self.crypto.decrypt(&existing.cipher)?;
        } else {
            let expected = self.crypto.decrypt(cipher)?;
            let id = reference.id;
            self.write(&path, reference, &expected)?;
            if self.resolve(&id.to_string())? != expected {
                return Err(unavailable("Credential handoff verification failed"));
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<CredentialReference>, AppError> {
        let mut references = Vec::new();
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(references),
            Err(error) => return Err(unavailable(error)),
        };
        for entry in entries {
            let entry = entry.map_err(unavailable)?;
            if entry.path().extension().is_some_and(|ext| ext == "json")
                && let Some(credential) = read(&entry.path())?
            {
                references.push(credential.reference);
            }
        }
        references.sort_by_key(|reference| reference.id);
        Ok(references)
    }

    pub fn remove(&self, id: &str) -> Result<(), AppError> {
        let path = self.reference_path(id)?;
        let _lock = StateRootLock::blocking_exclusive(&path).map_err(unavailable)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(unavailable(error)),
        }
    }

    fn reference_path(&self, id: &str) -> Result<PathBuf, AppError> {
        uuid::Uuid::parse_str(id)
            .map(|id| self.path(id))
            .map_err(unavailable)
    }

    fn path(&self, id: uuid::Uuid) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    fn write(
        &self,
        path: &Path,
        reference: CredentialReference,
        value: &Plaintext,
    ) -> Result<(), AppError> {
        let cipher = self.crypto.encrypt(value)?;
        if self.crypto.decrypt(&cipher)? != *value {
            return Err(unavailable("Credential verification failed"));
        }
        let staged = tempfile::NamedTempFile::new_in(&self.root).map_err(unavailable)?;
        serde_json::to_writer(staged.as_file(), &Credential { reference, cipher })
            .map_err(unavailable)?;
        staged.as_file().sync_all().map_err(unavailable)?;
        staged.persist(path).map_err(unavailable)?;
        Ok(())
    }
}

impl SecretResolver for SharedCredentials {
    fn resolve(&self, id: &str) -> Result<Plaintext, AppError> {
        let credential =
            read(&self.reference_path(id)?)?.ok_or_else(|| super::secrets::not_found(id))?;
        self.crypto.decrypt(&credential.cipher)
    }

    fn replace(&self, id: &str, value: Plaintext) -> Result<(), AppError> {
        let path = self.reference_path(id)?;
        let credential = read(&path)?.ok_or_else(|| super::secrets::not_found(id))?;
        self.write(&path, credential.reference, &value)
    }

    fn refresh_lock_target(&self, id: &str) -> Result<Option<PathBuf>, AppError> {
        self.reference_path(id).map(Some)
    }
}

fn read(path: &Path) -> Result<Option<Credential>, AppError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(unavailable),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(unavailable(error)),
    }
}

fn unavailable(_error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::SecretDecryptFailed,
        "Не удалось открыть общее хранилище секретов",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeCrypto;

    #[test]
    fn separate_hosts_reuse_a_validated_credential_without_replaying_its_old_cipher() {
        let home = tempfile::tempdir().expect("temporary local data");
        let paths = SharedPaths::new(home.path().join("AI Studio"));
        let backend = FakeCrypto::shared();
        let crypto = Arc::new(CryptoService::new(backend.clone()));
        let desktop = SharedCredentials::new(&paths, Arc::clone(&crypto));
        let cli = SharedCredentials::new(&paths, Arc::clone(&crypto));
        let reference = CredentialReference {
            id: uuid::Uuid::new_v4(),
            provider: "openai".to_owned(),
            account_id: Some("account".to_owned()),
            display_name: None,
        };
        let id = reference.id.to_string();
        let original = Plaintext::new("old-access-and-refresh-tokens");
        let cipher = crypto.encrypt(&original).expect("seals");
        backend.fail_next();
        assert!(desktop.adopt(reference.clone(), &cipher).is_err());
        assert!(cli.list().expect("reads").is_empty());
        desktop.adopt(reference.clone(), &cipher).expect("adopts");
        assert_eq!(
            cli.list().expect("reads").as_slice(),
            std::slice::from_ref(&reference)
        );
        assert_eq!(cli.resolve(&id).expect("unseals"), original);
        let lock_target = cli
            .refresh_lock_target(&id)
            .expect("target")
            .expect("shared");
        let lock = StateRootLock::blocking_exclusive(&lock_target).expect("locks");
        assert!(
            matches!(StateRootLock::exclusive(&lock_target), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
        let rotated = Plaintext::new("new-access-and-refresh-tokens");
        cli.replace(&id, rotated.clone()).expect("replaces");
        drop(lock);
        desktop.adopt(reference, &cipher).expect("retries");
        assert_eq!(desktop.resolve(&id).expect("unseals"), rotated);
        desktop.remove(&id).expect("removes");
        assert!(cli.list().expect("reads").is_empty());
        assert!(cli.resolve(&id).is_err());
    }
}
