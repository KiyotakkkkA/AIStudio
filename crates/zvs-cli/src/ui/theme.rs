use std::cell::Cell;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use zvs_contract::CliTheme;

pub struct Colors {
    pub gradient: [(u8, u8, u8); 6],
    pub accent: Color,
    pub accent_light: Color,
    pub ink: Color,
    pub muted: Color,
    pub faint: Color,
    pub line: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub selection_bg: Color,
    pub code_bg: Color,
}

const fn rgb(value: u32) -> Color {
    Color::Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

const fn stop(value: u32) -> (u8, u8, u8) {
    ((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

const EMBER: Colors = Colors {
    gradient: [
        stop(0xffd08a),
        stop(0xffb27a),
        stop(0xf2914d),
        stop(0xe57532),
        stop(0xc2571c),
        stop(0x9a4214),
    ],
    accent: rgb(0xf2914d),
    accent_light: rgb(0xf0c9ad),
    ink: rgb(0xece3d9),
    muted: rgb(0x9c8e80),
    faint: rgb(0x5e5349),
    line: rgb(0x4a3f36),
    ok: rgb(0x9cc06b),
    warn: rgb(0xe8b84a),
    err: rgb(0xe86a58),
    selection_bg: rgb(0x5a3a22),
    code_bg: rgb(0x1c1814),
};

const GRAPHITE: Colors = Colors {
    gradient: [
        stop(0xc9dcff),
        stop(0xa6c4ff),
        stop(0x7fa8f5),
        stop(0x5e8ce8),
        stop(0x4570cc),
        stop(0x3457a3),
    ],
    accent: rgb(0x7fa8f5),
    accent_light: rgb(0xc3d5f5),
    ink: rgb(0xe2e5ea),
    muted: rgb(0x8e95a0),
    faint: rgb(0x555b64),
    line: rgb(0x3a3f47),
    ok: rgb(0x8fc47a),
    warn: rgb(0xe5c06a),
    err: rgb(0xe86f6f),
    selection_bg: rgb(0x2f456b),
    code_bg: rgb(0x181a1e),
};

const MOSS: Colors = Colors {
    gradient: [
        stop(0xd8eca8),
        stop(0xbfdd8a),
        stop(0x9cc86a),
        stop(0x7db052),
        stop(0x5f923e),
        stop(0x47722f),
    ],
    accent: rgb(0x9cc86a),
    accent_light: rgb(0xcfe3b3),
    ink: rgb(0xe3e8da),
    muted: rgb(0x939c86),
    faint: rgb(0x58604e),
    line: rgb(0x3b4233),
    ok: rgb(0x7cc9a5),
    warn: rgb(0xe6bd58),
    err: rgb(0xe56f5c),
    selection_bg: rgb(0x3a4c26),
    code_bg: rgb(0x181b15),
};

const DUSK: Colors = Colors {
    gradient: [
        stop(0xe6d2ff),
        stop(0xd2b4ff),
        stop(0xb892f5),
        stop(0x9e74e6),
        stop(0x8258cc),
        stop(0x6642a6),
    ],
    accent: rgb(0xb892f5),
    accent_light: rgb(0xdcc8f8),
    ink: rgb(0xe8e2f0),
    muted: rgb(0x9a90a8),
    faint: rgb(0x5d556a),
    line: rgb(0x423a4f),
    ok: rgb(0x8fc88a),
    warn: rgb(0xe8be62),
    err: rgb(0xec7286),
    selection_bg: rgb(0x46346a),
    code_bg: rgb(0x1b1721),
};

const FROST: Colors = Colors {
    gradient: [
        stop(0xc8f2f6),
        stop(0xa0e4ee),
        stop(0x6ecfde),
        stop(0x4cb4c8),
        stop(0x3696ab),
        stop(0x287688),
    ],
    accent: rgb(0x6ecfde),
    accent_light: rgb(0xb9e6ed),
    ink: rgb(0xdeeaec),
    muted: rgb(0x889a9e),
    faint: rgb(0x505f63),
    line: rgb(0x334246),
    ok: rgb(0x95cc7c),
    warn: rgb(0xe8c068),
    err: rgb(0xec746c),
    selection_bg: rgb(0x224a54),
    code_bg: rgb(0x141a1c),
};

thread_local! {
    static ACTIVE: Cell<CliTheme> = const { Cell::new(CliTheme::Ember) };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorDepth {
    None,
    Indexed,
    Rgb,
}

struct Appearance {
    light: bool,
    depth: ColorDepth,
}

fn appearance() -> &'static Appearance {
    static DETECTED: OnceLock<Appearance> = OnceLock::new();
    DETECTED.get_or_init(|| {
        let terminal = std::env::var("TERM").unwrap_or_default();
        let true_color = std::env::var("COLORTERM")
            .is_ok_and(|value| matches!(value.as_str(), "truecolor" | "24bit"))
            || std::env::var_os("WT_SESSION").is_some();
        let depth = if std::env::var_os("NO_COLOR").is_some() || terminal == "dumb" {
            ColorDepth::None
        } else if !true_color && terminal.contains("256color") {
            ColorDepth::Indexed
        } else {
            ColorDepth::Rgb
        };
        let light = match std::env::var("ZVS_CLI_BACKGROUND").as_deref() {
            Ok("light") => true,
            Ok("dark") => false,
            _ => std::env::var("COLORFGBG")
                .ok()
                .and_then(|value| value.rsplit(';').next()?.parse::<u8>().ok())
                .is_some_and(|background| background == 7 || background == 15),
        };
        Appearance { light, depth }
    })
}

fn adapt(color: Color, depth: ColorDepth) -> Color {
    match (depth, color) {
        (ColorDepth::None, _) => Color::Reset,
        (ColorDepth::Indexed, Color::Rgb(r, g, b)) => {
            let cube = |value: u8| u8::try_from((u16::from(value) * 5 + 127) / 255).unwrap_or(5);
            Color::Indexed(16 + 36 * cube(r) + 6 * cube(g) + cube(b))
        }
        _ => color,
    }
}

fn terminal_palette(mut colors: Colors, appearance: &Appearance) -> Colors {
    let foreground = |color| {
        let color = if appearance.light {
            dim(color, 0.53)
        } else {
            color
        };
        adapt(color, appearance.depth)
    };
    colors.accent = foreground(colors.accent);
    colors.accent_light = foreground(colors.accent_light);
    colors.ink = Color::Reset;
    colors.muted = foreground(if appearance.light {
        rgb(0xbbbbbb)
    } else {
        colors.muted
    });
    colors.faint = foreground(if appearance.light {
        rgb(0xd1d1d1)
    } else {
        colors.faint
    });
    colors.line = foreground(if appearance.light {
        rgb(0xcccccc)
    } else {
        colors.line
    });
    colors.ok = foreground(colors.ok);
    colors.warn = foreground(colors.warn);
    colors.err = foreground(colors.err);
    colors.selection_bg = adapt(
        if appearance.light {
            rgb(0xd8e2f0)
        } else {
            colors.selection_bg
        },
        appearance.depth,
    );
    colors.code_bg = adapt(
        if appearance.light {
            rgb(0xf0f0f0)
        } else {
            colors.code_bg
        },
        appearance.depth,
    );
    colors
}

pub fn colors_of(theme: CliTheme) -> &'static Colors {
    static PALETTES: OnceLock<[Colors; 5]> = OnceLock::new();
    let palettes = PALETTES.get_or_init(|| {
        [EMBER, GRAPHITE, MOSS, DUSK, FROST].map(|colors| terminal_palette(colors, appearance()))
    });
    &palettes[match theme {
        CliTheme::Ember => 0,
        CliTheme::Graphite => 1,
        CliTheme::Moss => 2,
        CliTheme::Dusk => 3,
        CliTheme::Frost => 4,
    }]
}

pub fn activate(theme: CliTheme) {
    ACTIVE.with(|active| active.set(theme));
}

pub fn active() -> CliTheme {
    ACTIVE.with(Cell::get)
}

pub fn colors() -> &'static Colors {
    colors_of(active())
}

pub fn ember(position: f64) -> Color {
    let stops = &colors().gradient;
    let position = position.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
    let index = (position.floor() as usize).min(stops.len() - 2);
    let fraction = position - index as f64;
    let (from, to) = (stops[index], stops[index + 1]);
    let mix =
        |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * fraction).round() as u8;
    let color = Color::Rgb(mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2));
    adapt(
        if appearance().light {
            dim(color, 0.53)
        } else {
            color
        },
        appearance().depth,
    )
}

pub fn dim(color: Color, factor: f64) -> Color {
    match color {
        Color::Rgb(red, green, blue) => {
            let scale = |channel: u8| (f64::from(channel) * factor).round() as u8;
            Color::Rgb(scale(red), scale(green), scale(blue))
        }
        other => other,
    }
}

pub fn text() -> Style {
    Style::new().fg(colors().ink)
}

pub fn muted() -> Style {
    Style::new().fg(colors().muted)
}

pub fn faint() -> Style {
    Style::new().fg(colors().faint)
}

pub fn accent() -> Style {
    Style::new().fg(colors().accent)
}

pub fn strong() -> Style {
    Style::new().fg(colors().ink).add_modifier(Modifier::BOLD)
}

pub fn eyebrow() -> Style {
    Style::new().fg(colors().muted).add_modifier(Modifier::BOLD)
}

pub fn selected() -> Style {
    accent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_its_own_accent_and_the_active_one_drives_the_styles() {
        let accents: Vec<Color> = [EMBER, GRAPHITE, MOSS, DUSK, FROST]
            .iter()
            .map(|colors| colors.accent)
            .collect();
        for (index, accent) in accents.iter().enumerate() {
            assert!(!accents[index + 1..].contains(accent));
        }
        activate(CliTheme::Frost);
        assert_eq!(accent().fg, Some(colors_of(CliTheme::Frost).accent));
        activate(CliTheme::Ember);
        assert_eq!(accent().fg, Some(colors_of(CliTheme::Ember).accent));
    }

    #[test]
    fn terminal_appearance_preserves_the_background_and_limits_colors() {
        let light = terminal_palette(
            EMBER,
            &Appearance {
                light: true,
                depth: ColorDepth::Rgb,
            },
        );
        assert_eq!(light.ink, Color::Reset);
        assert_ne!(light.accent, EMBER.accent);
        assert_eq!(light.code_bg, rgb(0xf0f0f0));
        let indexed = terminal_palette(
            FROST,
            &Appearance {
                light: false,
                depth: ColorDepth::Indexed,
            },
        );
        assert!(matches!(indexed.accent, Color::Indexed(_)));
        let plain = terminal_palette(
            MOSS,
            &Appearance {
                light: false,
                depth: ColorDepth::None,
            },
        );
        assert_eq!(plain.accent, Color::Reset);
        assert_eq!(plain.code_bg, Color::Reset);
    }
}
