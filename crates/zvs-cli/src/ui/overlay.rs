use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;
use zvs_contract::CliTheme;

use super::text::{truncate, wrap};
use super::theme;
use super::transcript::spinner;
use crate::copy;
use crate::state::App;
use crate::state::overlay::{Detail, Overlay, Panel, Row, Tone, process_runtime, process_state};

const GAUGE_WIDTH: usize = 24;
const SWATCH_TITLE_WIDTH: usize = 12;
const SEARCH_FIELD_HEIGHT: u16 = 3;
const SEARCH_MIN_WIDTH: u16 = 6;
const COLUMN_GAP: usize = 2;
const ROW_PREFIX: &str = "     ";

fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Plain => theme::colors().ink,
        Tone::Accent => theme::colors().accent,
        Tone::Good => theme::colors().ok,
        Tone::Warn => theme::colors().warn,
        Tone::Bad => theme::colors().err,
        Tone::Faint => theme::colors().faint,
    }
}

fn tabs_line(names: &[String], active: usize, width: usize) -> Line<'static> {
    let tab = |index, name: &String| {
        Span::styled(
            format!(" {} ", truncate(name, width.saturating_sub(3))),
            if index == active {
                theme::selected()
            } else {
                theme::muted()
            },
        )
    };
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (index, name) in names.iter().enumerate() {
        let span = tab(index, name);
        if used + span.width() > width {
            if index <= active
                && let Some(name) = names.get(active)
            {
                return Line::from(vec![Span::raw(" "), tab(active, name)]);
            }
            break;
        }
        used += span.width() + 1;
        spans.extend([span, Span::raw(" ")]);
    }
    Line::from(spans)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn gauge(value: f64, width: usize, color: Color) -> Vec<Span<'static>> {
    let filled = (value.clamp(0.0, 1.0) * width as f64).round() as usize;
    vec![
        Span::styled("━".repeat(filled), Style::new().fg(color)),
        Span::styled("─".repeat(width - filled), theme::faint()),
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

fn table_header(column: usize, commands: bool) -> Line<'static> {
    let keys = if commands {
        copy::COMMAND
    } else {
        copy::SHORTCUT_COLUMN_KEYS
    };
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
    let base = Style::new();
    let marker = if selected { " › " } else { "   " };
    let keys_style = match row.tone {
        Tone::Faint => theme::faint(),
        Tone::Warn => Style::new()
            .fg(theme::colors().warn)
            .add_modifier(Modifier::BOLD),
        _ => theme::strong(),
    };
    let keys = truncate(&row.meta, column.saturating_sub(1));
    let pad = column.saturating_sub(keys.width());
    let mut spans = vec![
        Span::styled(
            marker,
            base.fg(theme::colors().accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{} ", row.icon), base.fg(color)),
        Span::styled(keys, base.patch(keys_style)),
        Span::styled(" ".repeat(pad), base),
    ];
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let title = truncate(&row.title, width.saturating_sub(used + 1));
    let title_column = width.saturating_sub(used + 1);
    let title_style = if row.tone == Tone::Accent {
        Style::new().fg(theme::colors().accent)
    } else {
        theme::text()
    };
    spans.push(Span::styled(
        format!(
            "{}{}",
            title,
            " ".repeat(title_column.saturating_sub(title.width()))
        ),
        base.patch(title_style),
    ));
    Line::from(spans)
}

fn directory_labels(panel: Panel) -> [&'static str; 5] {
    if panel == Panel::Skills {
        [
            copy::LIST_NAME,
            copy::LIST_STATUS,
            copy::LIST_SOURCE,
            copy::LIST_FILES,
            copy::LIST_DESCRIPTION,
        ]
    } else {
        [
            copy::LIST_NAME,
            copy::LIST_STATUS,
            copy::LIST_TRANSPORT,
            copy::MCP_TOOLS,
            copy::MCP_NAMESPACE,
        ]
    }
}

fn directory_columns(width: usize, rows: &[Row], labels: [&str; 5]) -> [usize; 5] {
    let room = width.saturating_sub(ROW_PREFIX.len());
    if room < 18 {
        return [room, 0, 0, 0, 0];
    }
    let content_width = |index: usize| {
        rows.iter()
            .filter_map(|row| row.columns.as_ref())
            .map(|columns| columns[index].width())
            .fold(labels[index + 1].width(), usize::max)
    };
    let status = content_width(0).min(room / 3).min(18);
    let [source, count] = if width >= 76 {
        [content_width(1).min(18), content_width(2).min(15)]
    } else {
        [0, 0]
    };
    let gaps = if source == 0 { 1 } else { 3 };
    let remaining = room - status - source - count - gaps * COLUMN_GAP;
    if width >= 104 {
        let name = rows
            .iter()
            .map(|row| row.title.width())
            .fold(labels[0].width(), usize::max)
            .min(room / 3);
        [name, status, source, count, remaining - name - COLUMN_GAP]
    } else {
        [remaining, status, source, count, 0]
    }
}

fn directory_cells(
    columns: [usize; 5],
    values: [&str; 5],
    styles: [Style; 5],
) -> impl Iterator<Item = Span<'static>> {
    columns
        .into_iter()
        .zip(values)
        .zip(styles)
        .enumerate()
        .filter(|(_, ((width, _), _))| *width > 0)
        .map(move |(index, ((width, value), style))| {
            let value = truncate(value.trim().lines().next().unwrap_or_default(), width);
            let gap =
                usize::from(columns.get(index + 1).is_some_and(|next| *next > 0)) * COLUMN_GAP;
            let padding = width - value.width() + gap;
            Span::styled(format!("{value}{}", " ".repeat(padding)), style)
        })
}

fn directory_header(labels: [&'static str; 5], columns: [usize; 5]) -> Line<'static> {
    Line::from_iter(
        std::iter::once(Span::raw(ROW_PREFIX)).chain(directory_cells(
            columns,
            labels,
            [theme::eyebrow(); 5],
        )),
    )
}

fn directory_row(
    row: &Row,
    values: &[String; 4],
    columns: [usize; 5],
    selected: bool,
) -> Line<'static> {
    let marker = if selected { " › " } else { "   " };
    let prefix = [
        Span::styled(marker, theme::accent()),
        Span::styled(
            format!("{} ", row.icon),
            Style::new().fg(tone_color(row.tone)),
        ),
    ];
    let [status, kind, count, summary] = values.each_ref().map(String::as_str);
    let name = if selected {
        theme::accent()
    } else {
        theme::text()
    };
    Line::from_iter(prefix.into_iter().chain(directory_cells(
        columns,
        [&row.title, status, kind, count, summary],
        [
            name,
            Style::new().fg(tone_color(row.tone)),
            theme::muted(),
            theme::muted(),
            theme::muted(),
        ],
    )))
}

fn row_lines(
    row: &Row,
    selected: bool,
    expanded: bool,
    width: usize,
    swatch: Option<CliTheme>,
) -> Vec<Line<'static>> {
    let color = swatch.map_or_else(
        || tone_color(row.tone),
        |theme| theme::colors_of(theme).accent,
    );
    let base = Style::new();
    let marker = if selected { " › " } else { "   " };
    let title_style = if selected {
        theme::accent()
    } else if row.tone == Tone::Faint {
        theme::faint()
    } else if row.current {
        Style::new()
            .fg(theme::colors().ink)
            .add_modifier(Modifier::BOLD)
    } else {
        theme::text()
    };
    let mut spans = vec![
        Span::styled(
            marker,
            base.fg(theme::colors().accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{} ", row.icon), base.fg(color)),
    ];
    let room = width.saturating_sub(4 + row.icon.width());
    let meta_width = row.meta.width().min(room / 2);
    let meta = if meta_width == 0 {
        String::new()
    } else {
        truncate(&row.meta, meta_width)
    };
    let name_room = room.saturating_sub(meta_width + usize::from(meta_width > 0) * 2);
    let gauge_width = if row.gauge.is_some() {
        (name_room / 3).min(GAUGE_WIDTH)
    } else {
        0
    };
    let show_swatch = swatch.is_some() && name_room >= 18;
    let title_width = if show_swatch {
        name_room.saturating_sub(12).min(SWATCH_TITLE_WIDTH)
    } else if gauge_width > 0 {
        name_room.saturating_sub(gauge_width + 2)
    } else {
        name_room
    };
    let title = if title_width == 0 {
        String::new()
    } else {
        truncate(&row.title, title_width)
    };
    let gap = title_width.saturating_sub(title.width()) + 2;
    spans.push(Span::styled(title, base.patch(title_style)));
    if let Some(theme) = swatch.filter(|_| show_swatch) {
        spans.push(Span::styled(" ".repeat(gap), base));
        let colors = theme::colors_of(theme);
        for tint in [
            colors.accent,
            colors.accent_light,
            colors.ok,
            colors.warn,
            colors.err,
        ] {
            spans.push(Span::styled("██", base.fg(tint)));
        }
    }
    if let Some(value) = row.gauge.filter(|_| gauge_width > 0) {
        spans.push(Span::styled(" ".repeat(gap), base));
        spans.extend(gauge(value, gauge_width, color).into_iter().map(|span| {
            let style = base.patch(span.style);
            span.style(style)
        }));
    }
    let used: usize = spans.iter().map(Span::width).sum();
    let meta_width = meta.width();
    if meta_width > 0 {
        let space = width.saturating_sub(used + meta_width);
        spans.push(Span::styled(" ".repeat(space), base));
        spans.push(Span::styled(meta, base.fg(theme::colors().muted)));
    }
    if selected && meta_width == 0 {
        spans.push(Span::styled(" ".repeat(width.saturating_sub(used)), base));
    }
    let mut lines = vec![Line::from(spans)];
    if expanded {
        for detail in &row.detail {
            for piece in wrap(detail, width.saturating_sub(6).max(1)) {
                lines.push(Line::from(vec![
                    Span::raw("      "),
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
    } else if overlay.panel == Panel::Mcp && overlay.filter.is_empty() && overlay.tab == 0 {
        copy::MCP_EMPTY
    } else if overlay.panel == Panel::Processes && overlay.filter.is_empty() {
        copy::PROCESSES_EMPTY
    } else if overlay.panel == Panel::Notes && overlay.filter.is_empty() {
        copy::NOTES_EMPTY
    } else {
        copy::NOTHING_FOUND
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn field(label: &str, column: usize, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(format!("{label}:"), theme::strong()),
        Span::raw(" ".repeat(column.saturating_sub(label.width() + 1))),
    ];
    spans.extend(value);
    Line::from(spans)
}

pub fn process_lines(app: &App, overlay: &Overlay, width: usize, now: i64) -> Vec<Line<'static>> {
    let Some(output) = &overlay.output else {
        return Vec::new();
    };
    let shell = app.processes.iter().find(|shell| shell.id == output.id);
    let labels = [
        copy::PROCESS_STATUS,
        copy::PROCESS_RUNTIME,
        copy::PROCESS_COMMAND,
        copy::PROCESS_FOLDER,
        copy::PROCESS_EXIT_CODE,
    ];
    let column = labels.iter().map(|label| label.width()).max().unwrap_or(0) + 3;
    let mut lines = vec![
        Line::from(Span::styled(
            format!("  {}", copy::PROCESS_OUTPUT_TITLE),
            theme::accent().add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];
    if let Some(shell) = shell {
        let (state, tone) = process_state(shell.status, shell.exit_code);
        lines.push(field(
            copy::PROCESS_STATUS,
            column,
            vec![Span::styled(state, Style::new().fg(tone_color(tone)))],
        ));
        lines.push(field(
            copy::PROCESS_RUNTIME,
            column,
            vec![Span::styled(
                crate::format::runtime(process_runtime(shell, now)),
                theme::text(),
            )],
        ));
    }
    let room = width.saturating_sub(column + 4).max(8);
    for (index, piece) in wrap(&output.command, room).into_iter().enumerate() {
        if index == 0 {
            lines.push(field(
                copy::PROCESS_COMMAND,
                column,
                vec![Span::styled(piece, theme::text())],
            ));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(column + 2)),
                Span::styled(piece, theme::text()),
            ]));
        }
    }
    if let Some(shell) = shell {
        lines.push(field(
            copy::PROCESS_FOLDER,
            column,
            vec![Span::styled(truncate(&shell.cwd, room), theme::muted())],
        ));
        if let Some(code) = shell.exit_code.filter(|_| !shell.status.running()) {
            lines.push(field(
                copy::PROCESS_EXIT_CODE,
                column,
                vec![Span::styled(code.to_string(), theme::text())],
            ));
        }
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!("  {}:", copy::PROCESS_OUTPUT_HEADING),
        theme::strong(),
    )));
    lines
}

fn draw_process(frame: &mut Frame<'_>, area: Rect, app: &App, overlay: &Overlay) {
    let width = usize::from(area.width);
    let head = process_lines(app, overlay, width, now_ms());
    let [top, output] = Layout::vertical([
        Constraint::Length(
            u16::try_from(head.len())
                .unwrap_or(u16::MAX)
                .min(area.height.saturating_sub(1)),
        ),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(head), top);
    let view = app.view();
    let rows = overlay.rows(&view);
    if overlay.loading {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {} ", spinner(app.frame)), theme::accent()),
                Span::styled(copy::LOADING, theme::muted()),
            ])),
            output,
        );
    } else if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {}", copy::PROCESS_OUTPUT_EMPTY),
                theme::faint(),
            ))),
            output,
        );
    } else {
        let lines: Vec<Line<'static>> = rows
            .iter()
            .map(|row| {
                let style = if row.tone == Tone::Faint {
                    theme::faint()
                } else {
                    theme::text()
                };
                Line::from(Span::styled(
                    format!("  {}", truncate(&row.title, width.saturating_sub(3))),
                    style,
                ))
            })
            .collect();
        let height = usize::from(output.height);
        let selected = overlay.selected.min(rows.len() - 1);
        let offset = (selected + 1).saturating_sub(height);
        frame.render_widget(
            Paragraph::new(lines).scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
            output,
        );
    }
}

fn detail_lines(overlay: &Overlay, width: usize) -> Vec<Line<'static>> {
    let content = match &overlay.detail {
        Some(Detail::Server(_, Some(Ok(detail)))) => {
            return super::transcript::server_card(detail, true, None, width);
        }
        Some(Detail::Skill(_, Some(Ok(detail)))) => {
            let skill = &detail.skill;
            let mut lines = vec![Line::from(Span::styled(
                format!("  {}", skill.name),
                theme::strong(),
            ))];
            for (label, value) in [
                (copy::SKILL_AUTHOR, skill.author.as_deref()),
                (copy::SKILL_VERSION, skill.version.as_deref()),
            ] {
                if let Some(value) = value {
                    lines.extend(detail_text(
                        &format!("{label} · {value}"),
                        width,
                        theme::text(),
                    ));
                }
            }
            if !skill.description.is_empty() {
                lines.push(Line::default());
                lines.extend(detail_text(&skill.description, width, theme::muted()));
            }
            if !detail.instructions.trim().is_empty() {
                lines.push(Line::default());
                lines.push(Line::from(Span::styled(
                    format!("  {}", copy::SKILL_INSTRUCTIONS),
                    theme::eyebrow(),
                )));
                lines.extend(super::text::markdown(&detail.instructions, width, "  "));
            }
            return lines;
        }
        Some(Detail::Server(_, Some(Err(error))) | Detail::Skill(_, Some(Err(error)))) => {
            return detail_text(error, width, Style::new().fg(theme::colors().err));
        }
        _ => Span::styled(format!("  {}", copy::LOADING), theme::muted()),
    };
    vec![Line::from(content)]
}

fn detail_text(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    wrap(text, width.saturating_sub(2))
        .into_iter()
        .map(|line| Line::from(Span::styled(format!("  {line}"), style)))
        .collect()
}

pub(super) fn detail_scroll_bounds(area: Rect, overlay: &Overlay) -> (usize, usize) {
    let area = content_area(area, overlay.panel);
    let page = usize::from(area.height);
    let total = detail_lines(overlay, usize::from(area.width)).len();
    (total.saturating_sub(page), page.max(1))
}

fn content_area(area: Rect, panel: Panel) -> Rect {
    if matches!(panel, Panel::ProcessOutput | Panel::Note) {
        return area;
    }
    area.inner(Margin {
        horizontal: u16::from(area.width >= 8) * 2,
        vertical: 0,
    })
}

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App, overlay: &Overlay) {
    if area.is_empty() {
        return;
    }
    let area = content_area(area, overlay.panel);
    if overlay.panel == Panel::ProcessOutput {
        draw_process(frame, area, app, overlay);
        return;
    }
    if matches!(overlay.panel, Panel::McpDetail | Panel::SkillDetail) {
        let lines = detail_lines(overlay, usize::from(area.width));
        let max_scroll = lines.len().saturating_sub(usize::from(area.height));
        let offset = overlay.detail_scroll.min(max_scroll);
        frame.render_widget(Paragraph::new(lines).scroll((super::rows(offset), 0)), area);
        return;
    }
    let view = app.view();
    let rows = overlay.rows(&view);
    let tabs = overlay.tabs(&view);
    let tabs_line = tabs_line(&tabs, overlay.tab, usize::from(area.width));
    let tab_height = u16::from(!tabs.is_empty() && area.height >= 3);
    let search_room = area.height.saturating_sub(tab_height + 1);
    let search_height = if overlay.floating_reference {
        0
    } else if search_room >= SEARCH_FIELD_HEIGHT + 3 && area.width >= SEARCH_MIN_WIDTH {
        SEARCH_FIELD_HEIGHT
    } else {
        search_room.min(1)
    };
    let [tabs_area, search, list] = Layout::vertical([
        Constraint::Length(tab_height),
        Constraint::Length(search_height),
        Constraint::Fill(1),
    ])
    .areas(area);

    frame.render_widget(Paragraph::new(tabs_line), tabs_area);
    draw_search(frame, search, app, overlay);
    draw_list(frame, list, app, overlay, &rows);
}

pub(super) fn sheet_measure(app: &App, overlay: &Overlay, width: u16) -> (usize, Option<usize>) {
    let view = app.view();
    let width = usize::from(content_area(Rect::new(0, 0, width, 1), overlay.panel).width);
    if matches!(overlay.panel, Panel::McpDetail | Panel::SkillDetail) {
        return (detail_lines(overlay, width).len(), None);
    }
    let rows = overlay.rows(&view);
    if overlay.panel == Panel::ProcessOutput {
        return (
            process_lines(app, overlay, width, now_ms()).len() + rows.len().max(1),
            None,
        );
    }
    let tabs = overlay.tabs(&view);
    let search_height = if overlay.floating_reference {
        0
    } else if width >= usize::from(SEARCH_MIN_WIDTH) {
        SEARCH_FIELD_HEIGHT
    } else {
        1
    };
    let controls = usize::from(search_height) + usize::from(!tabs.is_empty());
    let header = usize::from(
        !rows.is_empty() && matches!(overlay.panel, Panel::Shortcuts | Panel::Skills | Panel::Mcp),
    );
    let expanded: usize = rows
        .iter()
        .filter(|row| overlay.is_expanded(&row.key))
        .flat_map(|row| &row.detail)
        .map(|line| wrap(line, width.saturating_sub(6)).len())
        .sum();
    (
        controls + header + rows.len().max(1) + expanded,
        Some(rows.len()),
    )
}

fn draw_search(frame: &mut Frame<'_>, search: Rect, app: &App, overlay: &Overlay) {
    if search.is_empty() {
        return;
    }
    let body = if search.height == SEARCH_FIELD_HEIGHT {
        let border = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme::muted());
        let body = border.inner(search);
        frame.render_widget(border, search);
        body
    } else {
        search
    };
    let [icon, value] =
        Layout::horizontal([Constraint::Length(3), Constraint::Fill(1)]).areas(body);
    let filter = overlay.filter.text();
    let (_, column) = overlay.filter.lines_before_cursor();
    let available = usize::from(value.width);
    let offset = column.saturating_sub(available.saturating_sub(1));
    frame.render_widget(Paragraph::new(Span::styled(" ⌕ ", theme::accent())), icon);
    frame.render_widget(
        Paragraph::new(if filter.is_empty() {
            Span::styled(copy::SEARCH_PLACEHOLDER, theme::faint())
        } else {
            Span::styled(filter, theme::text())
        })
        .scroll((0, super::rows(offset))),
        value,
    );
    if !value.is_empty() && app.modal.is_none() && !app.capturing() {
        frame.set_cursor_position((value.x + super::rows(column - offset), value.y));
    }
}

fn draw_list(frame: &mut Frame<'_>, list: Rect, app: &App, overlay: &Overlay, rows: &[Row]) {
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
    let table = (overlay.panel == Panel::Shortcuts).then(|| key_column(rows).min(width / 2));
    let labels = directory_labels(overlay.panel);
    let directory = matches!(overlay.panel, Panel::Skills | Panel::Mcp)
        .then(|| directory_columns(width, rows, labels));
    let [header, list] = Layout::vertical([
        Constraint::Length(u16::from(
            (table.is_some() || directory.is_some()) && list.height > 1,
        )),
        Constraint::Fill(1),
    ])
    .areas(list);
    if let Some(column) = table {
        frame.render_widget(
            Paragraph::new(table_header(column, overlay.tab == 0)),
            header,
        );
    } else if let Some(columns) = directory {
        frame.render_widget(Paragraph::new(directory_header(labels, columns)), header);
    }
    for (index, row) in rows.iter().enumerate() {
        let start = lines.len();
        match (table, directory.zip(row.columns.as_ref())) {
            (Some(column), _) => lines.push(table_row(row, index == selected, column, width)),
            (_, Some((columns, values))) => {
                lines.push(directory_row(row, values, columns, index == selected))
            }
            _ => lines.extend(row_lines(
                row,
                index == selected,
                overlay.is_expanded(&row.key),
                width,
                (overlay.panel == Panel::Theme)
                    .then(|| CliTheme::parse(&row.key))
                    .flatten(),
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
