use crate::copy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Help,
    New,
    Resume,
    Models,
    Effort,
    Copilot,
    Context,
    Skills,
    Mcp,
    Permissions,
    Shortcuts,
    Clear,
    Exit,
}

pub const COMMANDS: &[Command] = &[
    Command::Help,
    Command::Models,
    Command::Effort,
    Command::Copilot,
    Command::Resume,
    Command::New,
    Command::Context,
    Command::Skills,
    Command::Mcp,
    Command::Permissions,
    Command::Shortcuts,
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
            Self::Copilot => "copilot",
            Self::Context => "context",
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Permissions => "permissions",
            Self::Shortcuts => "shortcuts",
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
            Self::Copilot => copy::CMD_COPILOT,
            Self::Context => copy::CMD_CONTEXT,
            Self::Skills => copy::CMD_SKILLS,
            Self::Mcp => copy::CMD_MCP,
            Self::Permissions => copy::CMD_PERMISSIONS,
            Self::Shortcuts => copy::CMD_SHORTCUTS,
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

pub fn is_command_text(text: &str) -> bool {
    text.starts_with('/') && !text.contains(char::is_whitespace)
}

fn score(command: Command, query: &str) -> Option<usize> {
    let name = command.name();
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

pub fn matches(text: &str) -> Vec<Command> {
    if !is_command_text(text) {
        return Vec::new();
    }
    let query = text[1..].to_lowercase();
    let mut found: Vec<(usize, usize, Command)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(order, command)| score(*command, &query).map(|rank| (rank, order, *command)))
        .collect();
    found.sort_unstable_by_key(|(rank, order, _)| (*rank, *order));
    found.into_iter().map(|(_, _, command)| command).collect()
}

impl Palette {
    pub const fn selected(&self) -> usize {
        self.selected
    }

    pub fn reset(&mut self) {
        self.selected = 0;
    }

    pub fn current(&self, text: &str) -> Option<Command> {
        let found = matches(text);
        found
            .get(self.selected.min(found.len().saturating_sub(1)))
            .copied()
    }

    pub fn up(&mut self, text: &str) {
        let count = matches(text).len();
        if count > 0 {
            self.selected = (self.selected + count - 1) % count;
        }
    }

    pub fn down(&mut self, text: &str) {
        let count = matches(text).len();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slash_lists_every_command_in_order() {
        assert_eq!(matches("/"), COMMANDS);
        assert!(matches("hello").is_empty());
        assert!(matches("/models gpt").is_empty());
    }

    #[test]
    fn prefixes_rank_before_substrings_and_subsequences() {
        assert_eq!(
            matches("/m"),
            [
                Command::Models,
                Command::Mcp,
                Command::Resume,
                Command::Permissions
            ]
        );
        assert_eq!(matches("/con"), [Command::Context]);
        assert_eq!(matches("/rsm"), [Command::Resume]);
        assert!(matches("/zzz").is_empty());
    }

    #[test]
    fn the_selection_wraps_and_tracks_the_filtered_list() {
        let mut palette = Palette::default();
        palette.up("/m");
        assert_eq!(palette.current("/m"), Some(Command::Permissions));
        palette.down("/m");
        assert_eq!(palette.current("/m"), Some(Command::Models));
        palette.down("/m");
        palette.down("/m");
        assert_eq!(palette.current("/m"), Some(Command::Resume));
        assert_eq!(matches("/mo"), [Command::Models, Command::Permissions]);
        assert_eq!(palette.current("/mo"), Some(Command::Permissions));
    }

    #[test]
    fn typed_commands_parse_with_arguments_and_any_case() {
        assert_eq!(Command::parse("/EXIT"), Some(Command::Exit));
        assert_eq!(Command::parse(" /resume abc "), Some(Command::Resume));
        assert_eq!(Command::parse("/nope"), None);
        assert_eq!(Command::parse("exit"), None);
    }
}
