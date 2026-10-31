use zvs_contract::{
    CliTheme, ConversationDto, ConversationId, McpOAuthState, McpServerDto, McpServerId,
};

use super::overlay;
use crate::copy;
use crate::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Help,
    New,
    Resume,
    Models,
    Effort,
    Connect,
    Desktop,
    Context,
    Skills,
    Mcp,
    Permissions,
    Shortcuts,
    Theme,
    Clear,
    Exit,
}

pub const COMMANDS: &[Command] = &[
    Command::Help,
    Command::Models,
    Command::Effort,
    Command::Connect,
    Command::Resume,
    Command::Desktop,
    Command::New,
    Command::Context,
    Command::Skills,
    Command::Mcp,
    Command::Permissions,
    Command::Shortcuts,
    Command::Theme,
    Command::Clear,
    Command::Exit,
];

impl Command {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::New => "new",
            Self::Resume => "resume",
            Self::Models => "models",
            Self::Effort => "effort",
            Self::Connect => "connect",
            Self::Desktop => "desktop",
            Self::Context => "context",
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Permissions => "permissions",
            Self::Shortcuts => "shortcuts",
            Self::Theme => "theme",
            Self::Clear => "clear",
            Self::Exit => "exit",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Help => copy::CMD_HELP,
            Self::New => copy::CMD_NEW,
            Self::Resume => copy::CMD_RESUME,
            Self::Models => copy::CMD_MODELS,
            Self::Effort => copy::CMD_EFFORT,
            Self::Connect => copy::CMD_CONNECT,
            Self::Desktop => copy::CMD_DESKTOP,
            Self::Context => copy::CMD_CONTEXT,
            Self::Skills => copy::CMD_SKILLS,
            Self::Mcp => copy::CMD_MCP,
            Self::Permissions => copy::CMD_PERMISSIONS,
            Self::Shortcuts => copy::CMD_SHORTCUTS,
            Self::Theme => copy::CMD_THEME,
            Self::Clear => copy::CMD_CLEAR,
            Self::Exit => copy::CMD_EXIT,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let name = text.trim().strip_prefix('/')?;
        let name = name.split_whitespace().next().unwrap_or_default();
        COMMANDS
            .iter()
            .copied()
            .find(|command| command.name().eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    selected: usize,
}

pub fn arguments(text: &str) -> &str {
    text.trim()
        .split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest.trim())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    Server,
    Theme,
    Conversation,
}

impl Command {
    pub const fn arg(self) -> Option<Arg> {
        match self {
            Self::Theme => Some(Arg::Theme),
            Self::Resume => Some(Arg::Conversation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sub {
    McpAdd,
    McpInspect,
}

pub const SUBS: &[Sub] = &[Sub::McpAdd, Sub::McpInspect];

impl Sub {
    pub const fn parent(self) -> Command {
        match self {
            Self::McpAdd | Self::McpInspect => Command::Mcp,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::McpAdd => "add",
            Self::McpInspect => "inspect",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::McpAdd => copy::CMD_MCP_ADD,
            Self::McpInspect => copy::CMD_MCP_INSPECT,
        }
    }

    pub const fn arg(self) -> Option<Arg> {
        match self {
            Self::McpAdd => None,
            Self::McpInspect => Some(Arg::Server),
        }
    }

    pub fn label(self) -> String {
        format!("{} {}", self.parent().name(), self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Server(McpServerId),
    Theme(CliTheme),
    Conversation(ConversationId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub label: String,
    pub meta: String,
    pub completion: String,
    pub pick: Pick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Command(Command),
    Sub(Sub),
    Value(Value),
}

impl Item {
    pub fn label(&self) -> String {
        match self {
            Self::Command(command) => command.name().to_owned(),
            Self::Sub(sub) => sub.label(),
            Self::Value(value) => value.label.clone(),
        }
    }

    pub fn shown(&self) -> String {
        match self {
            Self::Value(value) => value.label.clone(),
            other => format!("/{}", other.label()),
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::Command(command) => command.description().to_owned(),
            Self::Sub(sub) => sub.description().to_owned(),
            Self::Value(value) => value.meta.clone(),
        }
    }

    pub fn completion(&self) -> String {
        match self {
            Self::Command(command) => format!("/{}", command.name()),
            Self::Sub(sub) => format!("/{} ", sub.label()),
            Self::Value(value) => value.completion.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    pub servers: Option<&'a [McpServerDto]>,
    pub conversations: Option<&'a [ConversationDto]>,
    pub folder: &'a str,
}

pub fn subs_of(command: Command) -> impl Iterator<Item = Sub> {
    SUBS.iter()
        .copied()
        .filter(move |sub| sub.parent() == command)
}

pub fn every_item() -> Vec<Item> {
    COMMANDS
        .iter()
        .flat_map(|command| {
            std::iter::once(Item::Command(*command)).chain(subs_of(*command).map(Item::Sub))
        })
        .collect()
}

fn find_command(name: &str) -> Option<Command> {
    COMMANDS
        .iter()
        .copied()
        .find(|command| command.name().eq_ignore_ascii_case(name))
}

fn sub_query(text: &str) -> Option<(Command, String)> {
    let rest = text.strip_prefix('/')?;
    let (name, partial) = rest.split_once(' ')?;
    if partial.contains(char::is_whitespace) {
        return None;
    }
    let command = find_command(name)?;
    subs_of(command)
        .next()
        .map(|_| (command, partial.to_lowercase()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgSlot {
    pub arg: Arg,
    pub prefix: String,
    pub query: String,
}

pub fn arg_slot(text: &str) -> Option<ArgSlot> {
    let rest = text.strip_prefix('/')?;
    let (name, tail) = rest.split_once(' ')?;
    let command = find_command(name)?;
    if subs_of(command).next().is_some() {
        let (word, query) = tail.split_once(' ')?;
        let sub = subs_of(command).find(|sub| sub.name().eq_ignore_ascii_case(word))?;
        return sub.arg().map(|arg| ArgSlot {
            arg,
            prefix: format!("/{} {} ", command.name(), sub.name()),
            query: query.trim_start().to_lowercase(),
        });
    }
    command.arg().map(|arg| ArgSlot {
        arg,
        prefix: format!("/{} ", command.name()),
        query: tail.trim_start().to_lowercase(),
    })
}

pub fn is_command_text(text: &str) -> bool {
    text.starts_with('/')
        && (!text.contains(char::is_whitespace)
            || sub_query(text).is_some()
            || arg_slot(text).is_some())
}

fn score(name: &str, query: &str) -> Option<usize> {
    if query.is_empty() || name.starts_with(query) {
        return Some(0);
    }
    if name.contains(query) {
        return Some(1);
    }
    let mut rest = name.chars();
    query
        .chars()
        .all(|wanted| rest.any(|next| next == wanted))
        .then_some(2)
}

fn ranked(mut found: Vec<(usize, usize, Value)>) -> Vec<Value> {
    found.sort_by_key(|(rank, order, _)| (*rank, *order));
    found.into_iter().map(|(_, _, value)| value).collect()
}

fn server_values(slot: &ArgSlot, servers: &[McpServerDto]) -> Vec<Value> {
    ranked(
        servers
            .iter()
            .enumerate()
            .filter_map(|(order, server)| {
                let rank = score(&server.name.to_lowercase(), &slot.query)?;
                let state = if server.oauth == McpOAuthState::Required {
                    copy::MCP_SIGN_IN_REQUIRED
                } else {
                    overlay::server_state(server.status).0
                };
                Some((
                    rank,
                    order,
                    Value {
                        label: server.name.clone(),
                        meta: format!(
                            "{state} · {} · {}",
                            server.transport.as_str(),
                            format::tools(server.tool_count)
                        ),
                        completion: format!("{}{}", slot.prefix, server.name),
                        pick: Pick::Server(server.id),
                    },
                ))
            })
            .collect(),
    )
}

fn theme_values(slot: &ArgSlot) -> Vec<Value> {
    ranked(
        CliTheme::ALL
            .iter()
            .enumerate()
            .filter_map(|(order, theme)| {
                let label = overlay::theme_label(*theme);
                let rank = score(&label.to_lowercase(), &slot.query)
                    .or_else(|| score(theme.as_str(), &slot.query))?;
                Some((
                    rank,
                    order,
                    Value {
                        label: label.to_owned(),
                        meta: overlay::theme_hint(*theme).to_owned(),
                        completion: format!("{}{}", slot.prefix, theme.as_str()),
                        pick: Pick::Theme(*theme),
                    },
                ))
            })
            .collect(),
    )
}

fn conversation_values(
    slot: &ArgSlot,
    conversations: &[ConversationDto],
    folder: &str,
) -> Vec<Value> {
    ranked(
        overlay::in_folder(conversations, folder)
            .into_iter()
            .enumerate()
            .filter_map(|(order, conversation)| {
                let rank = score(&conversation.title.to_lowercase(), &slot.query)?;
                Some((
                    rank,
                    order,
                    Value {
                        label: conversation.title.clone(),
                        meta: format!(
                            "{} · {}",
                            format::date(conversation.updated_at),
                            conversation.model_id
                        ),
                        completion: format!("{}{}", slot.prefix, conversation.title),
                        pick: Pick::Conversation(conversation.id),
                    },
                ))
            })
            .collect(),
    )
}

pub fn values(slot: &ArgSlot, sources: &Sources<'_>) -> Option<Vec<Value>> {
    match slot.arg {
        Arg::Server => sources.servers.map(|servers| server_values(slot, servers)),
        Arg::Theme => Some(theme_values(slot)),
        Arg::Conversation => sources
            .conversations
            .map(|conversations| conversation_values(slot, conversations, sources.folder)),
    }
}

pub fn loading(text: &str, sources: &Sources<'_>) -> bool {
    arg_slot(text).is_some_and(|slot| values(&slot, sources).is_none())
}

pub fn matches(text: &str, sources: &Sources<'_>) -> Vec<Item> {
    if let Some(slot) = arg_slot(text) {
        return values(&slot, sources)
            .unwrap_or_default()
            .into_iter()
            .map(Item::Value)
            .collect();
    }
    if let Some((command, partial)) = sub_query(text) {
        return subs_of(command)
            .filter(|sub| sub.name().starts_with(&partial))
            .map(Item::Sub)
            .collect();
    }
    if !is_command_text(text) {
        return Vec::new();
    }
    let query = text[1..].to_lowercase();
    let mut found: Vec<(usize, usize, Command)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(order, command)| {
            score(command.name(), &query).map(|rank| (rank, order, *command))
        })
        .collect();
    found.sort_unstable_by_key(|(rank, order, _)| (*rank, *order));
    found
        .into_iter()
        .map(|(_, _, command)| Item::Command(command))
        .collect()
}

impl Palette {
    pub const fn selected(&self) -> usize {
        self.selected
    }

    pub fn reset(&mut self) {
        self.selected = 0;
    }

    pub fn current(&self, text: &str, sources: &Sources<'_>) -> Option<Item> {
        let found = matches(text, sources);
        let index = self.selected.min(found.len().saturating_sub(1));
        found.into_iter().nth(index)
    }

    pub fn up(&mut self, text: &str, sources: &Sources<'_>) {
        let count = matches(text, sources).len();
        if count > 0 {
            self.selected = (self.selected + count - 1) % count;
        }
    }

    pub fn down(&mut self, text: &str, sources: &Sources<'_>) {
        let count = matches(text, sources).len();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value as Json;

    use super::*;

    const MCP: Item = Item::Command(Command::Mcp);
    const ADD: Item = Item::Sub(Sub::McpAdd);
    const INSPECT: Item = Item::Sub(Sub::McpInspect);
    const NONE: Sources<'static> = Sources {
        servers: None,
        conversations: None,
        folder: "",
    };

    fn matches(text: &str) -> Vec<Item> {
        super::matches(text, &NONE)
    }

    fn commands(text: &str) -> Vec<Command> {
        matches(text)
            .into_iter()
            .filter_map(|item| match item {
                Item::Command(command) => Some(command),
                _ => None,
            })
            .collect()
    }

    fn servers() -> Vec<McpServerDto> {
        let cases: Vec<Json> = serde_json::from_str(include_str!(
            "../../../../packages/shared/fixtures/mcp/McpServerDto.valid.json"
        ))
        .expect("fixture");
        let base: McpServerDto = serde_json::from_value(cases[0]["value"].clone()).expect("server");
        ["Docs", "Qdrant", "Files"]
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mut server = base.clone();
                server.name = (*name).to_owned();
                server.id = McpServerId::parse(&format!("0199aa22-2222-7222-8222-{index:012}"))
                    .expect("id");
                server
            })
            .collect()
    }

    #[test]
    fn a_slash_lists_every_command_in_order_without_subcommands() {
        assert_eq!(commands("/"), COMMANDS);
        assert_eq!(matches("/").len(), COMMANDS.len());
        assert_eq!(matches("/mcp"), [MCP]);
        assert!(matches("hello").is_empty());
        assert!(matches("/models gpt").is_empty());
    }

    #[test]
    fn prefixes_rank_before_substrings_and_subsequences() {
        assert_eq!(
            commands("/m"),
            [
                Command::Models,
                Command::Mcp,
                Command::Resume,
                Command::Permissions,
                Command::Theme
            ]
        );
        assert_eq!(commands("/con"), [Command::Connect, Command::Context]);
        assert_eq!(matches("/desk"), [Item::Command(Command::Desktop)]);
        assert_eq!(matches("/rsm"), [Item::Command(Command::Resume)]);
        assert!(matches("/zzz").is_empty());
    }

    #[test]
    fn a_command_with_subcommands_offers_them_after_a_space() {
        assert_eq!(matches("/mcp "), [ADD, INSPECT]);
        assert_eq!(matches("/MCP in"), [INSPECT]);
        assert!(matches("/mcp x").is_empty());
        assert!(matches("/mcp add docs").is_empty());
        assert!(is_command_text("/mcp a"));
        assert!(!is_command_text("/models a"));
        assert_eq!(ADD.completion(), "/mcp add ");
        assert_eq!(MCP.completion(), "/mcp");
        assert_eq!(INSPECT.label(), "mcp inspect");
    }

    #[test]
    fn server_names_complete_after_inspect_once_they_are_loaded() {
        assert!(loading("/mcp inspect ", &NONE));
        assert!(matches("/mcp inspect ").is_empty());
        let list = servers();
        let sources = Sources {
            servers: Some(&list),
            ..NONE
        };
        let found = super::matches("/mcp inspect ", &sources);
        assert_eq!(found.len(), 3);
        let found = super::matches("/mcp inspect  qd", &sources);
        let [Item::Value(value)] = found.as_slice() else {
            panic!("one server: {found:?}");
        };
        assert_eq!(value.label, "Qdrant");
        assert_eq!(value.completion, "/mcp inspect Qdrant");
        assert_eq!(value.pick, Pick::Server(list[1].id));
        assert!(value.meta.contains("stdio"));
        assert!(!loading("/mcp inspect qd", &sources));
        assert!(arg_slot("/mcp add docs").is_none());
    }

    #[test]
    fn themes_complete_by_name_or_id_without_loading() {
        let found = matches("/theme ");
        assert_eq!(found.len(), CliTheme::ALL.len());
        let found = matches("/theme moss");
        let [Item::Value(value)] = found.as_slice() else {
            panic!("one theme: {found:?}");
        };
        assert_eq!(value.pick, Pick::Theme(CliTheme::Moss));
        assert_eq!(value.completion, "/theme moss");
    }

    #[test]
    fn the_selection_wraps_and_tracks_the_filtered_list() {
        let mut palette = Palette::default();
        palette.up("/m", &NONE);
        assert_eq!(
            palette.current("/m", &NONE),
            Some(Item::Command(Command::Theme))
        );
        palette.down("/m", &NONE);
        assert_eq!(
            palette.current("/m", &NONE),
            Some(Item::Command(Command::Models))
        );
        palette.down("/m", &NONE);
        assert_eq!(palette.current("/m", &NONE), Some(MCP));
        assert_eq!(commands("/mo"), [Command::Models, Command::Permissions]);
        assert_eq!(
            palette.current("/mo", &NONE),
            Some(Item::Command(Command::Permissions))
        );
    }

    #[test]
    fn typed_commands_parse_with_arguments_and_any_case() {
        assert_eq!(Command::parse("/EXIT"), Some(Command::Exit));
        assert_eq!(Command::parse(" /resume abc "), Some(Command::Resume));
        assert_eq!(Command::parse("/nope"), None);
        assert_eq!(Command::parse("exit"), None);
        assert_eq!(arguments("/mcp  add docs x"), "add docs x");
    }
}
