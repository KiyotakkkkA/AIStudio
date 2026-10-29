use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::theme;

const GLYPH_A: [&str; 6] = [
    " █████╗ ",
    "██╔══██╗",
    "███████║",
    "██╔══██║",
    "██║  ██║",
    "╚═╝  ╚═╝",
];
const GLYPH_I: [&str; 6] = ["██╗", "██║", "██║", "██║", "██║", "╚═╝"];
const GLYPH_S: [&str; 6] = [
    "███████╗",
    "██╔════╝",
    "███████╗",
    "╚════██║",
    "███████║",
    "╚══════╝",
];
const GLYPH_T: [&str; 6] = [
    "████████╗",
    "╚══██╔══╝",
    "   ██║   ",
    "   ██║   ",
    "   ██║   ",
    "   ╚═╝   ",
];
const GLYPH_U: [&str; 6] = [
    "██╗   ██╗",
    "██║   ██║",
    "██║   ██║",
    "██║   ██║",
    "╚██████╔╝",
    " ╚═════╝ ",
];
const GLYPH_D: [&str; 6] = [
    "██████╗ ",
    "██╔══██╗",
    "██║  ██║",
    "██║  ██║",
    "██████╔╝",
    "╚═════╝ ",
];
const GLYPH_O: [&str; 6] = [
    " ██████╗ ",
    "██╔═══██╗",
    "██║   ██║",
    "██║   ██║",
    "╚██████╔╝",
    " ╚═════╝ ",
];
const GAP: [&str; 6] = ["   "; 6];
const WORD: [[&str; 6]; 9] = [
    GLYPH_A, GLYPH_I, GAP, GLYPH_S, GLYPH_T, GLYPH_U, GLYPH_D, GLYPH_I, GLYPH_O,
];
const COMPACT: &str = "A I   S T U D I O";

pub fn rows() -> Vec<String> {
    (0..6)
        .map(|row| WORD.iter().map(|glyph| glyph[row]).collect())
        .collect()
}

pub fn width() -> usize {
    rows().iter().map(|row| row.width()).max().unwrap_or(0)
}

fn shade(character: char, color: ratatui::style::Color) -> Style {
    if character == '█' {
        Style::new().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::dim(color, 0.55))
    }
}

pub fn lines(available: usize) -> Vec<Line<'static>> {
    if available < width() {
        let span_count = COMPACT.chars().count().max(1);
        return vec![Line::from(
            COMPACT
                .chars()
                .enumerate()
                .map(|(index, character)| {
                    let position = index as f64 / span_count as f64;
                    Span::styled(
                        character.to_string(),
                        Style::new()
                            .fg(theme::ember(position))
                            .add_modifier(Modifier::BOLD),
                    )
                })
                .collect::<Vec<_>>(),
        )];
    }
    let rows = rows();
    let total = (width() + rows.len() * 2) as f64;
    rows.iter()
        .enumerate()
        .map(|(row, text)| {
            Line::from(
                text.chars()
                    .enumerate()
                    .map(|(column, character)| {
                        let position = (column + row * 2) as f64 / total;
                        let color = theme::ember(position);
                        Span::styled(character.to_string(), shade(character, color))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_of_the_art_has_the_same_width() {
        let rows = rows();
        assert_eq!(rows.len(), 6);
        assert!(rows.iter().all(|row| row.width() == width()));
        assert_eq!(width(), 60);
    }

    #[test]
    fn a_narrow_terminal_gets_the_one_line_title() {
        assert_eq!(lines(40).len(), 1);
        assert_eq!(lines(100).len(), 6);
    }
}
