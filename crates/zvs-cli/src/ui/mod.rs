pub mod banner;
pub mod overlay;
pub mod prompt;
pub mod text;
pub mod theme;
pub mod transcript;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;
use zvs_contract::ExecMode;

use crate::copy;
use crate::state::palette::{COMMANDS, matches};
use crate::state::transcript::Tone;
use crate::state::{App, Phase};

const INPUT_MAX_LINES: usize = 8;
const PALETTE_ROWS: usize = 8;

pub use crate::state::overlay::mode_label;

fn panel(
    title: &str,
    color: ratatui::style::Color,
    background: ratatui::style::Color,
) -> Block<'static> {
    Block::new()
        .style(Style::new().bg(background))
        .padding(Padding::new(1, 1, 0, 1))
        .title(Line::from(Span::styled(
            format!(" {title}"),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        )))
}

fn rows(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn model_line(app: &App) -> String {
    app.model.as_ref().map_or_else(
        || copy::NO_MODEL.to_owned(),
        |model| format!("{} / {}", model.provider_name, model.model_name),
    )
}

fn welcome(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let width = usize::from(area.width);
    let mut lines = banner::lines(width.saturating_sub(4));
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled(copy::TITLE, theme::strong()),
        Span::styled(format!("  ·  {}", copy::SUBTITLE), theme::muted()),
        Span::styled(format!("  ·  {}", crate::version::label()), theme::faint()),
    ]));
    lines.push(Line::default());
    let card = [
        (copy::WORKSPACE, app.folder.clone()),
        (copy::MODEL, model_line(app)),
        (copy::MODE, mode_label(app.mode).to_owned()),
    ];
    for (label, value) in card {
        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), theme::muted()),
            Span::styled(
                text::truncate(&value, width.saturating_sub(24)),
                theme::text(),
            ),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(copy::WELCOME_HINT, theme::faint())));
    let height = rows(lines.len());
    let top = area.y + area.height.saturating_sub(height) / 3;
    let target = Rect {
        y: top,
        height: height.min(area.height),
        ..area
    };
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        target,
    );
}

fn transcript_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.show_welcome() {
        welcome(frame, area, app);
        return;
    }
    let lines = transcript::lines(app, usize::from(area.width));
    let height = usize::from(area.height);
    let bottom = lines.len().saturating_sub(height);
    let offset = bottom.saturating_sub(app.scroll.min(bottom));
    frame.render_widget(Paragraph::new(lines).scroll((rows(offset), 0)), area);
}

fn input_height(app: &App) -> u16 {
    let count = app
        .input
        .text()
        .split('\n')
        .count()
        .clamp(1, INPUT_MAX_LINES);
    rows(count + 2)
}

fn input_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let shell = app.shell_mode();
    let lead = if shell {
        Style::new().fg(theme::WARN).add_modifier(Modifier::BOLD)
    } else if app.busy() {
        theme::faint()
    } else {
        theme::accent().add_modifier(Modifier::BOLD)
    };
    let block = if shell {
        Block::new()
            .style(Style::new().bg(theme::SHELL_BG))
            .padding(Padding::new(1, 1, 0, 1))
            .title(Line::from(vec![
                Span::styled(
                    format!(" {} ", copy::SHELL_MODE),
                    Style::new()
                        .fg(theme::ON_WARN)
                        .bg(theme::WARN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", app.folder), theme::faint()),
            ]))
    } else {
        Block::new()
            .style(Style::new().bg(theme::PANEL_BG))
            .padding(Padding::new(1, 1, 1, 1))
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = app.input.text();
    let (row, column) = app.input.lines_before_cursor();
    let visible = usize::from(inner.height.max(1));
    let first = row.saturating_sub(visible - 1);
    let lines: Vec<Line<'static>> = if text.is_empty() {
        vec![Line::from(vec![
            Span::styled("› ", lead),
            Span::styled(copy::INPUT_PLACEHOLDER, theme::faint()),
        ])]
    } else {
        text.split('\n')
            .enumerate()
            .skip(first)
            .map(|(index, line)| {
                let prompt = match (index, shell) {
                    (0, true) => "$ ",
                    (0, false) => "› ",
                    _ => "  ",
                };
                let body = if index == 0 && shell {
                    line.strip_prefix('!').unwrap_or(line)
                } else {
                    line
                };
                Line::from(vec![
                    Span::styled(prompt, lead),
                    Span::styled(body.to_owned(), theme::text()),
                ])
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), inner);
    if app.prompts.is_empty() && !app.help && app.overlay.is_none() {
        let shift = usize::from(shell && row == 0);
        let x = inner.x + rows(column + 2 - shift).min(inner.width.saturating_sub(1));
        let y = inner.y + rows(row - first);
        frame.set_cursor_position((x, y));
    }
}

fn palette_view(frame: &mut Frame<'_>, above: Rect, app: &App) {
    let found = matches(app.input.text());
    if found.is_empty() {
        return;
    }
    let selected = app.palette.selected().min(found.len() - 1);
    let start = selected.saturating_sub(PALETTE_ROWS - 1);
    let width = above.width.saturating_sub(2);
    let height = rows(found.len().min(PALETTE_ROWS) + 2);
    let area = Rect {
        x: above.x + 1,
        y: above.y + above.height.saturating_sub(height),
        width,
        height: height.min(above.height),
    };
    let name_width = COMMANDS
        .iter()
        .map(|command| command.name().width())
        .max()
        .unwrap_or(0)
        + 2;
    let lines: Vec<Line<'static>> = found
        .iter()
        .enumerate()
        .skip(start)
        .take(PALETTE_ROWS)
        .map(|(index, command)| {
            let name = format!("/{:<name_width$}", command.name());
            if index == selected {
                Line::from(vec![
                    Span::styled(format!(" {name}"), theme::selected()),
                    Span::styled(format!("{} ", command.description()), theme::selected()),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!(" {name}"), theme::accent()),
                    Span::styled(command.description(), theme::muted()),
                ])
            }
        })
        .collect();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(panel(copy::COMMANDS_TITLE, theme::MUTED, theme::PANEL_BG)),
        area,
    );
}

fn help_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![Line::from(Span::styled(
        copy::COMMANDS_TITLE,
        theme::eyebrow(),
    ))];
    for command in COMMANDS {
        lines.push(Line::from(vec![
            Span::styled(format!("  /{:<13}", command.name()), theme::accent()),
            Span::styled(command.description(), theme::text()),
        ]));
    }
    lines.push(Line::default());
    lines.extend(transcript::key_lines(&app.key_sections(), 70, 0));
    let target = centered(area, 72, rows(lines.len() + 2));
    frame.render_widget(Clear, target);
    frame.render_widget(
        Paragraph::new(lines).block(panel(copy::HELP_TITLE, theme::ACCENT, theme::PANEL_BG)),
        target,
    );
}

fn footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let [top, bottom] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let mode_color = match app.mode {
        ExecMode::NoRestrictions => theme::WARN,
        ExecMode::AlwaysAsk => theme::MUTED,
        ExecMode::SafetyAsk | ExecMode::AutoBypass => theme::ACCENT,
    };
    let left = vec![
        Span::styled(" ◆ ", Style::new().fg(mode_color)),
        Span::styled(mode_label(app.mode), Style::new().fg(mode_color)),
        Span::styled("  ·  ", theme::faint()),
        Span::styled(model_line(app), theme::muted()),
        Span::styled("  ·  ", theme::faint()),
        Span::styled(
            crate::state::overlay::effort_label(app.effort),
            theme::muted(),
        ),
    ];
    let used: usize = left.iter().map(|span| span.content.width()).sum();
    let room = usize::from(top.width).saturating_sub(used + 2);
    let folder = text::truncate(&app.folder, room);
    let pad = room.saturating_sub(folder.width());
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(pad + 1)));
    spans.push(Span::styled(folder, theme::faint()));
    frame.render_widget(Paragraph::new(Line::from(spans)), top);

    let hint = if let Some((tone, message)) = &app.status {
        let color = match tone {
            Tone::Info => theme::MUTED,
            Tone::Success => theme::OK,
            Tone::Error => theme::ERR,
        };
        Line::from(Span::styled(format!(" {message}"), Style::new().fg(color)))
    } else {
        let hint = if !app.prompts.is_empty() {
            copy::HINT_PROMPT
        } else if app.shell_mode() {
            copy::HINT_SHELL
        } else if app.shell.is_some() {
            copy::HINT_SHELL_RUNNING
        } else if app.verification.is_some() {
            copy::HINT_VERIFY
        } else if app.capturing() {
            copy::HINT_CAPTURE
        } else if app.overlay.is_some() {
            copy::HINT_PANEL
        } else if app.palette_open() {
            copy::HINT_PALETTE
        } else if app.busy() {
            copy::HINT_RUNNING
        } else {
            copy::HINT_IDLE
        };
        Line::from(Span::styled(format!(" {hint}"), theme::faint()))
    };
    frame.render_widget(Paragraph::new(hint), bottom);
}

fn prompt_view(frame: &mut Frame<'_>, area: Rect, content: prompt::Panel) {
    let block = panel(&content.title, theme::WARN, theme::PROMPT_BG);
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(content.lines).block(block), area);
    if let Some((column, row)) = content.cursor {
        frame.set_cursor_position((inner.x + column, inner.y + row));
    }
}

fn splash(frame: &mut Frame<'_>, area: Rect, message: &str, app: &App) {
    let mut lines = banner::lines(usize::from(area.width).saturating_sub(4));
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", transcript::spinner(app.frame)),
            theme::accent(),
        ),
        Span::styled(message.to_owned(), theme::muted()),
    ]));
    let target = centered(area, area.width, rows(lines.len()));
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        target,
    );
}

fn access(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(current) = app.prompts.front() else {
        return;
    };
    let art = banner::lines(usize::from(area.width).saturating_sub(4));
    let width = area.width.min(76);
    let panel = prompt::panel(current, usize::from(width.saturating_sub(2)));
    let panel_height = rows(panel.lines.len() + 2);
    let art_height = rows(art.len() + 1);
    let total = art_height + panel_height;
    let top = area.y + area.height.saturating_sub(total) / 2;
    frame.render_widget(
        Paragraph::new(art).alignment(ratatui::layout::Alignment::Center),
        Rect {
            y: top,
            height: art_height.min(area.height),
            ..area
        },
    );
    let target = Rect {
        x: area.x + (area.width - width) / 2,
        y: (top + art_height).min(area.bottom().saturating_sub(1)),
        width,
        height: panel_height.min(area.height.saturating_sub(art_height)),
    };
    prompt_view(frame, target, panel);
}

fn highlight(frame: &mut Frame<'_>, app: &App) {
    let Some(selection) = app.selection else {
        return;
    };
    let area = frame.area();
    let buffer = frame.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if selection.contains(x, y) {
                buffer[(x, y)].set_bg(theme::SELECTION_BG);
            }
        }
    }
}

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    draw_screen(frame, app);
    highlight(frame, app);
}

fn draw_screen(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    match app.phase {
        Phase::Connecting(message) => splash(frame, area, message, app),
        Phase::Access => access(frame, area, app),
        Phase::Chat => {
            let width = usize::from(area.width.saturating_sub(2));
            let panel = app
                .prompts
                .front()
                .map(|current| prompt::panel(current, width))
                .or_else(|| {
                    app.verification
                        .as_deref()
                        .map(|origin| prompt::verification(origin, width))
                });
            let bottom_height = panel
                .as_ref()
                .map_or_else(|| input_height(app), |panel| rows(panel.lines.len() + 2))
                .min(area.height.saturating_sub(4));
            let [body, bottom, status] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(bottom_height),
                Constraint::Length(2),
            ])
            .areas(area);
            match &app.overlay {
                Some(current) => overlay::draw(frame, body, app, current),
                None => transcript_view(frame, body, app),
            }
            match panel {
                Some(panel) => prompt_view(frame, bottom, panel),
                None => input_view(frame, bottom, app),
            }
            if app.palette_open() {
                palette_view(frame, body, app);
            }
            footer(frame, status, app);
            if app.help {
                help_view(frame, area, app);
            }
        }
    }
}

#[cfg(test)]
mod tests;
