use std::sync::Arc;

use serde_json::Value;

pub const THEME_SETTING_KEY: &str = "ui.theme";
pub const FONT_SIZE_SETTING_KEY: &str = "ui.font.size";
pub const DARK_PALETTE_SETTING_KEY: &str = "ui.palette.dark";
pub const LIGHT_PALETTE_SETTING_KEY: &str = "ui.palette.light";
pub const PALETTE_ID_MAX: usize = 64;
pub const BASE_FONT_SIZE: u64 = 13;
pub const FONT_SIZE_RANGE: std::ops::RangeInclusive<u64> = 12..=16;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Dark,
    Light,
}

impl PaletteMode {
    pub const ALL: [Self; 2] = [Self::Dark, Self::Light];

    pub const fn setting_key(self) -> &'static str {
        match self {
            Self::Dark => DARK_PALETTE_SETTING_KEY,
            Self::Light => LIGHT_PALETTE_SETTING_KEY,
        }
    }

    pub fn from_setting_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.setting_key() == key)
    }
}

pub fn palette_id(value: &Value) -> Option<&str> {
    value.as_str().filter(|id| {
        !id.is_empty()
            && id.len() <= PALETTE_ID_MAX
            && id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    })
}

pub trait ThemePort: Send + Sync {
    fn apply(&self, source: ThemeSource);

    fn zoom(&self, _factor: f64) {}

    fn palette(&self, _mode: PaletteMode, _id: &str) {}
}

pub fn interface_zoom(value: Option<&Value>) -> f64 {
    value
        .and_then(Value::as_u64)
        .filter(|size| FONT_SIZE_RANGE.contains(size))
        .map_or(1.0, |size| size as f64 / BASE_FONT_SIZE as f64)
}

pub fn apply_interface_zoom(value: Option<&Value>, port: &dyn ThemePort) {
    port.zoom(interface_zoom(value));
}

pub type SharedThemePort = Arc<dyn ThemePort>;

pub fn apply_palette(key: &str, value: Option<&Value>, port: &dyn ThemePort) {
    if let (Some(mode), Some(id)) = (
        PaletteMode::from_setting_key(key),
        value.and_then(palette_id),
    ) {
        port.palette(mode, id);
    }
}

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

    #[test]
    fn only_well_formed_palette_ids_under_palette_keys_reach_the_platform() {
        let theme = RecordingTheme::default();
        apply_palette("ui.palette.dark", Some(&json!("basic-grey")), &theme);
        apply_palette("ui.palette.light", Some(&json!("basic-white")), &theme);
        apply_palette("ui.palette.sepia", Some(&json!("nord")), &theme);
        apply_palette("ui.palette.dark", Some(&json!("Nord")), &theme);
        apply_palette("ui.palette.dark", Some(&json!("")), &theme);
        apply_palette("ui.palette.dark", Some(&json!("x".repeat(65))), &theme);
        apply_palette("ui.palette.dark", Some(&json!(3)), &theme);
        apply_palette("ui.palette.light", None, &theme);
        assert_eq!(
            theme.palettes(),
            [
                (PaletteMode::Dark, "basic-grey".to_owned()),
                (PaletteMode::Light, "basic-white".to_owned()),
            ]
        );
    }

    #[test]
    fn the_font_size_becomes_a_webview_zoom_relative_to_the_base_size() {
        assert!((interface_zoom(Some(&json!(13))) - 1.0).abs() < f64::EPSILON);
        assert!((interface_zoom(Some(&json!(16))) - 16.0 / 13.0).abs() < f64::EPSILON);
        for value in [json!(40), json!("15"), json!(12.5), Value::Null] {
            assert!((interface_zoom(Some(&value)) - 1.0).abs() < f64::EPSILON);
        }
        assert!((interface_zoom(None) - 1.0).abs() < f64::EPSILON);
    }
}
