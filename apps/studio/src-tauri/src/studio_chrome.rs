use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime, Theme};
use zvs_host::platform::theme::{PaletteMode, ThemeSource};

use crate::window::STUDIO;

type FrameCatalog = BTreeMap<String, BTreeMap<String, FrameColors>>;

#[derive(Deserialize)]
struct FrameColors {
    frame: String,
    ink: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CaptionColors {
    pub background: [u8; 3],
    pub foreground: [u8; 3],
}

struct Appearance {
    source: ThemeSource,
    dark: String,
    light: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            source: ThemeSource::System,
            dark: "zvs-dark".into(),
            light: "zvs-light".into(),
        }
    }
}

impl Appearance {
    fn colors(&self, system: Theme) -> CaptionColors {
        let theme = crate::theme::tauri_theme(self.source).unwrap_or(system);
        let (mode, selected, fallback) = match theme {
            Theme::Dark => ("dark", &self.dark, "zvs-dark"),
            _ => ("light", &self.light, "zvs-light"),
        };
        let catalog = &catalog()[mode];
        let colors = catalog.get(selected).unwrap_or(&catalog[fallback]);
        CaptionColors {
            background: rgb(&colors.frame),
            foreground: rgb(&colors.ink),
        }
    }
}

fn catalog() -> &'static FrameCatalog {
    static COLORS: OnceLock<FrameCatalog> = OnceLock::new();
    COLORS.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/renderer/app/frameColors.json"))
            .expect("the bundled frame palette catalog is valid")
    })
}

fn rgb(hex: &str) -> [u8; 3] {
    let value = u32::from_str_radix(&hex[1..], 16).expect("a bundled RGB color");
    [(value >> 16) as u8, (value >> 8) as u8, value as u8]
}

#[derive(Default)]
struct StudioChrome(Mutex<Appearance>);

pub fn install<R: Runtime>(app: &AppHandle<R>) {
    app.manage(StudioChrome::default());
    refresh(app);
}

pub fn set_theme<R: Runtime>(app: &AppHandle<R>, source: ThemeSource) {
    if let Some(state) = app.try_state::<StudioChrome>() {
        state
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .source = source;
        refresh(app);
    }
}

pub fn set_palette<R: Runtime>(app: &AppHandle<R>, mode: PaletteMode, id: &str) {
    if let Some(state) = app.try_state::<StudioChrome>() {
        let mut appearance = state.0.lock().unwrap_or_else(PoisonError::into_inner);
        match mode {
            PaletteMode::Dark => appearance.dark = id.into(),
            PaletteMode::Light => appearance.light = id.into(),
        }
        drop(appearance);
        refresh(app);
    }
}

pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(STUDIO) else {
        return;
    };
    let app = app.clone();
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Some(state) = app.try_state::<StudioChrome>() else {
            return;
        };
        let colors = state
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .colors(target.theme().unwrap_or(Theme::Light));
        let [r, g, b] = colors.background;
        let _ = target.set_background_color(Some(tauri::window::Color(r, g, b, 255)));
        #[cfg(windows)]
        if let Ok(hwnd) = target.hwnd() {
            crate::platform::windows::chrome::hide_caption_icon(hwnd.0 as isize);
            crate::platform::windows::chrome::set_colors(hwnd.0 as isize, &colors);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caption_follows_system_and_explicit_theme_choices() {
        let mut appearance = Appearance::default();
        assert_eq!(appearance.colors(Theme::Dark).background, [15, 13, 11]);
        assert_eq!(appearance.colors(Theme::Light).background, [236, 230, 221]);
        appearance.source = ThemeSource::Dark;
        assert_eq!(appearance.colors(Theme::Light).background, [15, 13, 11]);
        appearance.source = ThemeSource::Light;
        assert_eq!(appearance.colors(Theme::Dark).background, [236, 230, 221]);
    }

    #[test]
    fn selected_palettes_supply_both_the_frame_and_legible_caption_text() {
        let appearance = Appearance {
            dark: "github-dark".into(),
            light: "basic-white".into(),
            ..Appearance::default()
        };
        assert_eq!(
            appearance.colors(Theme::Dark),
            CaptionColors {
                background: [1, 4, 9],
                foreground: [240, 246, 252],
            }
        );
        assert_eq!(
            appearance.colors(Theme::Light),
            CaptionColors {
                background: [242, 242, 242],
                foreground: [10, 10, 10],
            }
        );
    }

    #[test]
    fn unknown_palette_ids_fall_back_to_the_matching_default() {
        let appearance = Appearance {
            dark: "unknown".into(),
            light: "unknown".into(),
            ..Appearance::default()
        };
        for theme in [Theme::Light, Theme::Dark] {
            assert_eq!(
                appearance.colors(theme),
                Appearance::default().colors(theme)
            );
        }
    }

    #[test]
    fn a_palette_from_the_other_mode_falls_back_like_the_renderer() {
        let appearance = Appearance {
            dark: "basic-white".into(),
            light: "github-dark".into(),
            ..Appearance::default()
        };
        for theme in [Theme::Light, Theme::Dark] {
            assert_eq!(
                appearance.colors(theme),
                Appearance::default().colors(theme)
            );
        }
    }

    #[test]
    fn every_shared_color_is_a_six_digit_rgb_value() {
        for colors in catalog().values().flat_map(BTreeMap::values) {
            for hex in [&colors.frame, &colors.ink] {
                assert_eq!(hex.len(), 7);
                assert!(hex.starts_with('#'));
                assert!(hex[1..].bytes().all(|byte| byte.is_ascii_hexdigit()));
                let _ = rgb(hex);
            }
        }
    }
}
