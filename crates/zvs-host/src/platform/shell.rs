use std::sync::Arc;

use serde_json::Value;
use zvs_contract::NotificationTargetDto;

pub const AUTOSTART_SETTING_KEY: &str = "app.autostart";
pub const TRAY_SETTING_KEY: &str = "app.tray";
pub const NOTIFICATIONS_SETTING_KEY: &str = "notifications.enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationKind {
    Approval,
    Turn,
    Download,
}

impl NotificationKind {
    pub const ALL: [Self; 3] = [Self::Approval, Self::Turn, Self::Download];

    pub const fn group(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Turn => "turn",
            Self::Download => "download",
        }
    }

    pub const fn setting_key(self) -> &'static str {
        match self {
            Self::Approval => "notifications.approval",
            Self::Turn => "notifications.turn",
            Self::Download => "notifications.download",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDraft {
    pub kind: NotificationKind,
    pub tag: String,
    pub title: String,
    pub body: String,
    pub target: NotificationTargetDto,
    pub silent: bool,
}

pub trait ShellPort: Send + Sync {
    fn set_autostart(&self, enabled: bool) -> Result<(), String>;

    fn autostart_enabled(&self) -> Option<bool>;

    fn set_tray(&self, enabled: bool);

    fn notify(&self, draft: NotificationDraft);

    fn clear(&self, target: NotificationTargetDto);
}

pub type SharedShellPort = Arc<dyn ShellPort>;

pub struct NoShell;

impl ShellPort for NoShell {
    fn set_autostart(&self, _enabled: bool) -> Result<(), String> {
        Ok(())
    }

    fn autostart_enabled(&self) -> Option<bool> {
        None
    }

    fn set_tray(&self, _enabled: bool) {}

    fn notify(&self, _draft: NotificationDraft) {}

    fn clear(&self, _target: NotificationTargetDto) {}
}

pub fn flag(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}
