use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::text::{truncate, wrap};
use super::theme;
use super::transcript::spinner;
use crate::copy;
use crate::state::App;
use crate::state::overlay::{Overlay, Panel, Row, Tone};

const GAUGE_WIDTH: usize = 24;

const fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Plain => theme::INK,
        Tone::Accent => theme::ACCENT,
        Tone::Good => theme::OK,
        Tone::Warn => theme::WARN,
        Tone::Bad => theme::ERR,
        Tone::Faint => theme::FAINT,
    }
}

fn tabs_line(names: &[String], active: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, name) in names.iter().enumerate() {
        let style = if index == active {
            theme::selected()
        } else {
            theme::muted()
        };
        spans.push(Span::styled(format!(" {name} "), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn gauge(value: f64, color: Color) -> Vec<Span<'static>> {
    let filled = ((value.clamp(0.0, 1.0) * GAUGE_WIDTH as f64).round() as usize).min(GAUGE_WIDTH);
    vec![
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled("░".repeat(GAUGE_WIDTH - filled), theme::faint()),
        Span::raw("  "),
    ]
}

pub fn key_column(rows: &[Row]) -> usize {
    rows.iter()
        .map(|row| row.meta.width())
        .chain([
            copy::SHORTCUT_PRESS.width(),
            copy::SHORTCUT_COLUMN_KEYS.width(),
        ])
        .max()
        .unwrap_or(0)
        + 3
}

fn table_header(column: usize) -> Line<'static> {
    let keys = copy::SHORTCUT_COLUMN_KEYS;
    Line::from(vec![
        Span::raw("     "),
        Span::styled(
            format!("{keys}{}", " ".repeat(column.saturating_sub(keys.width()))),
            theme::eyebrow(),
        ),
        Span::styled(copy::SHORTCUT_COLUMN_ACTION, theme::eyebrow()),
    ])
}

fn table_row(row: &Row, selected: bool, column: usize, width: usize) -> Line<'static> {
    let color = tone_color(row.tone);
    let base = if selected {
        Style::new().bg(theme::ACCENT_SOFT)
    } else {
        Style::new()
    };
    let marker = if selected { " › " } else { "   " };
    let keys_style = match row.tone {
        Tone::Faint => theme::faint(),
        Tone::Warn => Style::new().fg(theme::WARN).add_modifier(Modifier::BOLD),
        _ => theme::strong(),
    };
    let keys = truncate(&row.meta, column.saturating_sub(1));
    let pad = column.saturating_sub(keys.width());
    let mut spans = vec![
        Span::styled(marker, base.fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{} ", row.icon), base.fg(color)),
        Span::styled(keys, base.patch(keys_style)),
        Span::styled(" ".repeat(pad), base),
    ];
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let title = truncate(&row.title, width.saturating_sub(used + 1));
    let title_width = title.width();
    let title_style = if row.tone == Tone::Accent {
        Style::new().fg(theme::ACCENT)
    } else {
        theme::text()
    };
    spans.push(Span::styled(title, base.patch(title_style)));
    if selected {
        spans.push(Span::styled(
            " ".repeat(width.saturating_sub(used + title_width)),
            base,
        ));
    }
    Line::from(spans)
}

fn row_lines(row: &Row, selected: bool, expanded: bool, width: usize) -> Vec<Line<'static>> {
    let color = tone_color(row.tone);
    let base = if selected {
        Style::new().bg(theme::ACCENT_SOFT)
    } else {
        Style::new()
    };
    let marker = if selected { " › " } else { "   " };
    let title_style = if row.tone == Tone::Faint {
        theme::faint()
    } else if row.current {
        Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)
    } else {
        theme::text()
    };
    let mut spans = vec![
        Span::styled(marker, base.fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{} ", row.icon), base.fg(color)),
    ];
    let title_width = if row.gauge.is_some() { 24 } else { width / 2 };
    let title = truncate(&row.title, title_width);
    let pad = if row.gauge.is_some() {
        title_width.saturating_sub(title.width())
    } else {
        2
    };
    spans.push(Span::styled(title, base.patch(title_style)));
    spans.push(Span::styled(" ".repeat(pad + 1), base));
    if let Some(value) = row.gauge {
        spans.extend(gauge(value, color).into_iter().map(|span| {
            let style = base.patch(span.style);
            span.style(style)
        }));
    }
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let meta = truncate(&row.meta, width.saturating_sub(used + 1));
    let meta_width = meta.width();
    spans.push(Span::styled(meta, base.fg(theme::MUTED)));
    let fill = width.saturating_sub(used + meta_width);
    if selected {
        spans.push(Span::styled(" ".repeat(fill), base));
    }
    let mut lines = vec![Line::from(spans)];
    if expanded {
        for detail in &row.detail {
            for piece in wrap(detail, width.saturating_sub(8).max(8)) {
                lines.push(Line::from(vec![
                    Span::raw("       "),
                    Span::styled(piece, theme::muted()),
                ]));
            }
        }
    }
    lines
}

fn empty_text(overlay: &Overlay) -> &'static str {
    if overlay.panel == Panel::Resume && overlay.filter.is_empty() {
        copy::NO_CONVERSATIONS
    } else {
        copy::NOTHING_FOUND
    }
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App, overlay: &Overlay) {
    frame.render_widget(Clear, area);
    let view = app.view();
    let rows = overlay.rows(&view);
    let tabs = overlay.tabs(&view);
    let [title, tabs_area, search, _, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", overlay.icon()), theme::accent()),
            Span::styled(
                overlay.title(),
                theme::accent().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", rows.len()), theme::faint()),
        ])),
        title,
    );
    frame.render_widget(Paragraph::new(tabs_line(&tabs, overlay.tab)), tabs_area);

    let filter = overlay.filter.text();
    let search_line = if filter.is_empty() {
        Line::from(vec![
            Span::styled(" ⌕ ", theme::accent()),
            Span::styled(copy::SEARCH_PLACEHOLDER, theme::faint()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ⌕ ", theme::accent()),
            Span::styled(filter.to_owned(), theme::text()),
        ])
    };
    frame.render_widget(
        Paragraph::new(search_line).style(Style::new().bg(theme::PANEL_BG)),
        search,
    );
    if app.prompts.is_empty() {
        let (_, column) = overlay.filter.lines_before_cursor();
        let x = search.x + u16::try_from(column + 3).unwrap_or(u16::MAX);
        frame.set_cursor_position((x.min(search.right().saturating_sub(1)), search.y));
    }

    if overlay.loading {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("   {} ", spinner(app.frame)), theme::accent()),
                Span::styled(copy::LOADING, theme::muted()),
            ])),
            list,
        );
        return;
    }
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("   {}", empty_text(overlay)),
                theme::faint(),
            ))),
            list,
        );
        return;
    }
    let width = usize::from(list.width);
    let selected = overlay.selected.min(rows.len() - 1);
    let mut lines = Vec::new();
    let mut selected_span = (0, 0);
    let table = (overlay.panel == Panel::Shortcuts).then(|| key_column(&rows));
    if let Some(column) = table {
        lines.push(table_header(column));
    }
    for (index, row) in rows.iter().enumerate() {
        let start = lines.len();
        match table {
            Some(column) => lines.push(table_row(row, index == selected, column, width)),
            None => lines.extend(row_lines(
                row,
                index == selected,
                overlay.is_expanded(&row.key),
                width,
            )),
        }
        if index == selected {
            selected_span = (start, lines.len());
        }
    }
    let height = usize::from(list.height);
    let offset = selected_span.1.saturating_sub(height).min(selected_span.0);
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        list,
    );
}
