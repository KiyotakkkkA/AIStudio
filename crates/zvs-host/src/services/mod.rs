pub mod crypto;
pub mod secret_fields;
pub mod secret_policy;
pub mod secrets;
pub mod settings;
pub mod system;

pub use crypto::{
    CryptoBackend, CryptoService, Plaintext, SharedCryptoBackend, UnavailableBackend,
};
pub use secrets::{SecretCreateInput, SecretService, SecretSummary, SecretUpdateInput};
pub use settings::{SettingDto, SettingService};
pub use system::SystemService;

use crate::platform::theme::SharedThemePort;

pub struct Services {
    pub system: SystemService,
    pub settings: SettingService,
    pub secrets: SecretService,
    pub theme: SharedThemePort,
}
