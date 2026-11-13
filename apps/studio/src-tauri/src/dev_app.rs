use std::time::Duration;

use tauri::{AppHandle, Runtime};
use zvs_host::boot::{BootError, BootProgress};

use crate::window::BootReporter;

pub const DEV_APP_VAR: &str = "ZVS_DEV_APP";
const BOOT_PREVIEW_TOTAL: u64 = 400;
const BOOT_PREVIEW_STEPS: [&str; 3] = ["Перенос настроек", "Перенос диалогов", "Перенос хранилищ"];
const BOOT_PREVIEW_TICK: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevApp {
    Studio,
    Boot,
    Browser,
    Computer,
    Recovery,
}

impl DevApp {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "studio" => Some(Self::Studio),
            "boot" => Some(Self::Boot),
            "browser" => Some(Self::Browser),
            "computer" => Some(Self::Computer),
            "recovery" => Some(Self::Recovery),
            _ => None,
        }
    }

    pub fn current() -> Self {
        if !tauri::is_dev() {
            return Self::Studio;
        }
        std::env::var(DEV_APP_VAR)
            .ok()
            .and_then(|value| Self::parse(&value))
            .unwrap_or(Self::Studio)
    }
}

pub fn recovery_preview() -> BootError {
    BootError::Newer {
        schema: "chat".to_owned(),
        found: 9,
        supported: 7,
    }
}

pub fn boot_preview_frames() -> impl Iterator<Item = BootProgress> {
    let steps = BOOT_PREVIEW_STEPS.len();
    BOOT_PREVIEW_STEPS
        .into_iter()
        .enumerate()
        .flat_map(move |(index, label)| {
            (0..=BOOT_PREVIEW_TOTAL)
                .step_by(8)
                .map(move |done| BootProgress {
                    step: index + 1,
                    steps,
                    label,
                    done,
                    total: BOOT_PREVIEW_TOTAL,
                })
        })
}

pub fn play_boot_preview<R: Runtime>(app: AppHandle<R>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("boot-preview".to_owned())
        .spawn(move || {
            let reporter = BootReporter::new(app);
            loop {
                for progress in boot_preview_frames() {
                    reporter.report(progress);
                    std::thread::sleep(BOOT_PREVIEW_TICK);
                }
            }
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_name_parses() {
        assert_eq!(DevApp::parse("studio"), Some(DevApp::Studio));
        assert_eq!(DevApp::parse(""), Some(DevApp::Studio));
        assert_eq!(DevApp::parse("Boot"), Some(DevApp::Boot));
        assert_eq!(DevApp::parse(" browser "), Some(DevApp::Browser));
        assert_eq!(DevApp::parse("computer"), Some(DevApp::Computer));
        assert_eq!(DevApp::parse("recovery"), Some(DevApp::Recovery));
        assert_eq!(DevApp::parse("panel"), None);
    }

    #[test]
    fn the_boot_preview_runs_every_step_to_completion() {
        let frames: Vec<BootProgress> = boot_preview_frames().collect();
        for step in 1..=BOOT_PREVIEW_STEPS.len() {
            let last = frames.iter().rfind(|frame| frame.step == step).unwrap();
            assert_eq!(last.done, last.total);
            assert_eq!(last.steps, BOOT_PREVIEW_STEPS.len());
        }
    }
}
