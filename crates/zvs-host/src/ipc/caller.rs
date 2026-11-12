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
