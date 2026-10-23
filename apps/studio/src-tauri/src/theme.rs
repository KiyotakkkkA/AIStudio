use tauri::{AppHandle, Manager, Runtime, Theme};
use zvs_host::platform::theme::{ThemePort, ThemeSource};

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

impl<R: Runtime> ThemePort for TauriTheme<R> {
    fn apply(&self, source: ThemeSource) {
        self.0.set_theme(tauri_theme(source));
    }

    fn zoom(&self, factor: f64) {
        if let Some(studio) = self.0.get_webview(crate::window::STUDIO) {
            let _ = studio.set_zoom(factor);
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
}
