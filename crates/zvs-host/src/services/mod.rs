pub mod settings;
pub mod system;

pub use settings::{SettingDto, SettingService};
pub use system::SystemService;

use crate::platform::theme::SharedThemePort;

pub struct Services {
    pub system: SystemService,
    pub settings: SettingService,
    pub theme: SharedThemePort,
}
