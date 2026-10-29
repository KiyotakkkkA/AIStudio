use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;
use zvs_contract::{ChatToolCallDto, ChatToolCallStatus};

use super::text::{markdown, truncate, wrap};
use super::theme;
use crate::copy;
use crate::state::App;
use crate::state::transcript::{Entry, Segment, ShellEnd, ShellRun, Tone};

pub const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
pub const TICKS_PER_SECOND: u64 = 10;
const RESULT_LINES: usize = 12;
const COLLAPSED_ARGUMENT: usize = 80;

pub fn spinner(frame: u64) -> &'static str {
    SPINNER[usize::try_from(frame).unwrap_or(0) % SPINNER.len()]
}

fn status_look(status: ChatToolCallStatus, frame: u64) -> (&'static str, Color, &'static str) {
    match status {
        ChatToolCallStatus::Running => (spinner(frame), theme::ACCENT, copy::TOOL_RUNNING),
        ChatToolCallStatus::AwaitingApproval => ("?", theme::WARN, copy::TOOL_AWAITING_APPROVAL),
        ChatToolCallStatus::AwaitingAnswer => ("?", theme::WARN, copy::TOOL_AWAITING_ANSWER),
        ChatToolCallStatus::Succeeded => ("✓", theme::OK, copy::TOOL_SUCCEEDED),
        ChatToolCallStatus::Failed => ("✗", theme::ERR, copy::TOOL_FAILED),
        ChatToolCallStatus::Refused => ("⊘", theme::ERR, copy::TOOL_REFUSED),
        ChatToolCallStatus::Denied => ("⊘", theme::ERR, copy::TOOL_DENIED),
        ChatToolCallStatus::Cancelled => ("○", theme::MUTED, copy::TOOL_CANCELLED),
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.replace('\n', " "),
        Value::Null => "null".to_owned(),
        other => other.to_string(),
    }
}

pub fn arguments(arguments: &Value) -> Vec<(String, String)> {
    match arguments {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| (key.clone(), scalar(value)))
            .collect(),
        Value::Null => Vec::new(),
        other => vec![(String::new(), scalar(other))],
    }
}

fn duration(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1} с", ms as f64 / 1000.0).replace('.', ",")
    } else {
        format!("{ms} мс")
    }
}

fn indent() -> Span<'static> {
    Span::raw("    ")
}

fn tool(call: &ChatToolCallDto, expanded: bool, frame: u64, width: usize) -> Vec<Line<'static>> {
    let (icon, color, label) = status_look(call.status, frame);
    let mut lines = Vec::new();
    let chip = Style::new().bg(theme::TOOL_BG);
    let mut header = vec![
        Span::raw("  "),
        Span::styled(
            format!(" {icon} "),
            chip.fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", call.name),
            chip.fg(theme::INK).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {label}"), Style::new().fg(color)),
    ];
    if call.duration_ms > 0 {
        header.push(Span::styled(
            format!(" · {}", duration(call.duration_ms)),
            theme::faint(),
        ));
    }
    lines.push(Line::from(header));

    let room = width.saturating_sub(8).max(10);
    let pairs = arguments(&call.arguments);
    if expanded {
        if !pairs.is_empty() {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(copy::ARGUMENTS, theme::eyebrow()),
            ]));
        }
        for (key, value) in &pairs {
            let text = if key.is_empty() {
                value.clone()
            } else {
                format!("{key}: {value}")
            };
            for piece in wrap(&text, room) {
                lines.push(Line::from(vec![
                    indent(),
                    Span::styled(piece, theme::muted()),
                ]));
            }
        }
    } else if !pairs.is_empty() {
        let summary = pairs
            .iter()
            .map(|(key, value)| {
                if key.is_empty() {
                    value.clone()
                } else {
                    format!("{key}: {value}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(Line::from(vec![
            indent(),
            Span::styled(
                truncate(&summary, room.min(COLLAPSED_ARGUMENT)),
                theme::muted(),
            ),
        ]));
    }

    let output = call.error.as_ref().or(call.result.as_ref());
    if let Some(output) = output.filter(|text| !text.trim().is_empty()) {
        let style = if call.error.is_some() {
            Style::new().fg(theme::ERR)
        } else {
            theme::text()
        };
        let wrapped: Vec<String> = output
            .trim_end()
            .split('\n')
            .flat_map(|line| wrap(line, room))
            .collect();
        let shown = if expanded { RESULT_LINES } else { 1 };
        if expanded {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(copy::RESULT, theme::eyebrow()),
            ]));
        }
        for (index, piece) in wrapped.iter().take(shown).enumerate() {
            let lead = if index == 0 && !expanded { "⎿ " } else { "" };
            lines.push(Line::from(vec![
                indent(),
                Span::styled(lead, theme::faint()),
                Span::styled(piece.clone(), style),
            ]));
        }
        let hidden = wrapped.len().saturating_sub(shown);
        if hidden > 0 {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(format!("… {} {hidden}", copy::MORE_LINES), theme::faint()),
            ]));
        }
        if call.truncated && expanded {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(copy::TRUNCATED, theme::faint()),
            ]));
        }
    }
    lines
}

const SHELL_TAIL: usize = 12;
const SHELL_EXPANDED: usize = 400;

fn shell_block(run: &ShellRun, expanded: bool, frame: u64, width: usize) -> Vec<Line<'static>> {
    let (icon, color, label) = match run.end {
        None => (
            spinner(frame).to_owned(),
            theme::ACCENT,
            copy::SHELL_RUNNING.to_owned(),
        ),
        Some(ShellEnd::Exited(0)) => ("✓".to_owned(), theme::OK, copy::SHELL_DONE.to_owned()),
        Some(ShellEnd::Exited(code)) => (
            "✗".to_owned(),
            theme::ERR,
            format!("{} {code}", copy::SHELL_EXIT),
        ),
        Some(ShellEnd::Stopped) => ("○".to_owned(), theme::MUTED, copy::SHELL_STOPPED.to_owned()),
        Some(ShellEnd::Failed) => ("✗".to_owned(), theme::ERR, copy::SHELL_FAILED.to_owned()),
    };
    let chip = Style::new().bg(theme::SHELL_BG);
    let room = width.saturating_sub(8).max(10);
    let mut lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(" $ ", chip.fg(theme::WARN).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("{} ", truncate(&run.command, room)),
            chip.fg(theme::INK).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {icon} {label}"), Style::new().fg(color)),
    ])];
    let wrapped: Vec<String> = run
        .output
        .iter()
        .flat_map(|line| wrap(line, room))
        .collect();
    let shown = if expanded { SHELL_EXPANDED } else { SHELL_TAIL };
    let hidden = wrapped.len().saturating_sub(shown);
    if hidden > 0 {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(format!("… {} {hidden}", copy::MORE_LINES), theme::faint()),
        ]));
    }
    for piece in wrapped.into_iter().skip(hidden) {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(piece, theme::muted()),
        ]));
    }
    lines
}

fn user(text: &str, width: usize) -> Vec<Line<'static>> {
    let room = width.saturating_sub(6).max(8);
    let style = Style::new().fg(theme::INK).bg(theme::USER_BG);
    let edge = Span::styled("  ", style);
    let mut lines = vec![Line::from(vec![
        edge.clone(),
        Span::styled(
            copy::YOU,
            Style::new()
                .fg(theme::ACCENT)
                .bg(theme::USER_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ".repeat(room.saturating_sub(copy::YOU.width()) + 2),
            style,
        ),
    ])];
    for piece in wrap(text, room) {
        let pad = room.saturating_sub(piece.width()) + 2;
        lines.push(Line::from(vec![
            edge.clone(),
            Span::styled(piece, style),
            Span::styled(" ".repeat(pad), style),
        ]));
    }
    lines
}

fn reasoning(text: &str, expanded: bool, width: usize) -> Vec<Line<'static>> {
    let count = text.trim().lines().count();
    let mut lines = vec![Line::from(vec![
        Span::styled("  ✻ ", theme::faint()),
        Span::styled(
            copy::REASONING,
            theme::faint().add_modifier(Modifier::ITALIC),
        ),
        Span::styled(
            if expanded {
                String::new()
            } else {
                format!(" · {count} · Ctrl+O")
            },
            theme::faint(),
        ),
    ])];
    if expanded {
        for piece in text
            .trim()
            .split('\n')
            .flat_map(|line| wrap(line, width.saturating_sub(6).max(8)))
        {
            lines.push(Line::from(vec![
                Span::styled("    ", theme::faint()),
                Span::styled(piece, theme::faint().add_modifier(Modifier::ITALIC)),
            ]));
        }
    }
    lines
}

fn assistant(text: &str, thought: &str, expanded: bool, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !thought.trim().is_empty() {
        lines.extend(reasoning(thought, expanded, width));
    }
    let mut body = markdown(text.trim_end(), width, "    ");
    if let Some(first) = body.first_mut()
        && let Some(lead) = first.spans.first_mut()
    {
        *lead = Span::styled("  ● ", theme::accent());
    }
    lines.extend(body);
    lines
}

fn notice(tone: Tone, text: &str, width: usize) -> Vec<Line<'static>> {
    let (icon, color) = match tone {
        Tone::Info => ("ⓘ", theme::MUTED),
        Tone::Success => ("✓", theme::OK),
        Tone::Error => ("✗", theme::ERR),
    };
    wrap(text, width.saturating_sub(6).max(8))
        .into_iter()
        .enumerate()
        .map(|(index, piece)| {
            let lead = if index == 0 {
                format!("  {icon} ")
            } else {
                "    ".to_owned()
            };
            Line::from(vec![
                Span::styled(lead, Style::new().fg(color)),
                Span::styled(piece, Style::new().fg(color)),
            ])
        })
        .collect()
}

pub fn lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let expanded = app.expanded;
    for entry in &app.transcript.entries {
        lines.push(Line::default());
        lines.extend(match entry {
            Entry::User(text) => user(text, width),
            Entry::Assistant { text, reasoning } => assistant(text, reasoning, expanded, width),
            Entry::Tool(call) => tool(call, expanded, app.frame, width),
            Entry::Notice { tone, text } => notice(*tone, text, width),
            Entry::Shell(run) => shell_block(run, expanded, app.frame, width),
        });
    }
    if let Some(run) = &app.shell {
        lines.push(Line::default());
        lines.extend(shell_block(run, expanded, app.frame, width));
    }
    if let Some(live) = &app.transcript.live {
        let last = live.segments.len().saturating_sub(1);
        for (index, segment) in live.segments.iter().enumerate() {
            lines.push(Line::default());
            match segment {
                Segment::Tool(call) => lines.extend(tool(call, expanded, app.frame, width)),
                Segment::Text { text, reasoning } => {
                    let thinking = index == last && text.is_empty();
                    lines.extend(assistant(text, reasoning, expanded || thinking, width));
                }
            }
        }
        let seconds = app.frame.saturating_sub(app.started) / TICKS_PER_SECOND;
        let label = if app.cancelling {
            copy::CANCELLING
        } else {
            copy::WORKING
        };
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", spinner(app.frame)), theme::accent()),
            Span::styled(label, theme::accent().add_modifier(Modifier::ITALIC)),
            Span::styled(format!(" · {seconds} с · Esc"), theme::faint()),
        ]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn plain(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    fn call(result: &str) -> ChatToolCallDto {
        serde_json::from_value(json!({
            "id": "c1", "key": "files.read", "name": "read_file", "round": 0,
            "arguments": { "path": "src/main.rs", "lines": 40 },
            "status": "succeeded", "result": result, "durationMs": 1250
        }))
        .expect("call")
    }

    #[test]
    fn a_collapsed_tool_block_shows_one_line_of_arguments_and_result() {
        let text = plain(&tool(&call("one\ntwo\nthree"), false, 0, 80));
        assert_eq!(text[0], "   ✓ read_file   готово · 1,2 с");
        assert_eq!(text[1], "    path: src/main.rs  lines: 40");
        assert_eq!(text[2], "    ⎿ one");
        assert_eq!(text[3], format!("    … {} 2", copy::MORE_LINES));
    }

    #[test]
    fn an_expanded_tool_block_lists_every_argument_and_result_line() {
        let text = plain(&tool(&call("one\ntwo"), true, 0, 80));
        assert!(text.contains(&format!("    {}", copy::ARGUMENTS)));
        assert!(text.contains(&"    lines: 40".to_owned()));
        assert!(text.contains(&"    two".to_owned()));
    }

    #[test]
    fn arguments_flatten_to_key_value_pairs() {
        assert_eq!(
            arguments(&json!({ "a": "x\ny", "b": [1] })),
            [
                ("a".to_owned(), "x y".to_owned()),
                ("b".to_owned(), "[1]".to_owned())
            ]
        );
        assert!(arguments(&Value::Null).is_empty());
    }
}
