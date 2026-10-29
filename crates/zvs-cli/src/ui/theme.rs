use ratatui::style::{Color, Modifier, Style};

pub const EMBER: [(u8, u8, u8); 6] = [
    (0xff, 0xd0, 0x8a),
    (0xff, 0xb2, 0x7a),
    (0xf2, 0x91, 0x4d),
    (0xe5, 0x75, 0x32),
    (0xc2, 0x57, 0x1c),
    (0x9a, 0x42, 0x14),
];

pub const ACCENT: Color = Color::Rgb(0xf2, 0x91, 0x4d);
pub const ACCENT_SOFT: Color = Color::Rgb(0x3a, 0x24, 0x16);
pub const INK: Color = Color::Rgb(0xec, 0xe3, 0xd9);
pub const MUTED: Color = Color::Rgb(0x9c, 0x8e, 0x80);
pub const FAINT: Color = Color::Rgb(0x5e, 0x53, 0x49);
pub const LINE: Color = Color::Rgb(0x4a, 0x3f, 0x36);
pub const OK: Color = Color::Rgb(0x9c, 0xc0, 0x6b);
pub const WARN: Color = Color::Rgb(0xe8, 0xb8, 0x4a);
pub const ERR: Color = Color::Rgb(0xe8, 0x6a, 0x58);
pub const USER_BG: Color = Color::Rgb(0x2b, 0x22, 0x1c);
pub const TOOL_BG: Color = Color::Rgb(0x30, 0x27, 0x20);
pub const PANEL_BG: Color = Color::Rgb(0x22, 0x1c, 0x17);
pub const PROMPT_BG: Color = Color::Rgb(0x2e, 0x24, 0x14);
pub const EMBER_SOFT: Color = Color::Rgb(0xf0, 0xc9, 0xad);
pub const SHELL_BG: Color = Color::Rgb(0x2a, 0x24, 0x12);
pub const ON_WARN: Color = Color::Rgb(0x1a, 0x14, 0x08);
pub const CODE_BG: Color = Color::Rgb(0x1c, 0x18, 0x14);

pub fn ember(position: f64) -> Color {
    let position = position.clamp(0.0, 1.0) * (EMBER.len() - 1) as f64;
    let index = (position.floor() as usize).min(EMBER.len() - 2);
    let fraction = position - index as f64;
    let (from, to) = (EMBER[index], EMBER[index + 1]);
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
    Style::new().fg(INK)
}

pub fn muted() -> Style {
    Style::new().fg(MUTED)
}

pub fn faint() -> Style {
    Style::new().fg(FAINT)
}

pub fn accent() -> Style {
    Style::new().fg(ACCENT)
}

pub fn strong() -> Style {
    Style::new().fg(INK).add_modifier(Modifier::BOLD)
}

pub fn eyebrow() -> Style {
    Style::new().fg(MUTED).add_modifier(Modifier::BOLD)
}

pub fn selected() -> Style {
    Style::new()
        .fg(ACCENT)
        .bg(ACCENT_SOFT)
        .add_modifier(Modifier::BOLD)
}
