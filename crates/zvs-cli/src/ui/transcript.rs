use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;
use zvs_contract::{
    ChatSourceDto, ChatToolCallDto, ChatToolCallStatus, McpCapabilityDto, McpCapabilityKind,
    McpOAuthState, McpServerDetailDto, McpServerStatus, McpStartup, McpTransport, McpValue,
    NoteSourceDto, WebSourceDto,
};

use super::text::{markdown, truncate, wrap};
use super::theme;
use crate::copy;
use crate::format;
use crate::state::App;
use crate::state::shortcuts::Shortcut;
use crate::state::transcript::{AGENT_DELEGATE, Entry, Segment, ShellEnd, ShellRun, Tone};

pub const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
pub const TICKS_PER_SECOND: u64 = 10;
const RESULT_LINES: usize = 12;
const SOURCES_SHOWN: usize = 5;

pub fn spinner(frame: u64) -> &'static str {
    SPINNER[usize::try_from(frame).unwrap_or(0) % SPINNER.len()]
}

pub(super) fn status_look(
    status: ChatToolCallStatus,
    frame: u64,
) -> (&'static str, Color, &'static str) {
    match status {
        ChatToolCallStatus::Running => (spinner(frame), theme::colors().accent, copy::TOOL_RUNNING),
        ChatToolCallStatus::AwaitingApproval => {
            ("?", theme::colors().warn, copy::TOOL_AWAITING_APPROVAL)
        }
        ChatToolCallStatus::AwaitingAnswer => {
            ("?", theme::colors().warn, copy::TOOL_AWAITING_ANSWER)
        }
        ChatToolCallStatus::Succeeded => ("✓", theme::colors().ok, copy::TOOL_SUCCEEDED),
        ChatToolCallStatus::Failed => ("✗", theme::colors().err, copy::TOOL_FAILED),
        ChatToolCallStatus::Refused => ("⊘", theme::colors().err, copy::TOOL_REFUSED),
        ChatToolCallStatus::Denied => ("⊘", theme::colors().err, copy::TOOL_DENIED),
        ChatToolCallStatus::Cancelled => ("○", theme::colors().muted, copy::TOOL_CANCELLED),
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

fn short_id(id: &str) -> &str {
    id.rsplit('-').next().unwrap_or(id)
}

fn indent() -> Span<'static> {
    Span::raw("    ")
}

pub const GOAL_TOOL: &str = "goal.complete";
pub const EVIDENCE_LINES: usize = 3;

fn goal_text(arguments: &Value, key: &str) -> String {
    match arguments.get(key) {
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn goal_block(call: &ChatToolCallDto, expanded: bool, width: usize) -> Vec<Line<'static>> {
    let room = width.saturating_sub(8).max(10);
    let mut lines = vec![Line::from(vec![
        Span::styled("  ✓ ", Style::new().fg(theme::colors().ok)),
        Span::styled(
            copy::GOAL_DONE,
            Style::new()
                .fg(theme::colors().ok)
                .add_modifier(Modifier::BOLD),
        ),
    ])];
    for piece in goal_text(&call.arguments, "summary")
        .split('\n')
        .flat_map(|line| wrap(line, room))
    {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(piece, theme::text()),
        ]));
    }
    let evidence: Vec<String> = goal_text(&call.arguments, "evidence")
        .split('\n')
        .flat_map(|line| wrap(line, room))
        .collect();
    if evidence.is_empty() {
        return lines;
    }
    lines.push(Line::from(vec![
        indent(),
        Span::styled(copy::GOAL_EVIDENCE, theme::eyebrow()),
    ]));
    let shown = if expanded {
        evidence.len()
    } else {
        EVIDENCE_LINES
    };
    for piece in evidence.iter().take(shown) {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(piece.clone(), theme::muted()),
        ]));
    }
    let hidden = evidence.len().saturating_sub(shown);
    if hidden > 0 {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(format!("… {} {hidden}", copy::MORE_LINES), theme::faint()),
        ]));
    }
    lines
}

fn tool(
    call: &ChatToolCallDto,
    expanded: bool,
    frame: u64,
    width: usize,
    details_key: Option<&str>,
) -> Vec<Line<'static>> {
    if call.key == GOAL_TOOL && call.status == ChatToolCallStatus::Succeeded {
        return goal_block(call, expanded, width);
    }
    let (icon, color, label) = status_look(call.status, frame);
    let pairs = arguments(&call.arguments);
    let action = match call.key.as_str() {
        "files.read" | "fs.read" => copy::TOOL_READ_ACTION,
        "files.list" | "fs.list" => copy::TOOL_LIST_ACTION,
        "files.apply" | "fs.apply" => copy::TOOL_EDIT_ACTION,
        "shell.run" | "shell.exec" | "process.run" => copy::COMMAND,
        _ => call.name.as_str(),
    };
    let details = if expanded {
        ""
    } else {
        let mut values = pairs
            .iter()
            .filter(|(_, value)| !value.is_empty() && value != "null");
        values
            .clone()
            .find(|(key, _)| matches!(key.as_str(), "path" | "paths" | "argv" | "command"))
            .or_else(|| values.next())
            .map_or("", |(_, value)| value.as_str())
    };
    let mut suffix = String::new();
    if call.status != ChatToolCallStatus::Succeeded {
        suffix.push_str(&format!(" · {label}"));
    }
    if call.duration_ms > 0 {
        suffix.push_str(&format!(" · {}", duration(call.duration_ms)));
    }
    if let Some(shell) = call
        .meta
        .as_ref()
        .and_then(|meta| meta.get("shellId"))
        .and_then(Value::as_str)
    {
        suffix.push_str(&format!(
            " · {} {}",
            copy::PROCESS_IN_BACKGROUND,
            short_id(shell)
        ));
    }
    let room = width.saturating_sub(2 + suffix.width());
    let action = truncate(action, room);
    let detail_room = room.saturating_sub(action.width() + 1);
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{icon} "), Style::new().fg(color)),
        Span::styled(action, theme::strong()),
        Span::styled(
            if detail_room == 0 || details.is_empty() {
                String::new()
            } else {
                format!(" {}", truncate(details, detail_room))
            },
            theme::accent(),
        ),
        Span::styled(suffix, theme::muted()),
    ])];
    let room = width.saturating_sub(4).max(1);
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
    }

    let output = call.error.as_ref().or(call.result.as_ref());
    if let Some(output) = output.filter(|text| !text.trim().is_empty()) {
        let style = if call.error.is_some() {
            Style::new().fg(theme::colors().err)
        } else if expanded {
            theme::text()
        } else {
            theme::muted()
        };
        let wrapped: Vec<String> = output
            .trim_end()
            .split('\n')
            .flat_map(|line| {
                wrap(
                    line,
                    if expanded {
                        room
                    } else {
                        room.saturating_sub(2)
                    },
                )
            })
            .collect();
        let shown = if expanded { RESULT_LINES } else { 3 };
        if expanded {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(copy::RESULT, theme::eyebrow()),
            ]));
        }
        for (index, piece) in wrapped.iter().take(shown).enumerate() {
            let lead = if expanded {
                ""
            } else if index == 0 {
                "⎿ "
            } else {
                "  "
            };
            lines.push(Line::from(vec![
                indent(),
                Span::styled(lead, theme::faint()),
                Span::styled(piece.clone(), style),
            ]));
        }
        let hidden = wrapped.len().saturating_sub(shown);
        if hidden > 0 {
            let shortcut = details_key
                .filter(|_| !expanded)
                .map_or_else(String::new, |key| format!(" · {key}"));
            lines.push(Line::from(vec![
                indent(),
                Span::styled(
                    format!(
                        "… {} {}{shortcut}",
                        copy::MORE_LINES,
                        format::number(hidden as u64)
                    ),
                    theme::faint(),
                ),
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

pub const SUBAGENT_STEPS_SHOWN: usize = 5;
pub const REPORT_LINES: usize = 8;

fn meta_text<'a>(call: &'a ChatToolCallDto, key: &str) -> &'a str {
    call.meta
        .as_ref()
        .and_then(|meta| meta.get(key))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn meta_number(call: &ChatToolCallDto, key: &str) -> Option<u64> {
    call.meta
        .as_ref()
        .and_then(|meta| meta.get(key))
        .and_then(Value::as_u64)
}

pub fn subagent_block(
    call: &ChatToolCallDto,
    children: &[ChatToolCallDto],
    expanded: bool,
    frame: u64,
    width: usize,
) -> Vec<Line<'static>> {
    let room = width.saturating_sub(8).max(10);
    let status = meta_text(call, "status");
    let running = status == "running" || status == "queued";
    let partial = call
        .meta
        .as_ref()
        .and_then(|meta| meta.get("partial"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let preset = if meta_text(call, "preset") == "general" {
        copy::SUBAGENT_GENERAL
    } else {
        copy::SUBAGENT_EXPLORE
    };
    let label = match meta_text(call, "label") {
        "" => meta_text(call, "task").lines().next().unwrap_or_default(),
        label => label,
    };
    let (icon, color, word) = match status {
        "running" => (
            spinner(frame),
            theme::colors().accent,
            copy::SUBAGENT_RUNNING,
        ),
        "queued" => (spinner(frame), theme::colors().muted, copy::SUBAGENT_QUEUED),
        "partial" => ("◐", theme::colors().warn, copy::SUBAGENT_PARTIAL),
        "cancelled" => ("○", theme::colors().muted, copy::SUBAGENT_STOPPED),
        "failed" => ("✗", theme::colors().err, copy::SUBAGENT_FAILED),
        _ => ("✓", theme::colors().ok, copy::SUBAGENT_DONE),
    };
    let chip = Style::new();
    let mut lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!(" {icon} "),
            chip.fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{preset} "),
            chip.fg(theme::colors().ink).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", truncate(label, room.min(60))),
            theme::text(),
        ),
        Span::styled(format!(" · {}", meta_text(call, "model")), theme::faint()),
    ])];
    if running {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(format!("{} ", spinner(frame)), Style::new().fg(color)),
            Span::styled(word, Style::new().fg(color)),
            Span::styled(
                format!(" · {}", format::tools(children.len() as u64)),
                theme::faint(),
            ),
        ]));
        let hidden = children.len().saturating_sub(SUBAGENT_STEPS_SHOWN);
        for child in children.iter().skip(hidden) {
            if let Some(mut first) = tool(child, false, frame, width.saturating_sub(4), None)
                .into_iter()
                .next()
            {
                first.spans.insert(0, indent());
                lines.push(first);
            }
        }
        return lines;
    }
    let mut status_line = vec![indent(), Span::styled(word, Style::new().fg(color))];
    if partial && status != "partial" {
        status_line.push(Span::styled(
            format!(" · {}", copy::SUBAGENT_PARTIAL),
            Style::new().fg(theme::colors().warn),
        ));
    }
    if let Some(count) = meta_number(call, "toolCalls") {
        status_line.push(Span::styled(
            format!(" · {}", format::tools(count)),
            theme::faint(),
        ));
    }
    lines.push(Line::from(status_line));
    let report: Vec<String> = meta_text(call, "report")
        .trim_end()
        .split('\n')
        .flat_map(|line| wrap(line, room))
        .collect();
    if report.iter().any(|line| !line.trim().is_empty()) {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(copy::SUBAGENT_REPORT, theme::eyebrow()),
        ]));
        let shown = if expanded { report.len() } else { REPORT_LINES };
        for piece in report.iter().take(shown) {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(piece.clone(), theme::text()),
            ]));
        }
        let hidden = report.len().saturating_sub(shown);
        if hidden > 0 {
            lines.push(Line::from(vec![
                indent(),
                Span::styled(format!("… {} {hidden}", copy::MORE_LINES), theme::faint()),
            ]));
        }
    } else if let Some(error) = call.error.as_ref().filter(|error| !error.is_empty()) {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(error.clone(), Style::new().fg(theme::colors().err)),
        ]));
    }
    let mut footer = Vec::new();
    if let Some(tokens) = meta_number(call, "tokens").filter(|tokens| *tokens > 0) {
        footer.push(format::tokens(tokens));
    }
    if let Some(ms) = meta_number(call, "durationMs") {
        footer.push(duration(ms));
    }
    if !footer.is_empty() {
        lines.push(Line::from(vec![
            indent(),
            Span::styled(footer.join(" · "), theme::faint()),
        ]));
    }
    lines
}

const SHELL_TAIL: usize = 12;
const SHELL_EXPANDED: usize = 400;

fn shell_block(run: &ShellRun, expanded: bool, frame: u64, width: usize) -> Vec<Line<'static>> {
    let (icon, color, label) = match run.end {
        None => (
            spinner(frame).to_owned(),
            theme::colors().accent,
            copy::SHELL_RUNNING.to_owned(),
        ),
        Some(ShellEnd::Exited(0)) => (
            "✓".to_owned(),
            theme::colors().ok,
            copy::SHELL_DONE.to_owned(),
        ),
        Some(ShellEnd::Exited(code)) => (
            "✗".to_owned(),
            theme::colors().err,
            format!("{} {code}", copy::SHELL_EXIT),
        ),
        Some(ShellEnd::Stopped) => (
            "○".to_owned(),
            theme::colors().muted,
            copy::SHELL_STOPPED.to_owned(),
        ),
        Some(ShellEnd::Failed) => (
            "✗".to_owned(),
            theme::colors().err,
            copy::SHELL_FAILED.to_owned(),
        ),
    };
    let chip = Style::new();
    let room = width.saturating_sub(8).max(10);
    let mut lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(
            " $ ",
            chip.fg(theme::colors().warn).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{} ", truncate(&run.command, room)),
            chip.fg(theme::colors().ink).add_modifier(Modifier::BOLD),
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
    wrap(text, width.saturating_sub(2))
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            Line::from(vec![
                Span::styled(if index == 0 { "› " } else { "  " }, theme::accent()),
                Span::styled(part, theme::text()),
            ])
        })
        .collect()
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

const FIELD_COLUMN: usize = 24;

fn field(label: &str, value: &str, room: usize) -> Vec<Line<'static>> {
    wrap(value, room.saturating_sub(FIELD_COLUMN).max(10))
        .into_iter()
        .enumerate()
        .map(|(index, piece)| {
            let lead = if index == 0 { label } else { "" };
            let pad = " ".repeat(FIELD_COLUMN.saturating_sub(lead.width()));
            Line::from(vec![
                indent(),
                Span::styled(format!("{lead}{pad}"), theme::muted()),
                Span::styled(piece, theme::text()),
            ])
        })
        .collect()
}

fn value_names(values: &std::collections::BTreeMap<String, McpValue>) -> String {
    values
        .iter()
        .map(|(name, value)| match value {
            McpValue::Secret { .. } => format!("{name} ({})", copy::MCP_SECRET),
            McpValue::Literal { .. } => name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn server_state(oauth: McpOAuthState, status: McpServerStatus) -> (&'static str, Color) {
    if oauth == McpOAuthState::Required {
        return (copy::MCP_SIGN_IN_REQUIRED, theme::colors().warn);
    }
    match status {
        McpServerStatus::Connected => (copy::MCP_CONNECTED, theme::colors().ok),
        McpServerStatus::Starting => (copy::MCP_STARTING, theme::colors().warn),
        McpServerStatus::Failed => (copy::MCP_FAILED, theme::colors().err),
        McpServerStatus::Stopped => (copy::MCP_STOPPED, theme::colors().muted),
        McpServerStatus::Disabled => (copy::MCP_DISABLED, theme::colors().muted),
    }
}

fn capability_block(
    title: &str,
    items: &[&McpCapabilityDto],
    expanded: bool,
    toggle: Option<&str>,
    room: usize,
) -> Vec<Line<'static>> {
    if items.is_empty() {
        return Vec::new();
    }
    let action = if expanded {
        copy::MCP_COLLAPSE
    } else {
        copy::MCP_EXPAND
    };
    let mut lines = vec![
        Line::default(),
        Line::from(vec![
            indent(),
            Span::styled(
                format!("{title} · {}", format::number(items.len() as u64)),
                theme::strong(),
            ),
            toggle.map_or_else(Span::default, |key| {
                Span::styled(format!("   {key} — {action}"), theme::faint())
            }),
        ]),
    ];
    if !expanded {
        return lines;
    }
    for item in items {
        let mut flags = Vec::new();
        if item.read_only {
            flags.push(copy::MCP_READ_ONLY);
        }
        if item.destructive {
            flags.push(copy::MCP_DESTRUCTIVE);
        }
        let mut head = vec![
            indent(),
            Span::styled("• ", theme::faint()),
            Span::styled(item.name.clone(), theme::accent()),
        ];
        if let Some(title) = item.title.as_deref().filter(|title| *title != item.name) {
            head.push(Span::styled(format!("  {title}"), theme::text()));
        }
        if !flags.is_empty() {
            head.push(Span::styled(
                format!("  [{}]", flags.join(", ")),
                theme::faint(),
            ));
        }
        lines.push(Line::from(head));
        let about = item.uri.as_deref().map_or_else(
            || item.description.clone(),
            |uri| {
                if item.description.is_empty() {
                    uri.to_owned()
                } else {
                    format!("{uri} — {}", item.description)
                }
            },
        );
        for piece in about
            .lines()
            .flat_map(|line| wrap(line, room.saturating_sub(4).max(10)))
        {
            lines.push(Line::from(vec![
                indent(),
                Span::raw("  "),
                Span::styled(piece, theme::muted()),
            ]));
        }
    }
    lines
}

pub(super) fn server_card(
    detail: &McpServerDetailDto,
    expanded: bool,
    toggle: Option<&str>,
    width: usize,
) -> Vec<Line<'static>> {
    let server = &detail.server;
    let room = width.saturating_sub(6).max(20);
    let (state, color) = server_state(server.oauth, server.status);
    let mut lines = vec![Line::from(vec![
        Span::styled("  ⧉ ", theme::accent()),
        Span::styled(server.name.clone(), theme::strong()),
        Span::styled(
            format!("  {state} · {}", server.transport.as_str()),
            Style::new().fg(color),
        ),
    ])];
    let mut fields: Vec<(&str, String)> = Vec::new();
    match &server.url {
        Some(url) => fields.push((copy::MCP_ADDRESS, url.clone())),
        None => fields.push((
            copy::MCP_COMMAND,
            std::iter::once(server.command.clone().unwrap_or_default())
                .chain(server.args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" "),
        )),
    }
    fields.push((copy::MCP_NAMESPACE, server.namespace.clone()));
    if let Some(name) = &server.server_name {
        let version = server.server_version.as_deref().unwrap_or_default();
        fields.push((
            copy::MCP_REPORTED_AS,
            format!("{name} {version}").trim().to_owned(),
        ));
    }
    if let Some(protocol) = &server.protocol_version {
        fields.push((copy::MCP_PROTOCOL, protocol.clone()));
    }
    let startup = match server.startup {
        McpStartup::Launch => copy::MCP_STARTUP_LAUNCH,
        McpStartup::Manual => copy::MCP_STARTUP_MANUAL,
    };
    fields.push((
        copy::MCP_STARTUP,
        if server.enabled {
            startup.to_owned()
        } else {
            format!("{startup}, {}", copy::MCP_DISABLED)
        },
    ));
    if server.transport == McpTransport::Http {
        let sign_in = match server.oauth {
            McpOAuthState::None => copy::MCP_SIGN_IN_NONE,
            McpOAuthState::Required => copy::MCP_SIGN_IN_REQUIRED,
            McpOAuthState::SignedIn => copy::MCP_SIGN_IN_DONE,
        };
        fields.push((copy::MCP_SIGN_IN, sign_in.to_owned()));
    }
    if !server.env.is_empty() {
        fields.push((copy::MCP_ENV, value_names(&server.env)));
    }
    if !server.headers.is_empty() {
        fields.push((copy::MCP_HEADERS, value_names(&server.headers)));
    }
    if let Some(at) = server.last_connected_at {
        fields.push((copy::MCP_LAST_CONNECTED, format::date(at)));
    }
    if let Some(error) = &server.last_error {
        fields.push((copy::MCP_ERROR, error.clone()));
    }
    if let Some(instructions) = server
        .instructions
        .as_deref()
        .filter(|text| !text.is_empty())
    {
        let shown = if expanded {
            instructions.to_owned()
        } else {
            instructions.lines().next().unwrap_or_default().to_owned()
        };
        fields.push((copy::MCP_INSTRUCTIONS, shown));
    }
    for (label, value) in &fields {
        lines.extend(field(label, value, room));
    }
    let of = |kind: McpCapabilityKind| -> Vec<&McpCapabilityDto> {
        detail
            .capabilities
            .iter()
            .filter(|capability| capability.kind == kind)
            .collect()
    };
    lines.extend(capability_block(
        copy::MCP_TOOLS,
        &of(McpCapabilityKind::Tool),
        expanded,
        toggle,
        room,
    ));
    lines.extend(capability_block(
        copy::MCP_RESOURCES,
        &of(McpCapabilityKind::Resource),
        expanded,
        toggle,
        room,
    ));
    lines.extend(capability_block(
        copy::MCP_PROMPTS,
        &of(McpCapabilityKind::Prompt),
        expanded,
        toggle,
        room,
    ));
    lines
}

fn assistant(text: &str, thought: &str, expanded: bool, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !thought.trim().is_empty() {
        lines.extend(reasoning(thought, expanded, width));
    }
    let text = text.trim();
    if text.is_empty() {
        return lines;
    }
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    let mut body = markdown(text, width, "  ");
    if text.starts_with('#') || text.starts_with("```") {
        lines.push(Line::from(Span::styled("●", theme::accent())));
    } else if let Some(lead) = body.first_mut().and_then(|line| line.spans.first_mut()) {
        *lead = Span::styled("● ", theme::accent());
    }
    lines.extend(body);
    lines
}

fn recap(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!("  ≡ {}", copy::RECAP_LABEL),
        theme::muted().add_modifier(Modifier::BOLD),
    ))];
    for paragraph in text.lines().filter(|line| !line.trim().is_empty()) {
        for piece in wrap(paragraph.trim(), width.saturating_sub(6).max(8)) {
            lines.push(Line::from(vec![
                Span::styled("    ", theme::muted()),
                Span::styled(piece, theme::muted()),
            ]));
        }
    }
    lines
}

fn site(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host)
}

fn sources_header(count: usize, expanded: bool, toggle: &str) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!(
            "  ◦ {} · {}",
            copy::WEB_SOURCES,
            format::number(count as u64)
        ),
        theme::muted().add_modifier(Modifier::BOLD),
    )];
    if count > SOURCES_SHOWN {
        let action = if expanded {
            copy::MCP_COLLAPSE
        } else {
            copy::MCP_EXPAND
        };
        spans.push(Span::styled(
            format!("   {toggle} — {action}"),
            theme::faint(),
        ));
    }
    Line::from(spans)
}

fn source_line(index: usize, source: &WebSourceDto, width: usize) -> Line<'static> {
    let number = format!("    {}. ", index + 1);
    let title = if source.title.trim().is_empty() {
        source.url.as_str()
    } else {
        source.title.trim()
    };
    let host = format!(" · {}", site(&source.url));
    let room = width.saturating_sub(number.width() + host.width()).max(8);
    Line::from(vec![
        Span::styled(number, theme::muted()),
        Span::styled(
            truncate(title, room),
            theme::accent().add_modifier(Modifier::UNDERLINED),
        ),
        Span::styled(host, theme::faint()),
    ])
}

pub const NOTE_LINK: &str = "zvs-note:";

fn note_line(index: usize, note: &NoteSourceDto, width: usize) -> Line<'static> {
    let number = format!("    {}. ✎ ", index + 1);
    let kind = format!(" · {}", copy::NOTE_SOURCE);
    let room = width.saturating_sub(number.width() + kind.width()).max(8);
    Line::from(vec![
        Span::styled(number, theme::muted()),
        Span::styled(
            truncate(note.title.trim(), room),
            theme::accent().add_modifier(Modifier::UNDERLINED),
        ),
        Span::styled(kind, theme::faint()),
    ])
}

fn compaction_mark(text: &str, width: usize) -> Vec<Line<'static>> {
    let room = width.saturating_sub(4).max(8);
    let label = format!(" {text} ");
    let used = label.chars().count().min(room);
    let side = room.saturating_sub(used) / 2;
    vec![Line::from(vec![
        Span::styled(format!("  {}", "─".repeat(side)), theme::faint()),
        Span::styled(label, theme::muted()),
        Span::styled("─".repeat(room.saturating_sub(used + side)), theme::faint()),
    ])]
}

fn notice(tone: Tone, text: &str, width: usize) -> Vec<Line<'static>> {
    let (icon, color) = match tone {
        Tone::Info => ("ⓘ", theme::colors().muted),
        Tone::Success => ("✓", theme::colors().ok),
        Tone::Error => ("✗", theme::colors().err),
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
    layout(app, width).0
}

pub fn layout(app: &App, width: usize) -> (Vec<Line<'static>>, Vec<(usize, String)>) {
    layout_lines(app, width, true)
}

pub fn history(app: &App, width: usize) -> Vec<Line<'static>> {
    layout_lines(app, width, false).0
}

fn layout_lines(
    app: &App,
    width: usize,
    include_live: bool,
) -> (Vec<Line<'static>>, Vec<(usize, String)>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut links: Vec<(usize, String)> = Vec::new();
    let expanded = app.expanded;
    let toggle = app.keymap.label(Shortcut::Details);
    let tool_shortcut = app.keymap.chord(Shortcut::Details).map(|_| toggle.as_str());
    for entry in &app.transcript.entries {
        if !matches!(entry, Entry::Attachments(_)) {
            lines.push(Line::default());
        }
        if let Entry::Sources(sources) = entry {
            lines.push(sources_header(sources.len(), expanded, &toggle));
            let shown = if expanded {
                sources.len()
            } else {
                sources.len().min(SOURCES_SHOWN)
            };
            for (index, source) in sources.iter().take(shown).enumerate() {
                match source {
                    ChatSourceDto::Web(source) => {
                        links.push((lines.len(), source.url.clone()));
                        lines.push(source_line(index, source, width));
                    }
                    ChatSourceDto::Note(note) => {
                        links.push((lines.len(), format!("{NOTE_LINK}{}", note.id)));
                        lines.push(note_line(index, note, width));
                    }
                    ChatSourceDto::Vector(_) => {}
                }
            }
            if shown < sources.len() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "    … {} {}",
                        copy::MORE,
                        format::sources((sources.len() - shown) as u64)
                    ),
                    theme::faint(),
                )));
            }
            continue;
        }
        lines.extend(match entry {
            Entry::User(text) => user(text, width),
            Entry::Attachments(attachments) => attachments
                .iter()
                .flat_map(|attachment| {
                    let color = if attachment.image {
                        theme::colors().ok
                    } else {
                        theme::colors().accent
                    };
                    let icon = if attachment.image { "▧" } else { "▤" };
                    wrap(&attachment.reference(), width.saturating_sub(8).max(1))
                        .into_iter()
                        .enumerate()
                        .map(move |(index, text)| {
                            Line::from(vec![
                                Span::styled(
                                    if index == 0 {
                                        format!("  └ {icon} ")
                                    } else {
                                        "      ".to_owned()
                                    },
                                    theme::faint(),
                                ),
                                Span::styled(text, Style::new().fg(color)),
                            ])
                        })
                })
                .collect(),
            Entry::Assistant { text, reasoning } => assistant(text, reasoning, expanded, width),
            Entry::Tool(call) if call.key == AGENT_DELEGATE && call.meta.is_some() => {
                subagent_block(call, &[], expanded, app.frame, width)
            }
            Entry::Tool(call) => tool(call, expanded, app.frame, width, tool_shortcut),
            Entry::Notice { tone, text } => notice(*tone, text, width),
            Entry::Recap(text) => recap(text, width),
            Entry::Compaction(text) => compaction_mark(text, width),
            Entry::Shell(run) => shell_block(run, expanded, app.frame, width),
            Entry::Server(detail) => server_card(detail, expanded, Some(&toggle), width),
            Entry::Sources(_) => Vec::new(),
        });
    }
    if !include_live {
        return (lines, links);
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
                Segment::Tool(call) if call.key == AGENT_DELEGATE && call.meta.is_some() => {
                    lines.extend(subagent_block(
                        call,
                        live.children_of(call),
                        expanded,
                        app.frame,
                        width,
                    ));
                }
                Segment::Tool(call) => {
                    lines.extend(tool(call, expanded, app.frame, width, tool_shortcut));
                }
                Segment::Text { text, reasoning } => {
                    let thinking = index == last && text.is_empty();
                    lines.extend(assistant(text, reasoning, expanded || thinking, width));
                }
            }
        }
        let seconds = app.frame.saturating_sub(app.started) / TICKS_PER_SECOND;
        let label = if app.cancelling {
            copy::CANCELLING
        } else if app.auto_compacting {
            copy::COMPACTING
        } else {
            copy::WORKING
        };
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", spinner(app.frame)), theme::accent()),
            Span::styled(label, theme::accent().add_modifier(Modifier::ITALIC)),
            Span::styled(format!(" · {seconds} с · Esc"), theme::faint()),
        ]));
    } else if app.compacting {
        let seconds = app.frame.saturating_sub(app.started) / TICKS_PER_SECOND;
        let label = if app.cancelling {
            copy::CANCELLING
        } else {
            copy::COMPACTING
        };
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", spinner(app.frame)), theme::accent()),
            Span::styled(label, theme::accent().add_modifier(Modifier::ITALIC)),
            Span::styled(format!(" · {seconds} с · Esc"), theme::faint()),
        ]));
    }
    (lines, links)
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
    fn a_collapsed_tool_shows_a_short_preview_and_the_expansion_key() {
        let text = plain(&tool(
            &call("one\ntwo\nthree\nfour"),
            false,
            0,
            80,
            Some("Ctrl+O"),
        ));
        assert_eq!(
            text[0],
            format!("✓ {} src/main.rs · 1,2 с", copy::TOOL_READ_ACTION)
        );
        assert_eq!(text[1], "    ⎿ one");
        assert_eq!(text[2], "      two");
        assert_eq!(text[3], "      three");
        assert_eq!(text[4], format!("    … {} 1 · Ctrl+O", copy::MORE_LINES));
    }

    #[test]
    fn an_expanded_tool_block_lists_every_argument_and_result_line() {
        let text = plain(&tool(&call("one\ntwo"), true, 0, 80, Some("Ctrl+O")));
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
