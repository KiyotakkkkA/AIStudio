use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CliInstallState {
    pub available: bool,
    pub installed: bool,
}

pub trait CliInstaller: Send + Sync {
    fn state(&self) -> CliInstallState;

    fn install(&self) -> Result<(), String>;

    fn uninstall(&self) -> Result<(), String>;
}

pub type SharedCliInstaller = Arc<dyn CliInstaller>;

pub struct NoCliInstaller;

impl CliInstaller for NoCliInstaller {
    fn state(&self) -> CliInstallState {
        CliInstallState::default()
    }

    fn install(&self) -> Result<(), String> {
        Err("the CLI is not bundled".to_owned())
    }

    fn uninstall(&self) -> Result<(), String> {
        Ok(())
    }
}
