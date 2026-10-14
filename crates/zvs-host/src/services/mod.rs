pub mod accounts;
pub mod crypto;
pub mod health_check;
pub mod probe_outcome;
pub mod provider_status;
pub mod providers;
pub mod secret_fields;
pub mod secret_policy;
pub mod secrets;
pub mod settings;
pub mod system;

pub use accounts::{AccountBrowser, AccountService, AccountServiceOptions, SharedAccountBrowser};
pub use crypto::{
    CryptoBackend, CryptoService, Plaintext, SharedCryptoBackend, UnavailableBackend,
};
pub use health_check::{HealthCheckOptions, HealthCheckService};
pub use providers::{
    DriverSource, ProbeInput, ProbeResult, ProviderDraftInput, ProviderService,
    ProviderServiceOptions,
};
pub use secrets::{SecretCreateInput, SecretService, SecretSummary, SecretUpdateInput};
pub use settings::{SettingDto, SettingService};
pub use system::SystemService;

use std::sync::Arc;

use crate::platform::theme::SharedThemePort;

pub struct Services {
    pub system: SystemService,
    pub accounts: AccountService,
    pub settings: SettingService,
    pub secrets: Arc<SecretService>,
    pub providers: ProviderService,
    pub health: HealthCheckService,
    pub theme: SharedThemePort,
}
