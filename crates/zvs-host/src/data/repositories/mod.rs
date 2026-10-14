pub mod model;
pub mod provider;
pub mod secret;
pub mod setting;

use rusqlite::Connection;

pub use model::{ModelDraft, ModelRepository};
pub use provider::{ProviderDraft, ProviderFilter, ProviderPatch, ProviderRepository};
pub use secret::{
    SecretDraft, SecretFilter, SecretPatch, SecretRepository, SecretUsageDraft, secret_in_use,
};
pub use setting::SettingRepository;

pub struct Repositories<'c> {
    pub settings: SettingRepository<'c>,
    pub secrets: SecretRepository<'c>,
    pub providers: ProviderRepository<'c>,
    pub models: ModelRepository<'c>,
}

impl<'c> Repositories<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self {
            settings: SettingRepository::new(connection),
            secrets: SecretRepository::new(connection),
            providers: ProviderRepository::new(connection),
            models: ModelRepository::new(connection),
        }
    }
}
