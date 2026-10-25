use std::path::Path;

pub use crate::toolchain::process::{SCRATCH_KEEP, prepare_scratch};

pub const SCRIPT_UNSUPPORTED: &str =
    "Скрипт навыка должен иметь расширение .py, .js, .mjs или .cjs";
pub const SCRIPT_NO_TOOLCHAIN: &str = "Среда выполнения скриптов недоступна";
pub const SCRIPT_NOT_STARTED: &str = "Не удалось запустить скрипт навыка";

pub const SKILL_DIR_ENV: &str = "ZVS_SKILL_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptRuntime {
    Python,
    Node,
}

impl ScriptRuntime {
    pub fn of(path: &str) -> Option<Self> {
        let extension = Path::new(path)
            .extension()?
            .to_string_lossy()
            .to_ascii_lowercase();
        match extension.as_str() {
            "py" => Some(Self::Python),
            "js" | "mjs" | "cjs" => Some(Self::Node),
            _ => None,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Python => "uv",
            Self::Node => "node",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Python => "uv",
            Self::Node => "Node.js",
        }
    }

    pub fn args(self, script: &str, extra: &[String]) -> Vec<String> {
        let mut args = match self {
            Self::Python => vec!["run".to_owned(), script.to_owned()],
            Self::Node => vec![script.to_owned()],
        };
        args.extend(extra.iter().cloned());
        args
    }
}

#[cfg(test)]
mod tests;
