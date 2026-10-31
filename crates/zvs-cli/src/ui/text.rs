use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::theme;

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split_inclusive(' ') {
            let size = word.width();
            if used + size > width && used > 0 {
                lines.push(line.trim_end().to_owned());
                line.clear();
                used = 0;
            }
            if size > width {
                for character in word.chars() {
                    let step = character.width().unwrap_or(0);
                    if used + step > width {
                        lines.push(std::mem::take(&mut line));
                        used = 0;
                    }
                    line.push(character);
                    used += step;
                }
            } else {
                line.push_str(word);
                used += size;
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    lines
}

pub fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let step = character.width().unwrap_or(0);
        if used + step + 1 > width {
            break;
        }
        out.push(character);
        used += step;
    }
    out.push('…');
    out
}

pub fn inline(text: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let code = rest.find('`');
        let bold = rest.find("**");
        let (marker, at) = match (code, bold) {
            (Some(code), Some(bold)) if bold < code => ("**", bold),
            (Some(code), _) => ("`", code),
            (None, Some(bold)) => ("**", bold),
            (None, None) => {
                spans.push(Span::styled(rest.to_owned(), base));
                break;
            }
        };
        let after = &rest[at + marker.len()..];
        let Some(close) = after.find(marker) else {
            spans.push(Span::styled(rest.to_owned(), base));
            break;
        };
        if at > 0 {
            spans.push(Span::styled(rest[..at].to_owned(), base));
        }
        let inner = after[..close].to_owned();
        spans.push(if marker == "`" {
            Span::styled(inner, theme::accent())
        } else {
            Span::styled(inner, base.add_modifier(Modifier::BOLD))
        });
        rest = &after[close + marker.len()..];
    }
    spans
}

pub fn markdown(text: &str, width: usize, indent: &'static str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut fenced = false;
    let room = width.saturating_sub(indent.width() + 2).max(8);
    for raw in text.split('\n') {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            let language = trimmed.trim_start_matches('`').trim();
            if fenced && !language.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        format!(" {language} "),
                        theme::eyebrow().bg(theme::colors().code_bg),
                    ),
                ]));
            }
            continue;
        }
        if fenced {
            for piece in wrap(raw, room) {
                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        format!(" {piece} "),
                        Style::new()
                            .fg(theme::colors().accent_light)
                            .bg(theme::colors().code_bg),
                    ),
                ]));
            }
            continue;
        }
        let heading = trimmed.starts_with('#');
        let (marker, body) = if heading {
            ("", trimmed.trim_start_matches('#').trim_start())
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            ("• ", item)
        } else {
            ("", raw)
        };
        let base = if heading {
            theme::strong()
        } else {
            theme::text()
        };
        let wrapped = wrap(body, room.saturating_sub(marker.width()).max(4));
        for (index, piece) in wrapped.into_iter().enumerate() {
            let lead = if index == 0 { marker } else { "  " };
            let mut spans = vec![Span::raw(indent)];
            if !marker.is_empty() {
                spans.push(Span::styled(lead, theme::accent()));
            }
            spans.extend(inline(&piece, base));
            lines.push(Line::from(spans));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn wrapping_breaks_on_spaces_and_splits_long_words() {
        assert_eq!(wrap("один два три", 8), ["один", "два три"]);
        assert_eq!(wrap("абвгдежзик", 4), ["абвг", "дежз", "ик"]);
        assert_eq!(wrap("a\n\nb", 10), ["a", "", "b"]);
    }

    #[test]
    fn truncation_counts_display_width() {
        assert_eq!(truncate("короткий", 20), "короткий");
        assert_eq!(truncate("очень длинная строка", 8), "очень д…");
    }

    #[test]
    fn inline_code_and_bold_become_their_own_spans() {
        let spans = inline("run `cargo test` **now** ok", theme::text());
        let parts: Vec<&str> = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(parts, ["run ", "cargo test", " ", "now", " ok"]);
        assert_eq!(spans[1].style, theme::accent());
        assert_eq!(inline("a `b", theme::text()).len(), 1);
    }

    #[test]
    fn lists_headings_and_fences_are_shaped() {
        let lines = markdown("# Итог\n- пункт\n```rust\nfn main() {}\n```", 40, "  ");
        let text: Vec<String> = lines.iter().map(plain).collect();
        assert_eq!(
            text,
            ["  Итог", "  • пункт", "   rust ", "   fn main() {} "]
        );
    }
}
