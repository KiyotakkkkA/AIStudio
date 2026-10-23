use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;
use zvs_contract::NotificationTargetDto;
use zvs_host::platform::shell::{NotificationDraft, ShellPort};

use crate::toast::Toaster;
use crate::window;

pub const AUTOSTART_ARG: &str = "--autostart";
pub const TRAY_ID: &str = "studio-tray";
const TRAY_TOOLTIP: &str = "ZVS AI Studio";
const TRAY_OPEN: &str = "Открыть";
const TRAY_QUIT: &str = "Выход";
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";

pub fn launched_by_autostart() -> bool {
    std::env::args().skip(1).any(|arg| arg == AUTOSTART_ARG)
}

#[derive(Default)]
pub struct ShellState {
    tray: AtomicBool,
    quitting: AtomicBool,
}

impl ShellState {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn keeps_running(&self) -> bool {
        self.tray.load(Ordering::SeqCst) && !self.quitting.load(Ordering::SeqCst)
    }

    pub fn tray(&self) -> bool {
        self.tray.load(Ordering::SeqCst)
    }

    fn set_tray(&self, enabled: bool) {
        self.tray.store(enabled, Ordering::SeqCst);
    }

    pub fn quit(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }
}

pub struct TauriShell<R: Runtime> {
    app: AppHandle<R>,
    state: Arc<ShellState>,
    toaster: Arc<dyn Toaster>,
}

impl<R: Runtime> TauriShell<R> {
    pub fn new(app: AppHandle<R>, state: Arc<ShellState>, toaster: Arc<dyn Toaster>) -> Self {
        Self {
            app,
            state,
            toaster,
        }
    }
}

fn quit<R: Runtime>(app: &AppHandle<R>, state: &ShellState) {
    state.quit();
    if let Some(studio) = app.get_window(window::STUDIO) {
        let _ = studio.close();
    }
    app.exit(0);
}

fn show_tray<R: Runtime>(app: &AppHandle<R>, state: Arc<ShellState>) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let open = MenuItem::with_id(app, MENU_OPEN, TRAY_OPEN, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, MENU_QUIT, TRAY_QUIT, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &separator, &exit])?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TRAY_TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            MENU_OPEN => window::focus_studio(app),
            MENU_QUIT => quit(app, &state),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::focus_studio(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

impl<R: Runtime> ShellPort for TauriShell<R> {
    fn set_autostart(&self, enabled: bool) -> Result<(), String> {
        let manager = self.app.autolaunch();
        let changed = if enabled {
            manager.enable()
        } else {
            manager.disable()
        };
        changed.map_err(|error| error.to_string())
    }

    fn autostart_enabled(&self) -> Option<bool> {
        self.app.autolaunch().is_enabled().ok()
    }

    fn set_tray(&self, enabled: bool) {
        self.state.set_tray(enabled);
        let app = self.app.clone();
        let state = Arc::clone(&self.state);
        let _ = self.app.run_on_main_thread(move || {
            if enabled {
                let _ = show_tray(&app, state);
            } else {
                let _ = app.remove_tray_by_id(TRAY_ID);
            }
        });
    }

    fn notify(&self, draft: NotificationDraft) {
        self.toaster.show(draft);
    }

    fn clear(&self, target: NotificationTargetDto) {
        self.toaster.clear(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_keeps_running_only_with_the_tray_and_until_quit() {
        let state = ShellState::default();
        assert!(!state.keeps_running());
        state.set_tray(true);
        assert!(state.keeps_running());
        state.quit();
        assert!(!state.keeps_running());
    }
}
