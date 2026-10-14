pub mod secret;
pub mod setting;

use rusqlite::Connection;

pub use secret::{
    SecretDraft, SecretFilter, SecretPatch, SecretRepository, SecretUsageDraft, secret_in_use,
};
pub use setting::SettingRepository;

pub struct Repositories<'c> {
    pub settings: SettingRepository<'c>,
    pub secrets: SecretRepository<'c>,
}

impl<'c> Repositories<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self {
            settings: SettingRepository::new(connection),
            secrets: SecretRepository::new(connection),
        }
    }
}
