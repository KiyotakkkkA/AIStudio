use crate::data::schema::{GRANTED_BY_CLI, GRANTED_BY_USER};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Caller {
    Studio,
    Cli { connection: u64 },
    BrowserUi,
    Panel,
}

impl Caller {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Studio => "studio",
            Self::Cli { .. } => "cli",
            Self::BrowserUi => "browser-ui",
            Self::Panel => "panel",
        }
    }

    pub const fn connection(self) -> Option<u64> {
        match self {
            Self::Cli { connection } => Some(connection),
            _ => None,
        }
    }
}

pub const fn granted_by(caller: Caller) -> &'static str {
    match caller {
        Caller::Cli { .. } => GRANTED_BY_CLI,
        _ => GRANTED_BY_USER,
    }
}
