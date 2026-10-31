use std::cell::Cell;

use ratatui::style::{Color, Modifier, Style};
use zvs_contract::CliTheme;

pub struct Colors {
    pub gradient: [(u8, u8, u8); 6],
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_light: Color,
    pub ink: Color,
    pub muted: Color,
    pub faint: Color,
    pub line: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub on_warn: Color,
    pub user_bg: Color,
    pub tool_bg: Color,
    pub panel_bg: Color,
    pub prompt_bg: Color,
    pub shell_bg: Color,
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
    accent_soft: rgb(0x3a2416),
    accent_light: rgb(0xf0c9ad),
    ink: rgb(0xece3d9),
    muted: rgb(0x9c8e80),
    faint: rgb(0x5e5349),
    line: rgb(0x4a3f36),
    ok: rgb(0x9cc06b),
    warn: rgb(0xe8b84a),
    err: rgb(0xe86a58),
    on_warn: rgb(0x1a1408),
    user_bg: rgb(0x2b221c),
    tool_bg: rgb(0x302720),
    panel_bg: rgb(0x221c17),
    prompt_bg: rgb(0x2e2414),
    shell_bg: rgb(0x2a2412),
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
    accent_soft: rgb(0x1e2a3e),
    accent_light: rgb(0xc3d5f5),
    ink: rgb(0xe2e5ea),
    muted: rgb(0x8e95a0),
    faint: rgb(0x555b64),
    line: rgb(0x3a3f47),
    ok: rgb(0x8fc47a),
    warn: rgb(0xe5c06a),
    err: rgb(0xe86f6f),
    on_warn: rgb(0x17140a),
    user_bg: rgb(0x22252b),
    tool_bg: rgb(0x272a31),
    panel_bg: rgb(0x1c1f24),
    prompt_bg: rgb(0x292618),
    shell_bg: rgb(0x282514),
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
    accent_soft: rgb(0x222e18),
    accent_light: rgb(0xcfe3b3),
    ink: rgb(0xe3e8da),
    muted: rgb(0x939c86),
    faint: rgb(0x58604e),
    line: rgb(0x3b4233),
    ok: rgb(0x7cc9a5),
    warn: rgb(0xe6bd58),
    err: rgb(0xe56f5c),
    on_warn: rgb(0x181408),
    user_bg: rgb(0x22271d),
    tool_bg: rgb(0x282d22),
    panel_bg: rgb(0x1c2018),
    prompt_bg: rgb(0x2b2814),
    shell_bg: rgb(0x2a2612),
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
    accent_soft: rgb(0x2c223e),
    accent_light: rgb(0xdcc8f8),
    ink: rgb(0xe8e2f0),
    muted: rgb(0x9a90a8),
    faint: rgb(0x5d556a),
    line: rgb(0x423a4f),
    ok: rgb(0x8fc88a),
    warn: rgb(0xe8be62),
    err: rgb(0xec7286),
    on_warn: rgb(0x1a140a),
    user_bg: rgb(0x272130),
    tool_bg: rgb(0x2c2636),
    panel_bg: rgb(0x201b28),
    prompt_bg: rgb(0x2d261a),
    shell_bg: rgb(0x2c2515),
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
    accent_soft: rgb(0x172e34),
    accent_light: rgb(0xb9e6ed),
    ink: rgb(0xdeeaec),
    muted: rgb(0x889a9e),
    faint: rgb(0x505f63),
    line: rgb(0x334246),
    ok: rgb(0x95cc7c),
    warn: rgb(0xe8c068),
    err: rgb(0xec746c),
    on_warn: rgb(0x161309),
    user_bg: rgb(0x1b2629),
    tool_bg: rgb(0x202b2e),
    panel_bg: rgb(0x161f22),
    prompt_bg: rgb(0x292616),
    shell_bg: rgb(0x282513),
    selection_bg: rgb(0x224a54),
    code_bg: rgb(0x141a1c),
};

thread_local! {
    static ACTIVE: Cell<CliTheme> = const { Cell::new(CliTheme::Ember) };
}

pub const fn colors_of(theme: CliTheme) -> &'static Colors {
    match theme {
        CliTheme::Ember => &EMBER,
        CliTheme::Graphite => &GRAPHITE,
        CliTheme::Moss => &MOSS,
        CliTheme::Dusk => &DUSK,
        CliTheme::Frost => &FROST,
    }
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
    Color::Rgb(mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
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
    let colors = colors();
    Style::new()
        .fg(colors.accent)
        .bg(colors.accent_soft)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_its_own_accent_and_the_active_one_drives_the_styles() {
        let accents: Vec<Color> = CliTheme::ALL
            .iter()
            .map(|theme| colors_of(*theme).accent)
            .collect();
        for (index, accent) in accents.iter().enumerate() {
            assert!(!accents[index + 1..].contains(accent));
        }
        activate(CliTheme::Frost);
        assert_eq!(accent().fg, Some(colors_of(CliTheme::Frost).accent));
        assert_eq!(ember(0.0), rgb(0xc8f2f6));
        activate(CliTheme::Ember);
        assert_eq!(accent().fg, Some(rgb(0xf2914d)));
    }
}
