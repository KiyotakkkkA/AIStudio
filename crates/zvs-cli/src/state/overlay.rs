use zvs_contract::{
    ChatContextDto, CliTheme, ConversationDto, ConversationId, ExecJudgeDto, ExecMode,
    ExecPolicyDto, McpServerDto, McpServerId, McpServerStatus, ModelCapability, ProviderDto,
    ProviderUsageDto, SkillDto, SkillId, SkillSource, SkillStatus, ThinkingEffort,
};

use super::ModelChoice;
use super::input::Input;
use super::shortcuts::{Keymap, SHORTCUTS, Shortcut};
use crate::copy;
use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Models,
    Effort,
    Resume,
    Context,
    Skills,
    Mcp,
    Permissions,
    Shortcuts,
    Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Accent,
    Good,
    Warn,
    Bad,
    Faint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SelectModel(ModelChoice),
    SetEffort(ThinkingEffort),
    Resume(ConversationId),
    ToggleSkill { id: SkillId, enabled: bool },
    ToggleServer { id: McpServerId, start: bool },
    SetMode(ExecMode),
    SetJudge(Option<ExecJudgeDto>),
    Rebind(Shortcut),
    ResetShortcuts,
    SetTheme(CliTheme),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: String,
    pub icon: &'static str,
    pub tone: Tone,
    pub title: String,
    pub meta: String,
    pub detail: Vec<String>,
    pub gauge: Option<f64>,
    pub current: bool,
    pub action: Option<Action>,
}

impl Row {
    fn new(key: impl Into<String>, icon: &'static str, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            icon,
            tone: Tone::Plain,
            title: title.into(),
            meta: String::new(),
            detail: Vec::new(),
            gauge: None,
            current: false,
            action: None,
        }
    }

    fn meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = meta.into();
        self
    }

    fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    fn detail(mut self, lines: impl IntoIterator<Item = String>) -> Self {
        self.detail
            .extend(lines.into_iter().filter(|line| !line.is_empty()));
        self
    }

    fn action(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }

    const fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    const fn gauge(mut self, value: f64) -> Self {
        self.gauge = Some(value);
        self
    }

    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.title.to_lowercase().contains(query)
            || self.meta.to_lowercase().contains(query)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub panel: Panel,
    pub filter: Input,
    pub tab: usize,
    pub selected: usize,
    pub expanded: Vec<String>,
    pub loading: bool,
    pub armed: bool,
    pub capturing: Option<Shortcut>,
    pub conversations: Vec<ConversationDto>,
    pub context: Option<ChatContextDto>,
    pub usage: Option<ProviderUsageDto>,
    pub skills: Vec<SkillDto>,
    pub servers: Vec<McpServerDto>,
}

pub struct View<'a> {
    pub providers: &'a [ProviderDto],
    pub model: Option<&'a ModelChoice>,
    pub folder: &'a str,
    pub policy: Option<&'a ExecPolicyDto>,
    pub mode: ExecMode,
    pub effort: ThinkingEffort,
    pub keymap: &'a Keymap,
    pub theme: CliTheme,
}

pub const MODES: [ExecMode; 4] = [
    ExecMode::AlwaysAsk,
    ExecMode::SafetyAsk,
    ExecMode::AutoBypass,
    ExecMode::NoRestrictions,
];

pub const fn mode_label(mode: ExecMode) -> &'static str {
    match mode {
        ExecMode::AlwaysAsk => copy::MODE_ALWAYS_ASK,
        ExecMode::SafetyAsk => copy::MODE_SAFETY_ASK,
        ExecMode::AutoBypass => copy::MODE_AUTO_BYPASS,
        ExecMode::NoRestrictions => copy::MODE_NO_RESTRICTIONS,
    }
}

const fn mode_hint(mode: ExecMode) -> &'static str {
    match mode {
        ExecMode::AlwaysAsk => copy::MODE_HINT_ALWAYS_ASK,
        ExecMode::SafetyAsk => copy::MODE_HINT_SAFETY_ASK,
        ExecMode::AutoBypass => copy::MODE_HINT_AUTO_BYPASS,
        ExecMode::NoRestrictions => copy::MODE_HINT_NO_RESTRICTIONS,
    }
}

fn same_folder(left: &str, right: &str) -> bool {
    let clean = |path: &str| {
        path.trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_lowercase()
    };
    clean(left) == clean(right)
}

pub fn in_folder<'a>(
    conversations: &'a [ConversationDto],
    folder: &str,
) -> Vec<&'a ConversationDto> {
    let mut found: Vec<&ConversationDto> = conversations
        .iter()
        .filter(|conversation| {
            conversation
                .working_folder
                .as_deref()
                .is_some_and(|own| same_folder(own, folder))
        })
        .collect();
    found.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at));
    found
}

fn model_rows(view: &View<'_>, tab: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, provider) in view.providers.iter().enumerate() {
        if tab != 0 && tab != index + 1 {
            continue;
        }
        for model in &provider.models {
            let choice = ModelChoice {
                provider: provider.summary.id,
                provider_name: provider.summary.name.clone(),
                model: model.external_id.clone(),
                model_name: model.display_name.clone(),
                context_window: model.context_window,
            };
            let current = view.model.is_some_and(|selected| {
                selected.provider == choice.provider && selected.model == choice.model
            });
            let mut meta = vec![provider.summary.name.clone()];
            if let Some(window) = model.context_window {
                meta.push(format::tokens(window));
            }
            if current {
                meta.push(copy::MODEL_CURRENT.to_owned());
            }
            let mut row = Row::new(
                format!("{}/{}", provider.summary.id, model.id),
                "◆",
                format!("{} · {}", provider.summary.name, model.display_name),
            )
            .meta(meta.join(" · "))
            .current(current)
            .detail([
                model.external_id.clone(),
                model
                    .context_window
                    .map(|window| format!("{}: {}", copy::CONTEXT_WINDOW, format::tokens(window)))
                    .unwrap_or_default(),
            ]);
            if model.available {
                row = row
                    .tone(if current { Tone::Accent } else { Tone::Plain })
                    .action(Action::SelectModel(choice));
            } else {
                row = row.tone(Tone::Faint).meta(copy::UNAVAILABLE);
            }
            rows.push(row);
        }
    }
    rows
}

pub const EFFORT_STEPS: [ThinkingEffort; 3] = [
    ThinkingEffort::Fast,
    ThinkingEffort::Auto,
    ThinkingEffort::Thinking,
];
pub const EFFORT_FIXED_STEPS: [ThinkingEffort; 2] =
    [ThinkingEffort::Fast, ThinkingEffort::Thinking];

pub const fn effort_label(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Fast => copy::EFFORT_FAST,
        ThinkingEffort::Auto => copy::EFFORT_AUTO,
        ThinkingEffort::Thinking => copy::EFFORT_THINKING,
    }
}

const fn effort_hint(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Fast => copy::EFFORT_HINT_FAST,
        ThinkingEffort::Auto => copy::EFFORT_HINT_AUTO,
        ThinkingEffort::Thinking => copy::EFFORT_HINT_THINKING,
    }
}

fn effort_steps(view: &View<'_>) -> Option<&'static [ThinkingEffort]> {
    let model = view.model?;
    let capabilities = &view
        .providers
        .iter()
        .find(|provider| provider.summary.id == model.provider)?
        .models
        .iter()
        .find(|known| known.external_id == model.model)?
        .capabilities;
    if capabilities.contains(&ModelCapability::AdaptiveReasoning) {
        Some(&EFFORT_STEPS)
    } else if capabilities.contains(&ModelCapability::Reasoning) {
        Some(&EFFORT_FIXED_STEPS)
    } else {
        None
    }
}

fn effort_rows(view: &View<'_>) -> Vec<Row> {
    if view.model.is_none() {
        return vec![Row::new("none", "◐", copy::CONTEXT_NEEDS_MODEL).tone(Tone::Faint)];
    }
    let Some(steps) = effort_steps(view) else {
        return vec![Row::new("unsupported", "◐", copy::EFFORT_UNSUPPORTED).tone(Tone::Faint)];
    };
    let copilot = view.model.is_some_and(|model| {
        view.providers.iter().any(|provider| {
            provider.summary.id == model.provider
                && provider.summary.adapter == zvs_contract::AdapterFamily::Copilot
        })
    });
    steps
        .iter()
        .map(|effort| {
            let current = *effort == view.effort;
            let (label, hint) = if copilot {
                match effort {
                    ThinkingEffort::Fast => {
                        (copy::COPILOT_EFFORT_LOW, copy::COPILOT_EFFORT_LOW_HINT)
                    }
                    ThinkingEffort::Auto => (copy::EFFORT_AUTO, copy::COPILOT_EFFORT_AUTO_HINT),
                    ThinkingEffort::Thinking => {
                        (copy::COPILOT_EFFORT_HIGH, copy::COPILOT_EFFORT_HIGH_HINT)
                    }
                }
            } else {
                (effort_label(*effort), effort_hint(*effort))
            };
            Row::new(effort.as_str(), "◐", label)
                .meta(if current { copy::EFFORT_CURRENT } else { "" })
                .tone(if current { Tone::Accent } else { Tone::Plain })
                .current(current)
                .detail([hint.to_owned()])
                .action(Action::SetEffort(*effort))
        })
        .collect()
}

pub const fn theme_label(theme: CliTheme) -> &'static str {
    match theme {
        CliTheme::Ember => copy::THEME_EMBER,
        CliTheme::Graphite => copy::THEME_GRAPHITE,
        CliTheme::Moss => copy::THEME_MOSS,
        CliTheme::Dusk => copy::THEME_DUSK,
        CliTheme::Frost => copy::THEME_FROST,
    }
}

const fn theme_hint(theme: CliTheme) -> &'static str {
    match theme {
        CliTheme::Ember => copy::THEME_HINT_EMBER,
        CliTheme::Graphite => copy::THEME_HINT_GRAPHITE,
        CliTheme::Moss => copy::THEME_HINT_MOSS,
        CliTheme::Dusk => copy::THEME_HINT_DUSK,
        CliTheme::Frost => copy::THEME_HINT_FROST,
    }
}

fn theme_rows(view: &View<'_>) -> Vec<Row> {
    CliTheme::ALL
        .iter()
        .map(|theme| {
            let current = *theme == view.theme;
            Row::new(theme.as_str(), "●", theme_label(*theme))
                .meta(if current { copy::THEME_CURRENT } else { "" })
                .tone(if current { Tone::Accent } else { Tone::Plain })
                .current(current)
                .detail([theme_hint(*theme).to_owned()])
                .action(Action::SetTheme(*theme))
        })
        .collect()
}

fn resume_rows(overlay: &Overlay, view: &View<'_>) -> Vec<Row> {
    in_folder(&overlay.conversations, view.folder)
        .into_iter()
        .filter(|conversation| overlay.tab == 0 || conversation.pinned)
        .map(|conversation| {
            Row::new(conversation.id.to_string(), "↺", conversation.title.clone())
                .meta(conversation.model_id.clone())
                .tone(if conversation.pinned {
                    Tone::Accent
                } else {
                    Tone::Plain
                })
                .detail([conversation.working_folder.clone().unwrap_or_default()])
                .action(Action::Resume(conversation.id))
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

fn context_rows(overlay: &Overlay) -> Vec<Row> {
    let Some(context) = &overlay.context else {
        return Vec::new();
    };
    let window = context.context_window;
    let line = |key: &str, title: &str, value: u64, tone: Tone| {
        Row::new(key, "▤", title)
            .meta(format!(
                "{} · {}",
                format::tokens(value),
                format::percent(value, window)
            ))
            .gauge(share(value, window))
            .tone(tone)
    };
    if overlay.tab == 1 {
        let mut tools = context.tool_items.clone();
        tools.sort_by_key(|tool| std::cmp::Reverse(tool.tokens));
        return tools
            .iter()
            .map(|tool| line(&tool.key, &tool.key, tool.tokens, Tone::Plain))
            .collect();
    }
    let free = window.saturating_sub(context.total + context.reserved_output);
    let mut rows = vec![
        line(
            "used",
            copy::CONTEXT_USED,
            context.total,
            if share(context.total, window) > 0.8 {
                Tone::Warn
            } else {
                Tone::Accent
            },
        ),
        line("system", copy::CONTEXT_SYSTEM, context.system, Tone::Plain),
        line("tools", copy::CONTEXT_TOOLS, context.tools, Tone::Plain).detail([format::tools(
            u64::try_from(context.tool_items.len()).unwrap_or(0),
        )]),
        line(
            "history",
            copy::CONTEXT_HISTORY,
            context.history,
            Tone::Plain,
        )
        .detail([format::messages(context.history_messages)]),
        line(
            "reserved",
            copy::CONTEXT_RESERVED,
            context.reserved_output,
            Tone::Faint,
        ),
        line("free", copy::CONTEXT_FREE, free, Tone::Good),
    ];
    if context.trimmed > 0 {
        rows.push(
            Row::new("trimmed", "✂", copy::CONTEXT_TRIMMED)
                .meta(format::messages(context.trimmed))
                .tone(Tone::Warn),
        );
    }
    if let Some(usage) = &overlay.usage {
        rows.extend(limit_rows(usage));
    }
    rows
}

fn limit_rows(usage: &ProviderUsageDto) -> Vec<Row> {
    let mut rows: Vec<Row> = usage
        .windows
        .iter()
        .enumerate()
        .map(|(index, window)| {
            let used = window.used_percent.clamp(0.0, 100.0);
            Row::new(format!("limit-{index}"), "◷", window.label.clone())
                .meta(format!("{} {}\u{a0}%", copy::LIMIT_USED, used.round()))
                .gauge(used / 100.0)
                .tone(if used >= 85.0 { Tone::Warn } else { Tone::Good })
                .detail(
                    window
                        .resets_at
                        .map(|at| format!("{} {}", copy::LIMIT_RESETS, format::date(at))),
                )
        })
        .collect();
    if let Some(detail) = &usage.detail {
        rows.push(Row::new("limit-detail", "◷", detail.clone()).tone(Tone::Faint));
    }
    rows
}

const fn source_label(source: SkillSource) -> &'static str {
    match source {
        SkillSource::Local => copy::SOURCE_LOCAL,
        SkillSource::Imported => copy::SOURCE_IMPORTED,
        SkillSource::Bundled => copy::SOURCE_BUNDLED,
    }
}

fn skill_rows(overlay: &Overlay) -> Vec<Row> {
    overlay
        .skills
        .iter()
        .filter(|skill| match overlay.tab {
            1 => skill.enabled,
            2 => !skill.enabled,
            _ => true,
        })
        .map(|skill| {
            let (state, tone) = if skill.status == SkillStatus::Invalid {
                (copy::SKILL_INVALID, Tone::Bad)
            } else if skill.enabled {
                (copy::SKILL_ON, Tone::Good)
            } else {
                (copy::SKILL_OFF, Tone::Faint)
            };
            Row::new(skill.id.to_string(), "✦", skill.name.clone())
                .meta(format!(
                    "{state} · {} · {}",
                    source_label(skill.source),
                    format::files(skill.file_count)
                ))
                .tone(tone)
                .current(skill.enabled)
                .detail([
                    skill.description.clone(),
                    skill.key.clone(),
                    skill.version.clone().unwrap_or_default(),
                ])
                .action(Action::ToggleSkill {
                    id: skill.id,
                    enabled: !skill.enabled,
                })
        })
        .collect()
}

const fn server_state(status: McpServerStatus) -> (&'static str, Tone) {
    match status {
        McpServerStatus::Disabled => (copy::MCP_DISABLED, Tone::Faint),
        McpServerStatus::Stopped => (copy::MCP_STOPPED, Tone::Faint),
        McpServerStatus::Starting => (copy::MCP_STARTING, Tone::Warn),
        McpServerStatus::Connected => (copy::MCP_CONNECTED, Tone::Good),
        McpServerStatus::Failed => (copy::MCP_FAILED, Tone::Bad),
    }
}

fn server_rows(overlay: &Overlay) -> Vec<Row> {
    overlay
        .servers
        .iter()
        .filter(|server| {
            let live = matches!(
                server.status,
                McpServerStatus::Connected | McpServerStatus::Starting
            );
            match overlay.tab {
                1 => live,
                2 => !live,
                _ => true,
            }
        })
        .map(|server| {
            let (state, tone) = server_state(server.status);
            let running = matches!(
                server.status,
                McpServerStatus::Connected | McpServerStatus::Starting
            );
            let target = server
                .url
                .clone()
                .or_else(|| {
                    server
                        .command
                        .clone()
                        .map(|command| format!("{command} {}", server.args.join(" ")))
                })
                .unwrap_or_default();
            Row::new(server.id.to_string(), "⧉", server.name.clone())
                .meta(format!(
                    "{state} · {} · {}",
                    server.transport.as_str(),
                    format::tools(server.tool_count)
                ))
                .tone(tone)
                .current(running)
                .detail([
                    target.trim().to_owned(),
                    server.last_error.clone().unwrap_or_default(),
                ])
                .action(Action::ToggleServer {
                    id: server.id,
                    start: !running,
                })
        })
        .collect()
}

fn permission_rows(overlay: &Overlay, view: &View<'_>) -> Vec<Row> {
    let judge = view.policy.and_then(|policy| policy.judge.as_ref());
    if overlay.tab == 0 {
        return MODES
            .iter()
            .map(|mode| {
                let current = *mode == view.mode;
                Row::new(mode.as_str(), "◉", mode_label(*mode))
                    .meta(if current { copy::MODE_CURRENT } else { "" })
                    .tone(match (current, mode) {
                        (true, ExecMode::NoRestrictions) => Tone::Warn,
                        (true, _) => Tone::Accent,
                        _ => Tone::Plain,
                    })
                    .current(current)
                    .detail([mode_hint(*mode).to_owned()])
                    .action(Action::SetMode(*mode))
            })
            .collect();
    }
    let mut rows = vec![
        Row::new("none", "○", copy::JUDGE_NONE)
            .current(judge.is_none())
            .tone(if judge.is_none() {
                Tone::Accent
            } else {
                Tone::Plain
            })
            .detail([copy::JUDGE_HINT.to_owned()])
            .action(Action::SetJudge(None)),
    ];
    for provider in view.providers {
        for model in provider.models.iter().filter(|model| model.available) {
            let candidate = ExecJudgeDto {
                provider_id: provider.summary.id.to_string(),
                model_id: model.external_id.clone(),
            };
            let current = judge.is_some_and(|judge| {
                judge.provider_id == candidate.provider_id
                    && (judge.model_id == candidate.model_id
                        || judge.model_id == model.id.to_string())
            });
            rows.push(
                Row::new(
                    format!("{}/{}", provider.summary.id, model.id),
                    "◆",
                    format!("{} · {}", provider.summary.name, model.display_name),
                )
                .meta(if current { copy::MODE_CURRENT } else { "" })
                .tone(if current { Tone::Accent } else { Tone::Plain })
                .current(current)
                .action(Action::SetJudge(Some(candidate))),
            );
        }
    }
    rows
}

fn shortcut_rows(overlay: &Overlay, view: &View<'_>) -> Vec<Row> {
    SHORTCUTS
        .iter()
        .map(|shortcut| {
            let capturing = overlay.capturing == Some(*shortcut);
            let bound = view.keymap.chord(*shortcut).is_some();
            Row::new(shortcut.id(), "⌨", shortcut.description())
                .meta(if capturing {
                    copy::SHORTCUT_PRESS.to_owned()
                } else {
                    view.keymap.label(*shortcut)
                })
                .tone(if capturing {
                    Tone::Warn
                } else if bound {
                    Tone::Plain
                } else {
                    Tone::Faint
                })
                .current(capturing)
                .action(Action::Rebind(*shortcut))
        })
        .chain(std::iter::once(
            Row::new("reset", "↺", copy::SHORTCUTS_RESET)
                .tone(Tone::Accent)
                .action(Action::ResetShortcuts),
        ))
        .collect()
}

impl Overlay {
    pub fn new(panel: Panel) -> Self {
        Self {
            panel,
            filter: Input::default(),
            tab: 0,
            selected: 0,
            expanded: Vec::new(),
            loading: !matches!(
                panel,
                Panel::Models | Panel::Effort | Panel::Shortcuts | Panel::Theme
            ),
            armed: false,
            capturing: None,
            conversations: Vec::new(),
            context: None,
            usage: None,
            skills: Vec::new(),
            servers: Vec::new(),
        }
    }

    pub const fn title(&self) -> &'static str {
        match self.panel {
            Panel::Models => copy::MODELS_TITLE,
            Panel::Effort => copy::EFFORT_TITLE,
            Panel::Resume => copy::RESUME_TITLE,
            Panel::Context => copy::CONTEXT_TITLE,
            Panel::Skills => copy::SKILLS_TITLE,
            Panel::Mcp => copy::MCP_TITLE,
            Panel::Permissions => copy::PERMISSIONS_TITLE,
            Panel::Shortcuts => copy::SHORTCUTS_TITLE,
            Panel::Theme => copy::THEME_TITLE,
        }
    }

    pub const fn icon(&self) -> &'static str {
        match self.panel {
            Panel::Models => "◆",
            Panel::Effort => "◐",
            Panel::Resume => "↺",
            Panel::Context => "▤",
            Panel::Skills => "✦",
            Panel::Mcp => "⧉",
            Panel::Permissions => "◉",
            Panel::Shortcuts => "⌨",
            Panel::Theme => "◑",
        }
    }

    pub fn tabs(&self, view: &View<'_>) -> Vec<String> {
        let fixed = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect();
        match self.panel {
            Panel::Models => std::iter::once(copy::TAB_ALL.to_owned())
                .chain(
                    view.providers
                        .iter()
                        .map(|provider| provider.summary.name.clone()),
                )
                .collect(),
            Panel::Resume => fixed(&[copy::TAB_RECENT, copy::TAB_PINNED]),
            Panel::Context => fixed(&[copy::TAB_SUMMARY, copy::TAB_TOOLS]),
            Panel::Skills => fixed(&[copy::TAB_ALL, copy::TAB_ENABLED, copy::TAB_DISABLED]),
            Panel::Mcp => fixed(&[copy::TAB_ALL, copy::TAB_CONNECTED, copy::TAB_STOPPED]),
            Panel::Permissions => fixed(&[copy::TAB_MODE, copy::TAB_JUDGE]),
            Panel::Effort | Panel::Shortcuts | Panel::Theme => Vec::new(),
        }
    }

    pub fn rows(&self, view: &View<'_>) -> Vec<Row> {
        let rows = match self.panel {
            Panel::Models => model_rows(view, self.tab),
            Panel::Effort => effort_rows(view),
            Panel::Resume => resume_rows(self, view),
            Panel::Context => context_rows(self),
            Panel::Skills => skill_rows(self),
            Panel::Mcp => server_rows(self),
            Panel::Permissions => permission_rows(self, view),
            Panel::Shortcuts => shortcut_rows(self, view),
            Panel::Theme => theme_rows(view),
        };
        let query = self.filter.text().trim().to_lowercase();
        rows.into_iter().filter(|row| row.matches(&query)).collect()
    }

    pub fn select_current(&mut self, view: &View<'_>) {
        if let Some(index) = self.rows(view).iter().position(|row| row.current) {
            self.selected = index;
        }
    }

    pub fn clamp(&mut self, count: usize) {
        self.selected = self.selected.min(count.saturating_sub(1));
    }

    pub fn up(&mut self, count: usize) {
        if count > 0 {
            self.selected = (self.selected + count - 1) % count;
        }
        self.armed = false;
    }

    pub fn down(&mut self, count: usize) {
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
        self.armed = false;
    }

    pub fn next_tab(&mut self, count: usize, forward: bool) {
        if count == 0 {
            return;
        }
        self.tab = if forward {
            (self.tab + 1) % count
        } else {
            (self.tab + count - 1) % count
        };
        self.selected = 0;
        self.armed = false;
    }

    pub fn toggle(&mut self, key: &str) {
        if let Some(index) = self.expanded.iter().position(|known| known == key) {
            self.expanded.remove(index);
        } else {
            self.expanded.push(key.to_owned());
        }
    }

    pub fn is_expanded(&self, key: &str) -> bool {
        self.expanded.iter().any(|known| known == key)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const PROVIDERS: &str =
        include_str!("../../../../packages/shared/fixtures/providers/ProviderDto.valid.json");
    const CONVERSATIONS: &str =
        include_str!("../../../../packages/shared/fixtures/chat/ConversationDto.valid.json");

    fn providers() -> Vec<ProviderDto> {
        let cases: Vec<Value> = serde_json::from_str(PROVIDERS).expect("fixture");
        vec![serde_json::from_value(cases[0]["value"].clone()).expect("provider")]
    }

    fn conversation(folder: &str, updated: i64, pinned: bool) -> ConversationDto {
        let cases: Vec<Value> = serde_json::from_str(CONVERSATIONS).expect("fixture");
        let mut value = cases[0]["value"].clone();
        value["id"] = json!(format!("0199aa11-2222-7111-8111-{updated:012}"));
        value["title"] = json!(format!("Диалог {updated}"));
        value["workingFolder"] = json!(folder);
        value["workingFolderAccess"] = json!("granted");
        value["updatedAt"] = json!(updated);
        value["pinned"] = json!(pinned);
        serde_json::from_value(value).expect("conversation")
    }

    static KEYMAP: std::sync::LazyLock<Keymap> = std::sync::LazyLock::new(Keymap::default);

    fn view(providers: &[ProviderDto]) -> View<'_> {
        View {
            providers,
            model: None,
            folder: r"D:\work",
            policy: None,
            mode: ExecMode::SafetyAsk,
            effort: ThinkingEffort::Auto,
            keymap: &KEYMAP,
            theme: CliTheme::Ember,
        }
    }

    #[test]
    fn models_are_listed_per_provider_and_filtered_by_text() {
        let providers = providers();
        let view = view(&providers);
        let mut overlay = Overlay::new(Panel::Models);
        assert_eq!(overlay.tabs(&view), ["Все", "OpenRouter"]);
        let rows = overlay.rows(&view);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "OpenRouter · GPT-4o mini");
        assert!(matches!(rows[0].action, Some(Action::SelectModel(_))));
        overlay.filter.insert("claude");
        assert!(overlay.rows(&view).is_empty());
    }

    #[test]
    fn resume_lists_only_this_folder_newest_first_and_pins_on_their_tab() {
        let providers = providers();
        let view = view(&providers);
        let mut overlay = Overlay::new(Panel::Resume);
        overlay.conversations = vec![
            conversation(r"D:\work", 1, false),
            conversation(r"d:\work\", 3, true),
            conversation(r"D:\other", 5, false),
        ];
        let titles: Vec<String> = overlay
            .rows(&view)
            .into_iter()
            .map(|row| row.title)
            .collect();
        assert_eq!(titles, ["Диалог 3", "Диалог 1"]);
        overlay.next_tab(2, true);
        assert_eq!(overlay.rows(&view).len(), 1);
    }

    #[test]
    fn permissions_mark_the_current_mode_and_offer_judges() {
        let providers = providers();
        let view = view(&providers);
        let mut overlay = Overlay::new(Panel::Permissions);
        overlay.select_current(&view);
        assert_eq!(overlay.selected, 1);
        overlay.next_tab(2, true);
        let rows = overlay.rows(&view);
        assert_eq!(rows[0].title, copy::JUDGE_NONE);
        assert!(rows[0].current);
        assert_eq!(
            rows[1].action,
            Some(Action::SetJudge(Some(ExecJudgeDto {
                provider_id: "0199aa11-1111-7111-8111-111111111111".to_owned(),
                model_id: "openai/gpt-4o-mini".to_owned(),
            })))
        );
    }

    #[test]
    fn context_shares_are_fractions_of_the_window() {
        let providers = providers();
        let view = view(&providers);
        let mut overlay = Overlay::new(Panel::Context);
        overlay.context = Some(
            serde_json::from_value(json!({
                "contextWindow": 1000, "reservedOutput": 100, "system": 50, "tools": 150,
                "toolItems": [{ "key": "files.read", "tokens": 150, "source": "base" }],
                "history": 300, "historyMessages": 4, "trimmed": 0, "total": 500
            }))
            .expect("context"),
        );
        let rows = overlay.rows(&view);
        assert_eq!(rows[0].gauge, Some(0.5));
        assert_eq!(rows.last().map(|row| row.gauge), Some(Some(0.4)));
        overlay.next_tab(2, true);
        assert_eq!(overlay.rows(&view)[0].title, "files.read");
    }

    #[test]
    fn every_theme_is_listed_and_the_active_one_is_marked() {
        let providers = providers();
        let mut view = view(&providers);
        view.theme = CliTheme::Moss;
        let mut overlay = Overlay::new(Panel::Theme);
        assert!(!overlay.loading);
        let rows = overlay.rows(&view);
        assert_eq!(rows.len(), CliTheme::ALL.len());
        overlay.select_current(&view);
        assert_eq!(rows[overlay.selected].key, CliTheme::Moss.as_str());
        assert_eq!(rows[overlay.selected].meta, copy::THEME_CURRENT);
        assert_eq!(rows[0].action, Some(Action::SetTheme(CliTheme::Ember)));
    }

    #[test]
    fn provider_limits_follow_the_context_breakdown() {
        let mut overlay = Overlay::new(Panel::Context);
        overlay.context = Some(
            serde_json::from_value(json!({
                "contextWindow": 1000, "reservedOutput": 100, "system": 50, "tools": 0,
                "toolItems": [], "history": 0, "historyMessages": 0, "trimmed": 0, "total": 50
            }))
            .expect("context"),
        );
        overlay.usage = Some(
            serde_json::from_value(json!({
                "source": "copilot", "checkedAt": 1,
                "windows": [{ "label": "Премиум-запросы · 40 / 50", "usedPercent": 90,
                    "resetsAt": 1_793_491_200_000_i64 }],
                "detail": "Запросов сверх лимита: 3"
            }))
            .expect("usage"),
        );
        let providers = providers();
        let view = view(&providers);
        let rows = overlay.rows(&view);
        let limit = &rows[rows.len() - 2];
        assert_eq!(limit.title, "Премиум-запросы · 40 / 50");
        assert_eq!(limit.gauge, Some(0.9));
        assert_eq!(limit.tone, Tone::Warn);
        assert_eq!(limit.meta, format!("{} 90\u{a0}%", copy::LIMIT_USED));
        assert_eq!(limit.detail, [format!("{} 01.11.2026", copy::LIMIT_RESETS)]);
        let detail = rows.last().expect("a detail row");
        assert_eq!(detail.title, "Запросов сверх лимита: 3");
        assert_eq!(detail.tone, Tone::Faint);
    }

    #[test]
    fn expanding_is_toggled_per_row_key() {
        let mut overlay = Overlay::new(Panel::Skills);
        overlay.toggle("a");
        assert!(overlay.is_expanded("a"));
        overlay.toggle("a");
        assert!(!overlay.is_expanded("a"));
    }
}
