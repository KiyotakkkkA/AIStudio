use std::sync::Arc;

use serde_json::Value;

pub const THEME_SETTING_KEY: &str = "ui.theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSource {
    System,
    Light,
    Dark,
}

impl ThemeSource {
    pub fn parse(value: &Value) -> Option<Self> {
        match value.as_str()? {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

pub trait ThemePort: Send + Sync {
    fn apply(&self, source: ThemeSource);
}

pub type SharedThemePort = Arc<dyn ThemePort>;

pub fn apply_theme_source(value: Option<&Value>, port: &dyn ThemePort) {
    if let Some(source) = value.and_then(ThemeSource::parse) {
        port.apply(source);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testing::RecordingTheme;

    #[test]
    fn only_known_theme_modes_reach_the_platform_colour_scheme() {
        let theme = RecordingTheme::default();
        apply_theme_source(Some(&json!("dark")), &theme);
        apply_theme_source(Some(&json!("sepia")), &theme);
        apply_theme_source(None, &theme);
        apply_theme_source(Some(&json!(1)), &theme);
        apply_theme_source(Some(&json!("light")), &theme);
        apply_theme_source(Some(&json!("system")), &theme);
        assert_eq!(
            theme.applied(),
            [ThemeSource::Dark, ThemeSource::Light, ThemeSource::System]
        );
    }
}
