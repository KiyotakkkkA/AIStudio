use tauri::{AppHandle, Manager, Runtime, Theme};
use zvs_contract::BrowserTheme;
use zvs_host::platform::theme::{PaletteMode, ThemePort, ThemeSource};

pub struct TauriTheme<R: Runtime>(AppHandle<R>);

impl<R: Runtime> TauriTheme<R> {
    pub const fn new(app: AppHandle<R>) -> Self {
        Self(app)
    }
}

pub const fn tauri_theme(source: ThemeSource) -> Option<Theme> {
    match source {
        ThemeSource::System => None,
        ThemeSource::Light => Some(Theme::Light),
        ThemeSource::Dark => Some(Theme::Dark),
    }
}

pub const fn browser_theme(source: ThemeSource) -> BrowserTheme {
    match source {
        ThemeSource::System => BrowserTheme::System,
        ThemeSource::Light => BrowserTheme::Light,
        ThemeSource::Dark => BrowserTheme::Dark,
    }
}

impl<R: Runtime> TauriTheme<R> {
    fn workspace(&self) -> Option<crate::browser::BrowserWorkspace> {
        self.0
            .try_state::<crate::browser::BrowserWorkspace>()
            .map(|state| state.inner().clone())
    }
}

impl<R: Runtime> ThemePort for TauriTheme<R> {
    fn apply(&self, source: ThemeSource) {
        self.0.set_theme(tauri_theme(source));
        if let Some(workspace) = self.workspace() {
            workspace.set_theme(browser_theme(source));
        }
    }

    fn palette(&self, mode: PaletteMode, id: &str) {
        if let Some(workspace) = self.workspace() {
            workspace.set_palette(mode, id);
        }
    }

    fn zoom(&self, factor: f64) {
        if let Some(studio) = self.0.get_webview(crate::window::STUDIO) {
            let _ = studio.set_zoom(factor);
        }
        if let Some(workspace) = self.0.try_state::<crate::browser::BrowserWorkspace>() {
            workspace.zoom(factor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_hands_the_choice_back_to_the_os() {
        assert_eq!(tauri_theme(ThemeSource::System), None);
        assert_eq!(tauri_theme(ThemeSource::Light), Some(Theme::Light));
        assert_eq!(tauri_theme(ThemeSource::Dark), Some(Theme::Dark));
    }

    #[test]
    fn the_browser_chrome_receives_the_same_theme_mode() {
        assert_eq!(browser_theme(ThemeSource::System), BrowserTheme::System);
        assert_eq!(browser_theme(ThemeSource::Light), BrowserTheme::Light);
        assert_eq!(browser_theme(ThemeSource::Dark), BrowserTheme::Dark);
    }
}
