pub mod activity;
pub mod banner;
pub mod modal;
mod overlay;
pub mod prompt;
mod scrollbar;
pub mod text;
pub mod theme;
pub mod transcript;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::copy;
use crate::state::overlay::Panel;
use crate::state::palette::{loading, matches};
use crate::state::shortcuts::Shortcut;
use crate::state::transcript::Tone;
use crate::state::{App, Phase};

const INPUT_MAX_LINES: usize = 8;
const PALETTE_ROWS: usize = 8;
const REFERENCE_ROWS: usize = 18;
const QUEUE_ROWS: usize = 5;
const ACCESS_WIDTH: u16 = 76;

pub use crate::state::overlay::mode_label;

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

fn welcome(frame: &mut Frame<'_>, area: Rect) {
    let mut lines = banner::lines(
        usize::from(area.width).saturating_sub(4),
        area.height.saturating_sub(2),
    );
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.push(Line::from(vec![
        Span::styled(copy::TITLE, theme::strong()),
        Span::styled(format!("  ·  {}", copy::SUBTITLE), theme::muted()),
        Span::styled(format!("  ·  {}", crate::version::label()), theme::faint()),
    ]));
    let target = centered(area, area.width, rows(lines.len()));
    frame.render_widget(
        Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center),
        target,
    );
}

fn transcript_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.show_welcome() {
        welcome(frame, area);
        return;
    }
    let text = transcript_text_area(area);
    let lines = transcript::lines(app, usize::from(text.width));
    let height = usize::from(area.height);
    let bottom = lines.len().saturating_sub(height);
    let offset = bottom.saturating_sub(app.scroll.min(bottom));
    let target = transcript_area(text, lines.len());
    scrollbar::draw(frame, area, lines.len(), offset, true);
    let visible: Vec<_> = lines.into_iter().skip(offset).take(height).collect();
    frame.render_widget(Paragraph::new(visible), target);
}

fn transcript_text_area(area: Rect) -> Rect {
    Rect {
        width: area.width - u16::from(area.width > 1),
        ..area
    }
}

fn transcript_area(area: Rect, line_count: usize) -> Rect {
    let height = area.height.min(rows(line_count));
    Rect {
        y: area.y + area.height - height,
        height,
        ..area
    }
}

fn reference_filter(app: &App) -> Option<&crate::state::input::Input> {
    app.overlay
        .as_ref()
        .filter(|overlay| overlay.floating_reference)
        .map(|overlay| &overlay.filter)
}

fn input_layout(app: &App, width: u16) -> (Vec<std::ops::Range<usize>>, (usize, usize)) {
    reference_filter(app)
        .unwrap_or(&app.input)
        .visual_layout(usize::from(width.saturating_sub(4)), app.shell_mode())
}

fn input_height(app: &App, width: u16) -> u16 {
    let (lines, _) = input_layout(app, width);
    rows(lines.len().clamp(1, INPUT_MAX_LINES) + 2)
}

pub fn queue_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    if app.queue.is_empty() {
        return Vec::new();
    }
    let mut title = format!(" {} · {}", copy::QUEUE_TITLE, copy::queued(app.queue.len()));
    if app.queue_urgent {
        title.push_str(&format!(" · {}", copy::QUEUE_SENDING));
    }
    let rule = "┄".repeat(width.saturating_sub(title.width() + 2));
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("┄{title} "), theme::muted()),
        Span::styled(rule, theme::faint()),
    ])];
    let selected = app.queue_selected.unwrap_or(usize::MAX);
    let start = selected
        .min(app.queue.len().saturating_sub(1))
        .saturating_sub(QUEUE_ROWS - 1)
        .min(app.queue.len().saturating_sub(QUEUE_ROWS));
    for (index, message) in app.queue.iter().enumerate().skip(start).take(QUEUE_ROWS) {
        let chosen = index == selected;
        let editing = app
            .queue_editing
            .as_ref()
            .is_some_and(|(id, _)| *id == message.id);
        let marker = if chosen { " › " } else { "   " };
        let flat = message
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let body = text::truncate(&flat, width.saturating_sub(4));
        let style = if editing || chosen {
            theme::accent()
        } else {
            theme::muted()
        };
        lines.push(Line::from(vec![
            Span::styled(marker, theme::accent()),
            Span::styled(body, style),
        ]));
    }
    let key = |shortcut| app.keymap.label(shortcut);
    let hint = format!(
        " {}/{} {} · {} {} · {} {} · {} {}",
        key(Shortcut::QueuePrevious),
        key(Shortcut::QueueNext),
        copy::QUEUE_SELECT,
        key(Shortcut::QueueEdit),
        copy::QUEUE_EDIT,
        key(Shortcut::QueueRemove),
        copy::QUEUE_REMOVE,
        key(Shortcut::QueueSend),
        copy::QUEUE_SEND,
    );
    lines.push(Line::from(Span::styled(
        text::truncate(&hint, width),
        theme::faint(),
    )));
    lines
}

fn queue_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new(queue_lines(app, usize::from(area.width))),
        area,
    );
}

fn input_view(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let shell = app.shell_mode();
    let lead = if shell {
        Style::new()
            .fg(theme::colors().warn)
            .add_modifier(Modifier::BOLD)
    } else if app.busy() {
        theme::faint()
    } else {
        theme::accent().add_modifier(Modifier::BOLD)
    };
    let block = if shell {
        Block::new()
            .padding(Padding::new(1, 1, 0, 1))
            .title(Line::from(vec![
                Span::styled(
                    format!(" {} ", copy::SHELL_MODE),
                    Style::new()
                        .fg(theme::colors().warn)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", app.folder), theme::faint()),
            ]))
    } else {
        Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme::faint())
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let filter = reference_filter(app);
    let input = filter.unwrap_or(&app.input);
    let text = input.text();
    let (wrapped, (row, column)) = input_layout(app, area.width);
    let visible = usize::from(inner.height.max(1));
    let first = row.saturating_sub(visible - 1);
    let lines: Vec<Line<'static>> = if text.is_empty() {
        vec![Line::from(vec![
            Span::styled("› ", lead),
            Span::styled(
                if filter.is_some() {
                    copy::SEARCH_PLACEHOLDER
                } else {
                    copy::INPUT_PLACEHOLDER
                },
                theme::faint(),
            ),
        ])]
    } else {
        wrapped
            .into_iter()
            .enumerate()
            .skip(first)
            .map(|(index, range)| {
                let prompt = match (index, shell) {
                    (0, true) => "$ ",
                    (0, false) => "› ",
                    _ => "  ",
                };
                let mut spans = vec![Span::styled(prompt, lead)];
                if shell {
                    spans.push(Span::styled(text[range].to_owned(), theme::text()));
                } else {
                    let mut offset = 0;
                    spans.extend(input.pieces(0, text.len()).into_iter().filter_map(
                        |(piece, attachment)| {
                            let start = range.start.max(offset);
                            let end = range.end.min(offset + piece.len());
                            offset += piece.len();
                            (start < end).then(|| {
                                Span::styled(
                                    text[start..end].to_owned(),
                                    attachment.map_or_else(theme::text, |attachment| {
                                        let color = if attachment.image() {
                                            theme::colors().ok
                                        } else {
                                            theme::colors().accent
                                        };
                                        Style::new().fg(color).add_modifier(Modifier::BOLD)
                                    }),
                                )
                            })
                        },
                    ));
                }
                if input.selected_all() {
                    for span in spans.iter_mut().skip(1) {
                        span.style = span.style.bg(theme::colors().selection_bg);
                    }
                }
                Line::from(spans)
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), inner);
    if !inner.is_empty()
        && app.prompts.is_empty()
        && app.modal.is_none()
        && (app.overlay.is_none() || filter.is_some() && !app.capturing())
    {
        let x = inner.x + rows(column + 2).min(inner.width.saturating_sub(1));
        let y = inner.y + rows(row - first).min(inner.height.saturating_sub(1));
        frame.set_cursor_position((x, y));
    }
}

fn picker_lines(app: &App, width: usize, limit: usize, attachments: bool) -> Vec<Line<'static>> {
    let row = |index, selected, label: String| {
        let marker = if index == selected { " › " } else { "   " };
        let style = if index == selected {
            theme::accent()
        } else {
            theme::text()
        };
        Line::from(Span::styled(
            format!(
                "{marker}{}",
                text::truncate(&label, width.saturating_sub(3))
            ),
            style,
        ))
    };
    if attachments {
        let choices = app.attachment_choices();
        let start = app
            .attachment_selected
            .saturating_add(1)
            .saturating_sub(limit);
        return choices
            .iter()
            .enumerate()
            .skip(start)
            .take(limit)
            .map(|(index, choice)| {
                let kind = if choice.image() {
                    copy::IMAGE_KIND
                } else {
                    copy::STORE_KIND
                };
                row(
                    index,
                    app.attachment_selected,
                    format!("{} · {kind}", choice.name()),
                )
            })
            .collect();
    }
    let sources = app.sources();
    let found = matches(app.input.text(), &sources);
    if found.is_empty() && loading(app.input.text(), &sources) {
        return vec![Line::from(vec![
            Span::styled(
                format!(" {} ", transcript::spinner(app.frame)),
                theme::accent(),
            ),
            Span::styled(copy::LOADING, theme::muted()),
        ])];
    }
    let selected = app.palette.selected().min(found.len().saturating_sub(1));
    let start = selected.saturating_add(1).saturating_sub(limit);
    found
        .iter()
        .enumerate()
        .skip(start)
        .take(limit)
        .map(|(index, item)| {
            row(
                index,
                selected,
                format!("{}  ·  {}", item.shown(), item.description()),
            )
        })
        .collect()
}

fn hint_line(hint: &str, width: usize) -> Line<'static> {
    let mut shown = String::new();
    for segment in hint.split(" · ") {
        let gap = if shown.is_empty() { " " } else { " · " };
        if shown.width() + gap.width() + segment.width() <= width {
            shown.push_str(gap);
            shown.push_str(segment);
        }
    }
    Line::from(Span::styled(shown, theme::muted()))
}

fn sheet_rule(title: &str, count: Option<usize>, width: usize) -> Line<'static> {
    let count = count.map_or_else(String::new, |count| {
        format!(" {} ", crate::format::number(count as u64))
    });
    let title = text::truncate(title, width.saturating_sub(count.width() + 5));
    let fill = width.saturating_sub(title.width() + count.width() + 4);
    Line::from(vec![
        Span::styled("─ ", theme::faint()),
        Span::styled(title, theme::strong()),
        Span::styled(format!(" {}{count}─", "─".repeat(fill)), theme::muted()),
    ])
}

enum SheetBody<'a> {
    Overlay(&'a crate::state::overlay::Overlay),
    Lines(Vec<Line<'static>>, usize, Option<(u16, u16)>),
}

struct Sheet<'a> {
    title: std::borrow::Cow<'a, str>,
    body: SheetBody<'a>,
    count: Option<usize>,
    hint: &'static str,
    height: u16,
    floating: bool,
}

fn sheet_min_height(area: Rect) -> u16 {
    area.height - area.height / 4
}

fn sheet_limit(area: Rect) -> u16 {
    area.height.saturating_sub(1).max(sheet_min_height(area))
}

fn sheet_height(area: Rect, body_lines: usize) -> u16 {
    rows(body_lines + 2).clamp(sheet_min_height(area), sheet_limit(area))
}

fn floating_sheet_limit(app: &App, area: Rect, max_height: u16) -> u16 {
    let [history, ..] = chat_areas(app, area, 0, true);
    let available = history.height.saturating_sub(u16::from(history.height > 3));
    available.min(max_height)
}

fn active_sheet(app: &App, area: Rect) -> Option<Sheet<'_>> {
    let cap = sheet_limit(area);
    let width = usize::from(area.width);
    let room = usize::from(cap.saturating_sub(2));
    let lines_sheet = |title, lines: Vec<Line<'static>>, offset, cursor, hint| {
        let height = sheet_height(area, lines.len());
        Sheet {
            title,
            body: SheetBody::Lines(lines, offset, cursor),
            count: None,
            hint,
            height,
            floating: false,
        }
    };
    if let Some(current) = app.prompts.front() {
        let panel = prompt::panel(current, width);
        let offset = panel
            .cursor
            .map_or_else(
                || panel.lines.len().saturating_sub(room),
                |(_, row)| usize::from(row).saturating_add(1).saturating_sub(room),
            )
            .saturating_sub(app.prompt_scroll);
        return Some(lines_sheet(
            panel.title.into(),
            panel.lines,
            offset,
            panel.cursor,
            copy::HINT_PROMPT,
        ));
    }
    if let Some(modal) = &app.modal {
        let (title, lines) = modal::content(app, width);
        return Some(lines_sheet(
            title.into(),
            lines,
            modal.offset,
            None,
            copy::MODAL_HINT,
        ));
    }
    if let Some(current) = &app.overlay {
        let (desired, count) = overlay::sheet_measure(app, current, area.width);
        let hint = if app.capturing() {
            copy::HINT_CAPTURE
        } else {
            match current.panel {
                Panel::Theme => copy::HINT_THEME,
                Panel::Notes => copy::HINT_NOTES,
                Panel::Note => copy::HINT_NOTE,
                Panel::Processes => copy::HINT_PROCESSES,
                Panel::ProcessOutput => copy::HINT_PROCESS_OUTPUT,
                Panel::McpDetail | Panel::SkillDetail | Panel::Activity => copy::HINT_DETAIL,
                Panel::Shortcuts => {
                    if current.tab == 0 {
                        copy::HINT_REFERENCE_COMMANDS
                    } else {
                        copy::HINT_REFERENCE_KEYS
                    }
                }
                _ => copy::HINT_PANEL,
            }
        };
        return Some(Sheet {
            title: current.title().into(),
            body: SheetBody::Overlay(current),
            height: if current.floating_reference {
                // The reference also needs its tabs and column headings.
                rows(desired + 2).min(floating_sheet_limit(app, area, rows(REFERENCE_ROWS + 4)))
            } else {
                sheet_height(area, desired)
            },
            count,
            hint,
            floating: current.floating_reference,
        });
    }
    if let Some(origin) = app.verification.as_deref() {
        let panel = prompt::verification(origin, width);
        return Some(lines_sheet(
            panel.title.into(),
            panel.lines,
            0,
            None,
            copy::HINT_VERIFY,
        ));
    }
    let attachments = app.attachments_open();
    if attachments || app.palette_open() {
        let cap = floating_sheet_limit(app, area, rows(PALETTE_ROWS + 2));
        let mut lines = picker_lines(app, width, usize::from(cap.saturating_sub(2)), attachments);
        if lines.is_empty() {
            lines.push(Line::from(copy::NOTHING_FOUND));
        }
        let (title, hint) = if attachments {
            (copy::ATTACHMENTS_TITLE, copy::HINT_ATTACHMENTS)
        } else {
            (copy::COMMANDS_TITLE, copy::HINT_PALETTE)
        };
        return Some(Sheet {
            title: title.into(),
            height: rows(lines.len() + 2).min(cap),
            body: SheetBody::Lines(lines, 0, None),
            count: None,
            hint,
            floating: true,
        });
    }
    None
}

fn draw_sheet(frame: &mut Frame<'_>, area: Rect, app: &App, sheet: Sheet<'_>) {
    if area.is_empty() {
        return;
    }
    let [head, body, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(u16::from(area.height >= 3)),
    ])
    .areas(area);
    let width = usize::from(area.width);
    frame.render_widget(
        Paragraph::new(sheet_rule(&sheet.title, sheet.count, width)),
        head,
    );
    match sheet.body {
        SheetBody::Overlay(overlay) => overlay::draw(frame, body, app, overlay),
        SheetBody::Lines(lines, offset, cursor) => {
            let offset = offset.min(lines.len().saturating_sub(usize::from(body.height)));
            frame.render_widget(Paragraph::new(lines).scroll((rows(offset), 0)), body);
            if let Some((column, row)) = cursor
                && usize::from(row) >= offset
                && usize::from(row) - offset < usize::from(body.height)
                && column < body.width
            {
                frame.set_cursor_position((body.x + column, body.y + row - rows(offset)));
            }
        }
    }
    frame.render_widget(Paragraph::new(hint_line(sheet.hint, width)), hint);
}

fn footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mode = mode_label(app.mode);
    let effort = crate::state::overlay::effort_label(app.effort);
    let model = model_line(app);
    let width = usize::from(area.width);
    let folder = app
        .folder
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(&app.folder);
    let context = app
        .context_percent
        .map_or_else(String::new, |value| format!("{value}% · "));
    let full = format!(" {context}{}  ?", app.folder);
    let short = format!(" {context}{folder}  ?");
    let right = if full.width() <= width / 2 {
        full
    } else {
        short
    };
    let right = text::truncate(&right, (width / 2).max(1));
    let left_room = width.saturating_sub(right.width() + 1);
    let mut left = format!(" ◆ {mode} · {model} · {effort}");
    if left.width() > left_room {
        left = format!(" ◆ {model}");
    }
    let running = app.running_processes();
    if running > 0 {
        let segment = format!(" · {}", crate::format::processes(running as u64));
        if left.width() + segment.width() <= left_room {
            left.push_str(&segment);
        }
    }
    if let Some(goal) = app.goal.filter(crate::state::GoalState::shown) {
        let value = if goal.active() {
            crate::format::goal_progress(
                goal.usage.rounds,
                goal.budget.rounds,
                app.goal_elapsed_ms(),
            )
        } else {
            crate::format::with_reason(copy::GOAL_PAUSED, goal.reason)
        };
        let segment = format!(" · {value}");
        if left.width() + segment.width() <= left_room {
            left.push_str(&segment);
        }
    }
    let left = text::truncate(&left, left_room);
    let gap = width.saturating_sub(left.width() + right.width());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, theme::muted()),
            Span::raw(" ".repeat(gap)),
            Span::styled(right, theme::muted()),
        ])),
        area,
    );
}

fn splash(frame: &mut Frame<'_>, area: Rect, message: &str, app: &App) {
    let mut lines = banner::lines(
        usize::from(area.width).saturating_sub(4),
        area.height.saturating_sub(2),
    );
    if !lines.is_empty() {
        lines.push(Line::default());
    }
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
    let width = area.width.min(ACCESS_WIDTH);
    let panel = prompt::panel(current, usize::from(width.saturating_sub(2)));
    let panel_height = rows(panel.lines.len() + 2).min(area.height);
    let spare = area.height - panel_height;
    let gap = if spare >= 3 { 2 } else { 0 };
    let art = banner::lines(usize::from(area.width).saturating_sub(4), spare - gap);
    let art_height = rows(art.len());
    let gap = if art_height > 0 { gap } else { 0 };
    let group = centered(area, area.width, art_height + gap + panel_height);
    frame.render_widget(
        Paragraph::new(art).alignment(ratatui::layout::Alignment::Center),
        Rect {
            height: art_height,
            ..group
        },
    );
    let target = Rect {
        x: area.x + (area.width - width) / 2,
        y: group.y + art_height + gap,
        width,
        height: panel_height,
    };
    let offset = panel
        .lines
        .len()
        .saturating_sub(usize::from(target.height.saturating_sub(2)))
        .saturating_sub(app.prompt_scroll);
    draw_sheet(
        frame,
        target,
        app,
        Sheet {
            title: panel.title.into(),
            body: SheetBody::Lines(panel.lines, offset, panel.cursor),
            count: None,
            hint: copy::HINT_PROMPT,
            height: target.height,
            floating: false,
        },
    );
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
                buffer[(x, y)].set_bg(theme::colors().selection_bg);
            }
        }
    }
}

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    theme::activate(app.active_theme());
    draw_screen(frame, app);
    highlight(frame, app);
}

fn chat_areas(app: &App, area: Rect, sheet: u16, floating: bool) -> [Rect; 6] {
    let panel_open = sheet > 0;
    let status = u16::from(!panel_open || floating);
    let bottom = if panel_open && !floating {
        0
    } else {
        input_height(app, area.width).min(area.height.saturating_sub(status))
    };
    let feedback = u16::from(
        (app.status.is_some() || !floating && idle_hint(app, sheet).is_some())
            && app.prompts.is_empty()
            && area.height > bottom + sheet + status,
    );
    let queue = if panel_open || floating {
        0
    } else {
        rows(queue_lines(app, usize::from(area.width)).len())
            .min(area.height.saturating_sub(bottom + feedback + 2))
    };
    Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(queue),
        Constraint::Length(feedback),
        Constraint::Length(sheet),
        Constraint::Length(bottom),
        Constraint::Length(status),
    ])
    .areas(area)
}

pub(crate) fn scroll_transcript(app: &App, delta: isize, at: Option<(u16, u16)>) -> usize {
    if app.phase != Phase::Chat {
        return app.scroll;
    }
    let area = Rect::new(0, 0, app.viewport.0, app.viewport.1);
    let sheet = active_sheet(app, area);
    let height = sheet.as_ref().map_or(0, |sheet| sheet.height);
    let floating = sheet.as_ref().is_some_and(|sheet| sheet.floating);
    let [body, ..] = chat_areas(app, area, height, floating);
    if body.is_empty()
        || at.is_some_and(|(x, y)| !body.contains(ratatui::layout::Position::new(x, y)))
    {
        return app.scroll;
    }
    let text = transcript_text_area(body);
    let max = transcript::lines(app, usize::from(text.width))
        .len()
        .saturating_sub(usize::from(text.height));
    app.scroll.min(max).saturating_add_signed(delta).min(max)
}

fn idle_hint(app: &App, sheet: u16) -> Option<&'static str> {
    if sheet > 0 {
        None
    } else if app.consent_pending {
        Some(copy::CONSENT_PENDING)
    } else if app.shell_mode() {
        Some(copy::HINT_SHELL)
    } else {
        None
    }
}

pub fn detail_scroll_bounds(app: &App, detail: &crate::state::overlay::Overlay) -> (usize, usize) {
    let area = Rect::new(0, 0, app.viewport.0, app.viewport.1);
    let body = Rect {
        height: sheet_limit(area).saturating_sub(2),
        ..area
    };
    overlay::detail_scroll_bounds(body, detail)
}

pub fn modal_max_scroll(app: &App) -> usize {
    let area = Rect::new(0, 0, app.viewport.0, app.viewport.1);
    let page = usize::from(sheet_limit(area).saturating_sub(2));
    modal::content(app, usize::from(area.width))
        .1
        .len()
        .saturating_sub(page)
}

pub fn prompt_scroll_limit(app: &App) -> usize {
    let area = Rect::new(0, 0, app.viewport.0, app.viewport.1);
    let (width, height) = if app.phase == Phase::Access {
        (area.width.min(ACCESS_WIDTH).saturating_sub(2), area.height)
    } else {
        (area.width, sheet_limit(area))
    };
    let room = usize::from(height.saturating_sub(2));
    app.prompts.front().map_or(0, |current| {
        let panel = prompt::panel(current, usize::from(width));
        panel.cursor.map_or_else(
            || panel.lines.len().saturating_sub(room),
            |(_, row)| usize::from(row).saturating_add(1).saturating_sub(room),
        )
    })
}

pub fn link_at(app: &App, area: Rect, x: u16, y: u16) -> Option<String> {
    if app.phase != Phase::Chat
        || app.overlay.is_some()
        || app.palette_open()
        || app.attachments_open()
        || app.modal.is_some()
        || app.show_welcome()
    {
        return None;
    }
    let height = active_sheet(app, area).map_or(0, |sheet| sheet.height);
    let [body, ..] = chat_areas(app, area, height, false);
    let body = transcript_text_area(body);
    let (lines, links) = transcript::layout(app, usize::from(body.width));
    let body = transcript_area(body, lines.len());
    if !body.contains(ratatui::layout::Position::new(x, y)) {
        return None;
    }
    let height = usize::from(body.height);
    let bottom = lines.len().saturating_sub(height);
    let offset = bottom.saturating_sub(app.scroll.min(bottom));
    let row = offset + usize::from(y - body.y);
    let width = lines.get(row).map_or(0, Line::width);
    if usize::from(x - body.x) >= width {
        return None;
    }
    links
        .into_iter()
        .find(|(index, _)| *index == row)
        .map(|(_, url)| url)
}

fn draw_screen(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    match app.phase {
        Phase::Connecting(message) => splash(frame, area, message, app),
        Phase::Access => access(frame, area, app),
        Phase::Chat => {
            let sheet = active_sheet(app, area);
            let height = sheet.as_ref().map_or(0, |sheet| sheet.height);
            let floating = sheet.as_ref().is_some_and(|sheet| sheet.floating);
            let [body, queued, feedback, sheet_area, composer, status] =
                chat_areas(app, area, height, floating);
            transcript_view(frame, body, app);
            queue_view(frame, queued, app);
            if let Some((tone, message)) = &app.status {
                let color = match tone {
                    Tone::Info => theme::colors().muted,
                    Tone::Success => theme::colors().ok,
                    Tone::Error => theme::colors().err,
                };
                let line = text::truncate(message, usize::from(feedback.width));
                frame.render_widget(
                    Paragraph::new(Span::styled(line, Style::new().fg(color))),
                    feedback,
                );
            } else if let Some(hint) = idle_hint(app, height) {
                frame.render_widget(
                    Paragraph::new(hint_line(hint, usize::from(feedback.width))),
                    feedback,
                );
            }
            if let Some(sheet) = sheet {
                draw_sheet(frame, sheet_area, app, sheet);
            }
            if !composer.is_empty() {
                input_view(frame, composer, app);
            }
            if !status.is_empty() {
                footer(frame, status, app);
            }
        }
    }
}

#[cfg(test)]
mod tests;
