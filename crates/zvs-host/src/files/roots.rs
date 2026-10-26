use std::path::{Path, PathBuf};

use zvs_contract::{AppError, AppErrorCode};

use crate::artifacts::ArtifactStore;
use crate::chat_files::chat_dir;
use crate::skills::SkillLibrary;

pub const CHAT_ROOT: &str = "chat:";
pub const SKILL_ROOT: &str = "skill:";
pub const ROOT_PATH_MAX: usize = 512;

pub const BAD_ROOT_PATH: &str =
    "Путь должен оставаться внутри папки: без «..», скрытых имён и недопустимых символов";
pub const SKILL_KEY_MISSING: &str =
    "После skill: указывается идентификатор навыка, например skill:dashboard/references/spec.md";
pub const SKILL_READ_ONLY: &str = "Папка навыка доступна только для чтения";
pub const ROOTS_UNAVAILABLE: &str = "Пути chat: и skill: недоступны в этом запуске";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    Chat,
    Skill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRoot<'a> {
    Chat(&'a str),
    Skill { key: &'a str, rest: &'a str },
    Real(&'a str),
}

impl PathRoot<'_> {
    pub const fn kind(&self) -> Option<RootKind> {
        match self {
            Self::Chat(_) => Some(RootKind::Chat),
            Self::Skill { .. } => Some(RootKind::Skill),
            Self::Real(_) => None,
        }
    }
}

const SEPARATORS: [char; 2] = ['/', '\\'];

pub fn path_root(path: &str) -> PathRoot<'_> {
    let trimmed = path.trim();
    if let Some(rest) = trimmed.strip_prefix(CHAT_ROOT) {
        return PathRoot::Chat(rest.trim_start_matches(SEPARATORS));
    }
    if let Some(rest) = trimmed.strip_prefix(SKILL_ROOT) {
        let rest = rest.trim_start_matches(SEPARATORS);
        let (key, rest) = rest.split_once(SEPARATORS).unwrap_or((rest, ""));
        return PathRoot::Skill { key, rest };
    }
    PathRoot::Real(path)
}

pub fn root_kind(path: &str) -> Option<RootKind> {
    path_root(path).kind()
}

pub fn is_virtual(path: &str) -> bool {
    root_kind(path).is_some()
}

fn bad_path() -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, BAD_ROOT_PATH)
}

fn safe_segment(segment: &str) -> bool {
    segment != "."
        && segment != ".."
        && !segment.starts_with('.')
        && !segment.ends_with(['.', ' '])
        && !segment
            .chars()
            .any(|character| character.is_control() || "<>\"|?*:".contains(character))
}

pub fn under(base: &Path, rest: &str) -> Result<PathBuf, AppError> {
    let text = rest.trim().replace('\\', "/");
    if text.chars().count() > ROOT_PATH_MAX {
        return Err(bad_path());
    }
    let mut path = base.to_path_buf();
    for segment in text.split('/').filter(|segment| !segment.is_empty()) {
        if !safe_segment(segment) {
            return Err(bad_path());
        }
        path.push(segment);
    }
    Ok(path)
}

fn contained(base: &Path, path: &Path) -> Result<(), AppError> {
    let Ok(real) = path.canonicalize() else {
        return Ok(());
    };
    match base.canonicalize() {
        Ok(base) if real.starts_with(&base) => Ok(()),
        _ => Err(bad_path()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub kind: RootKind,
    pub base: PathBuf,
    pub path: PathBuf,
}

impl Located {
    pub fn is_base(&self) -> bool {
        self.path == self.base
    }
}

#[derive(Clone, Default)]
pub struct VirtualRoots {
    chats: Option<ArtifactStore>,
    skills: Option<SkillLibrary>,
}

impl VirtualRoots {
    pub const fn new(chats: Option<ArtifactStore>, skills: Option<SkillLibrary>) -> Self {
        Self { chats, skills }
    }

    pub const fn chats(&self) -> Option<&ArtifactStore> {
        self.chats.as_ref()
    }

    pub const fn skills(&self) -> Option<&SkillLibrary> {
        self.skills.as_ref()
    }

    pub fn chat_folder(&self, conversation: &str) -> Result<PathBuf, AppError> {
        let store = self
            .chats
            .as_ref()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, ROOTS_UNAVAILABLE))?;
        chat_dir(store, conversation)
    }

    pub fn skill_folder(&self, key: &str) -> Result<PathBuf, AppError> {
        let library = self
            .skills
            .as_ref()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, ROOTS_UNAVAILABLE))?;
        if key.trim().is_empty() {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                SKILL_KEY_MISSING,
            ));
        }
        library.folder(key)
    }

    pub fn locate(&self, conversation: &str, path: &str) -> Result<Option<Located>, AppError> {
        let (kind, base, rest) = match path_root(path) {
            PathRoot::Real(_) => return Ok(None),
            PathRoot::Chat(rest) => (RootKind::Chat, self.chat_folder(conversation)?, rest),
            PathRoot::Skill { key, rest } => (RootKind::Skill, self.skill_folder(key)?, rest),
        };
        let path = under(&base, rest)?;
        contained(&base, &path)?;
        Ok(Some(Located { kind, base, path }))
    }
}

#[cfg(test)]
mod tests;
