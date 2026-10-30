use serde_json::{Map, Value, json};

use super::palette::Command;
use crate::copy;

pub const KEYMAP_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordKey {
    Char(char),
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: ChordKey,
}

const RUSSIAN: [(char, char); 33] = [
    ('й', 'q'),
    ('ц', 'w'),
    ('у', 'e'),
    ('к', 'r'),
    ('е', 't'),
    ('н', 'y'),
    ('г', 'u'),
    ('ш', 'i'),
    ('щ', 'o'),
    ('з', 'p'),
    ('х', '['),
    ('ъ', ']'),
    ('ф', 'a'),
    ('ы', 's'),
    ('в', 'd'),
    ('а', 'f'),
    ('п', 'g'),
    ('р', 'h'),
    ('о', 'j'),
    ('л', 'k'),
    ('д', 'l'),
    ('ж', ';'),
    ('э', '\''),
    ('я', 'z'),
    ('ч', 'x'),
    ('с', 'c'),
    ('м', 'v'),
    ('и', 'b'),
    ('т', 'n'),
    ('ь', 'm'),
    ('б', ','),
    ('ю', '.'),
    ('ё', '`'),
];

fn latin(value: char) -> char {
    let lower = value.to_lowercase().next().unwrap_or(value);
    RUSSIAN
        .iter()
        .find(|(cyrillic, _)| *cyrillic == lower)
        .map_or(lower, |(_, mapped)| *mapped)
}

impl Chord {
    pub const fn ctrl(value: char) -> Self {
        Self {
            ctrl: true,
            alt: false,
            shift: false,
            key: ChordKey::Char(value),
        }
    }

    pub const fn alt(value: char) -> Self {
        Self {
            ctrl: false,
            alt: true,
            shift: false,
            key: ChordKey::Char(value),
        }
    }

    pub const fn function(number: u8) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            key: ChordKey::F(number),
        }
    }

    pub fn character(value: char, ctrl: bool, alt: bool, shift: bool) -> Option<Self> {
        if ctrl == alt || value.is_control() || value.is_whitespace() {
            return None;
        }
        Some(Self {
            ctrl,
            alt,
            shift,
            key: ChordKey::Char(latin(value)),
        })
    }

    pub const fn function_key(number: u8, ctrl: bool, alt: bool, shift: bool) -> Option<Self> {
        if number == 0 || number > 12 {
            return None;
        }
        Some(Self {
            ctrl,
            alt,
            shift,
            key: ChordKey::F(number),
        })
    }

    pub fn label(self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        parts.push(match self.key {
            ChordKey::Char(value) => value.to_uppercase().collect(),
            ChordKey::F(number) => format!("F{number}"),
        });
        parts.join("+")
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                other if key.is_none() => key = Some(other.to_owned()),
                _ => return None,
            }
        }
        let key = key?;
        if let Some(number) = key.strip_prefix('f').filter(|digits| !digits.is_empty())
            && let Ok(number) = number.parse::<u8>()
        {
            return Self::function_key(number, ctrl, alt, shift);
        }
        let mut chars = key.chars();
        let value = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        Self::character(value, ctrl, alt, shift).filter(|chord| !chord.reserved())
    }

    pub const fn reserved(self) -> bool {
        matches!(
            self,
            Self {
                ctrl: true,
                alt: false,
                key: ChordKey::Char('c' | 'd' | 'h' | 'i' | 'j' | 'm' | '['),
                ..
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shortcut {
    Details,
    ClearInput,
    NewChat,
    ClearScreen,
    Models,
    Resume,
    Context,
    Skills,
    Mcp,
    Permissions,
    Help,
    Shortcuts,
}

pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut::Details,
    Shortcut::ClearInput,
    Shortcut::NewChat,
    Shortcut::ClearScreen,
    Shortcut::Models,
    Shortcut::Resume,
    Shortcut::Context,
    Shortcut::Skills,
    Shortcut::Mcp,
    Shortcut::Permissions,
    Shortcut::Help,
    Shortcut::Shortcuts,
];

impl Shortcut {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Details => "details",
            Self::ClearInput => "clearInput",
            Self::NewChat => "newChat",
            Self::ClearScreen => "clearScreen",
            Self::Models => "models",
            Self::Resume => "resume",
            Self::Context => "context",
            Self::Skills => "skills",
            Self::Mcp => "mcp",
            Self::Permissions => "permissions",
            Self::Help => "help",
            Self::Shortcuts => "shortcuts",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Details => copy::SHORTCUT_DETAILS,
            Self::ClearInput => copy::SHORTCUT_CLEAR_INPUT,
            Self::NewChat => copy::CMD_NEW,
            Self::ClearScreen => copy::CMD_CLEAR,
            Self::Models => copy::CMD_MODELS,
            Self::Resume => copy::CMD_RESUME,
            Self::Context => copy::CMD_CONTEXT,
            Self::Skills => copy::CMD_SKILLS,
            Self::Mcp => copy::CMD_MCP,
            Self::Permissions => copy::CMD_PERMISSIONS,
            Self::Help => copy::CMD_HELP,
            Self::Shortcuts => copy::CMD_SHORTCUTS,
        }
    }

    pub const fn default_chord(self) -> Option<Chord> {
        Some(match self {
            Self::Details => Chord::ctrl('o'),
            Self::ClearInput => Chord::ctrl('u'),
            Self::NewChat => Chord::ctrl('n'),
            Self::ClearScreen => Chord::ctrl('l'),
            Self::Models => Chord::ctrl('p'),
            Self::Resume => Chord::ctrl('r'),
            Self::Context => Chord::ctrl('k'),
            Self::Skills => Chord::alt('s'),
            Self::Mcp => Chord::alt('m'),
            Self::Permissions => Chord::alt('p'),
            Self::Help => Chord::function(1),
            Self::Shortcuts => Chord::alt('k'),
        })
    }

    pub const fn command(self) -> Option<Command> {
        match self {
            Self::Details | Self::ClearInput => None,
            Self::NewChat => Some(Command::New),
            Self::ClearScreen => Some(Command::Clear),
            Self::Models => Some(Command::Models),
            Self::Resume => Some(Command::Resume),
            Self::Context => Some(Command::Context),
            Self::Skills => Some(Command::Skills),
            Self::Mcp => Some(Command::Mcp),
            Self::Permissions => Some(Command::Permissions),
            Self::Help => Some(Command::Help),
            Self::Shortcuts => Some(Command::Shortcuts),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        SHORTCUTS
            .iter()
            .copied()
            .find(|shortcut| shortcut.id() == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Shortcut, Option<Chord>)>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: SHORTCUTS
                .iter()
                .map(|shortcut| (*shortcut, shortcut.default_chord()))
                .collect(),
        }
    }
}

impl Keymap {
    pub fn chord(&self, shortcut: Shortcut) -> Option<Chord> {
        self.bindings
            .iter()
            .find(|(known, _)| *known == shortcut)
            .and_then(|(_, chord)| *chord)
    }

    pub fn action(&self, chord: Chord) -> Option<Shortcut> {
        self.bindings
            .iter()
            .find(|(_, bound)| *bound == Some(chord))
            .map(|(shortcut, _)| *shortcut)
    }

    pub fn label(&self, shortcut: Shortcut) -> String {
        self.chord(shortcut)
            .map_or_else(|| copy::SHORTCUT_NONE.to_owned(), Chord::label)
    }

    pub fn bind(&mut self, shortcut: Shortcut, chord: Chord) -> Option<Shortcut> {
        let displaced = self.action(chord).filter(|other| *other != shortcut);
        for (known, bound) in &mut self.bindings {
            if *known == shortcut {
                *bound = Some(chord);
            } else if *bound == Some(chord) {
                *bound = None;
            }
        }
        displaced
    }

    pub fn unbind(&mut self, shortcut: Shortcut) {
        for (known, bound) in &mut self.bindings {
            if *known == shortcut {
                *bound = None;
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn to_json(&self) -> Value {
        let bindings: Map<String, Value> = self
            .bindings
            .iter()
            .map(|(shortcut, chord)| {
                (
                    shortcut.id().to_owned(),
                    chord.map_or(Value::Null, |chord| json!(chord.label())),
                )
            })
            .collect();
        json!({ "version": KEYMAP_VERSION, "bindings": bindings })
    }

    pub fn from_json(value: &Value) -> Self {
        let mut keymap = Self::default();
        let Some(bindings) = value.get("bindings").and_then(Value::as_object) else {
            return keymap;
        };
        for (id, raw) in bindings {
            let Some(shortcut) = Shortcut::from_id(id) else {
                continue;
            };
            match raw {
                Value::Null => keymap.unbind(shortcut),
                Value::String(text) => {
                    if let Some(chord) = Chord::parse(text) {
                        keymap.bind(shortcut, chord);
                    }
                }
                _ => {}
            }
        }
        keymap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip_through_the_parser() {
        for chord in [
            Chord::ctrl('o'),
            Chord::alt('s'),
            Chord::function(5),
            Chord::character('n', true, false, true).expect("a chord"),
        ] {
            assert_eq!(
                Chord::parse(&chord.label()),
                Some(chord),
                "{}",
                chord.label()
            );
        }
        assert_eq!(Chord::ctrl('o').label(), "Ctrl+O");
        assert_eq!(Chord::function(1).label(), "F1");
        assert_eq!(
            Chord::parse("ctrl + shift + n")
                .map(Chord::label)
                .as_deref(),
            Some("Ctrl+Shift+N")
        );
        assert!(Chord::parse("Ctrl+C").is_none());
        assert!(Chord::parse("Ctrl").is_none());
        assert!(Chord::parse("Ctrl+O+P").is_none());
        assert!(Chord::parse("F13").is_none());
    }

    #[test]
    fn russian_letters_map_to_their_latin_keys_and_altgr_types_text() {
        assert_eq!(
            Chord::character('т', true, false, false),
            Some(Chord::ctrl('n'))
        );
        assert_eq!(
            Chord::character('Ы', false, true, true).map(|chord| chord.key),
            Some(ChordKey::Char('s'))
        );
        assert!(Chord::character('@', true, true, false).is_none());
        assert!(Chord::character('a', false, false, false).is_none());
    }

    #[test]
    fn defaults_are_unique_and_never_reserved() {
        let keymap = Keymap::default();
        for shortcut in SHORTCUTS {
            let chord = keymap.chord(*shortcut).expect("bound by default");
            assert!(!chord.reserved(), "{}", chord.label());
            assert_eq!(keymap.action(chord), Some(*shortcut));
        }
    }

    #[test]
    fn binding_a_taken_chord_moves_it_and_reports_the_previous_owner() {
        let mut keymap = Keymap::default();
        assert_eq!(
            keymap.bind(Shortcut::Models, Chord::ctrl('o')),
            Some(Shortcut::Details)
        );
        assert_eq!(keymap.action(Chord::ctrl('o')), Some(Shortcut::Models));
        assert_eq!(keymap.chord(Shortcut::Details), None);
        assert_eq!(keymap.label(Shortcut::Details), copy::SHORTCUT_NONE);
        assert_eq!(keymap.bind(Shortcut::Models, Chord::ctrl('o')), None);
        keymap.reset();
        assert_eq!(keymap, Keymap::default());
    }

    #[test]
    fn the_stored_map_round_trips_and_tolerates_bad_entries() {
        let mut keymap = Keymap::default();
        keymap.bind(Shortcut::NewChat, Chord::alt('n'));
        keymap.unbind(Shortcut::Help);
        assert_eq!(Keymap::from_json(&keymap.to_json()), keymap);

        let loaded = Keymap::from_json(&json!({
            "bindings": {
                "newChat": "Ctrl+C",
                "unknown": "Alt+X",
                "models": 7,
                "skills": "Ctrl+O"
            }
        }));
        assert_eq!(loaded.chord(Shortcut::NewChat), Some(Chord::ctrl('n')));
        assert_eq!(loaded.chord(Shortcut::Models), Some(Chord::ctrl('p')));
        assert_eq!(loaded.chord(Shortcut::Skills), Some(Chord::ctrl('o')));
        assert_eq!(loaded.chord(Shortcut::Details), None);
        assert_eq!(Keymap::from_json(&json!("garbage")), Keymap::default());
    }
}
