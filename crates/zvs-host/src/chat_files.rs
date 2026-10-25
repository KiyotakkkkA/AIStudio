use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zvs_contract::validate::check;
use zvs_contract::{AppError, AppErrorCode, Validate};

use crate::artifacts::ArtifactStore;
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, StepContext};
use crate::platform::long_path::extended;

pub const CHAT_FILE: &str = "chat.file";
pub const CHAT_FOLDERS: &str = "chats";
pub const CHAT_DIR_ENV: &str = "ZVS_CHAT_DIR";
pub const WORKING_PREFIX: char = '_';
pub const CHAT_FILE_MAX_BYTES: usize = 1024 * 1024;
pub const CHAT_FOLDER_MAX_BYTES: u64 = 200 * 1024 * 1024;
pub const CHAT_FOLDER_MAX_FILES: usize = 1000;
const LIST_MAX: usize = 500;
const PATH_MAX: usize = 240;

pub const CHAT_FILE_DESCRIPTION: &str = "Рабочая папка диалога: запись, чтение и список текстовых файлов без запроса разрешения. Скрипты навыков запускаются в этой папке, поэтому относительный путь в их аргументах, например _spec.json, указывает сюда же. Файлы и папки, имя которых начинается с «_», — рабочие: пользователю они не показываются. Остальные файлы, которые скрипты навыков создают в этой папке, показываются пользователю как результаты. Возвращает абсолютный путь файла.";

pub const NO_CHAT: &str = "Рабочая папка доступна только в диалоге или агентной сессии";
pub const BAD_CHAT_PATH: &str =
    "Путь должен быть относительным, внутри рабочей папки диалога, без «..» и скрытых имён";
pub const CHAT_FILE_TOO_LARGE: &str = "Файл больше 1 МиБ";
pub const CHAT_FOLDER_FULL: &str = "Рабочая папка диалога заполнена";
pub const CHAT_FILE_MISSING: &str = "Файл не найден в рабочей папке диалога";
pub const CHAT_FILE_BINARY: &str = "Файл не является текстом";
pub const CHAT_FILE_FAILED: &str = "Не удалось выполнить операцию с файлом рабочей папки";

fn failed(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, CHAT_FILE_FAILED).with_detail("raw", error.to_string())
}

fn is_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 64
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

pub fn is_working(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(component, Component::Normal(name) if name.to_string_lossy().starts_with(WORKING_PREFIX))
    })
}

pub fn chat_root(store: &ArtifactStore) -> PathBuf {
    store.root().join(CHAT_FOLDERS)
}

pub fn chat_dir(store: &ArtifactStore, owner: &str) -> Result<PathBuf, AppError> {
    if !is_owner(owner) {
        return Err(AppError::new(AppErrorCode::ValidationFailed, NO_CHAT));
    }
    let folder = chat_root(store).join(owner);
    std::fs::create_dir_all(extended(&folder)).map_err(|error| failed(&error))?;
    Ok(folder)
}

pub fn remove_chat_dir(store: &ArtifactStore, owner: &str) {
    if is_owner(owner) {
        let _ = std::fs::remove_dir_all(extended(&chat_root(store).join(owner)));
    }
}

pub fn chat_path(folder: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let bad = || AppError::new(AppErrorCode::ValidationFailed, BAD_CHAT_PATH);
    let text = relative.trim().replace('\\', "/");
    if text.is_empty() || text.chars().count() > PATH_MAX || text.contains(':') {
        return Err(bad());
    }
    let mut path = folder.to_path_buf();
    for segment in text.split('/').filter(|segment| !segment.is_empty()) {
        if segment == "."
            || segment == ".."
            || segment.starts_with('.')
            || segment.ends_with(['.', ' '])
            || segment
                .chars()
                .any(|character| character.is_control() || "<>\"|?*".contains(character))
        {
            return Err(bad());
        }
        path.push(segment);
    }
    if path == folder {
        return Err(bad());
    }
    Ok(path)
}

fn walk(folder: &Path, base: &Path, out: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(extended(folder)) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = folder.join(entry.file_name());
        if kind.is_dir() {
            walk(&path, base, out);
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(base)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let size = entry.metadata().map_or(0, |metadata| metadata.len());
            out.push((relative, size));
        }
    }
}

pub fn chat_files(folder: &Path) -> Vec<(String, u64)> {
    let mut files = Vec::new();
    walk(folder, folder, &mut files);
    files
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatFileAction {
    Write,
    Read,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatFileInput {
    #[schemars(
        description = "write — записать текстовый файл целиком, read — прочитать файл, list — перечислить файлы рабочей папки"
    )]
    pub action: ChatFileAction,
    #[serde(default)]
    #[schemars(
        description = "Относительный путь в рабочей папке, например _spec.json или data/_kpi.csv; для list не нужен"
    )]
    pub path: Option<String>,
    #[serde(default)]
    #[schemars(description = "Содержимое файла для write")]
    pub content: Option<String>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
}

impl Validate for ChatFileInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::max_len("path", path, PATH_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatFileEntry {
    pub path: String,
    pub size: u64,
    pub working: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatFileOutput {
    pub folder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ChatFileEntry>,
}

impl Validate for ChatFileOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct ChatFileNode(pub ArtifactStore);

impl ChatFileNode {
    fn write(folder: &Path, path: &Path, content: &str) -> Result<u64, AppError> {
        if content.len() > CHAT_FILE_MAX_BYTES {
            return Err(AppError::new(
                AppErrorCode::LimitExceeded,
                CHAT_FILE_TOO_LARGE,
            ));
        }
        let files = chat_files(folder);
        let existing = std::fs::metadata(extended(path)).map_or(0, |metadata| metadata.len());
        let used = files
            .iter()
            .map(|(_, size)| size)
            .sum::<u64>()
            .saturating_sub(existing);
        let fresh = existing == 0 && !path.exists();
        if used + content.len() as u64 > CHAT_FOLDER_MAX_BYTES
            || (fresh && files.len() >= CHAT_FOLDER_MAX_FILES)
        {
            return Err(AppError::new(AppErrorCode::LimitExceeded, CHAT_FOLDER_FULL));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(extended(parent)).map_err(|error| failed(&error))?;
        }
        let temporary = path.with_extension(format!(
            "{}.partial",
            path.extension()
                .map(|extension| extension.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        std::fs::write(extended(&temporary), content).map_err(|error| failed(&error))?;
        std::fs::rename(extended(&temporary), extended(path)).map_err(|error| {
            let _ = std::fs::remove_file(extended(&temporary));
            failed(&error)
        })?;
        Ok(content.len() as u64)
    }

    fn read(path: &Path) -> Result<(u64, String), AppError> {
        let metadata = std::fs::metadata(extended(path))
            .ok()
            .filter(std::fs::Metadata::is_file)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CHAT_FILE_MISSING))?;
        if metadata.len() > CHAT_FILE_MAX_BYTES as u64 {
            return Err(AppError::new(
                AppErrorCode::LimitExceeded,
                CHAT_FILE_TOO_LARGE,
            ));
        }
        let bytes = std::fs::read(extended(path)).map_err(|error| failed(&error))?;
        let text = String::from_utf8(bytes)
            .ok()
            .filter(|text| !text.contains('\u{0}'))
            .ok_or_else(|| AppError::new(AppErrorCode::UnsupportedFormat, CHAT_FILE_BINARY))?;
        Ok((metadata.len(), text))
    }
}

#[async_trait]
impl NodeDef for ChatFileNode {
    type Input = ChatFileInput;
    type Output = ChatFileOutput;

    fn node_type(&self) -> &str {
        CHAT_FILE
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(CHAT_FILE)
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: ChatFileInput,
    ) -> Result<ChatFileOutput, AppError> {
        context.check_cancelled()?;
        let folder = chat_dir(&self.0, &input.conversation_id)?;
        let mut output = ChatFileOutput {
            folder: folder.to_string_lossy().into_owned(),
            path: None,
            size: None,
            content: None,
            files: Vec::new(),
        };
        if input.action == ChatFileAction::List {
            output.files = chat_files(&folder)
                .into_iter()
                .take(LIST_MAX)
                .map(|(path, size)| ChatFileEntry {
                    working: is_working(Path::new(&path)),
                    path,
                    size,
                })
                .collect();
            return Ok(output);
        }
        let path = chat_path(&folder, input.path.as_deref().unwrap_or_default())?;
        output.path = Some(path.to_string_lossy().into_owned());
        if input.action == ChatFileAction::Write {
            let content = input.content.unwrap_or_default();
            let target = path.clone();
            let base = folder.clone();
            let size = tokio::task::spawn_blocking(move || Self::write(&base, &target, &content))
                .await
                .map_err(|error| {
                    AppError::new(AppErrorCode::Unknown, CHAT_FILE_FAILED)
                        .with_detail("raw", error.to_string())
                })??;
            output.size = Some(size);
        } else {
            let (size, content) = Self::read(&path)?;
            output.size = Some(size);
            output.content = Some(content);
        }
        Ok(output)
    }
}

pub fn register_chat_file_node(
    nodes: &NodeRegistry,
    store: &ArtifactStore,
) -> Result<(), AppError> {
    nodes.register(ChatFileNode(store.clone()))
}

pub fn with_chat_files(keys: &mut Vec<String>) {
    if keys.iter().any(|key| key == crate::skills::SKILL_SCRIPT)
        && !keys.iter().any(|key| key == CHAT_FILE)
    {
        keys.push(CHAT_FILE.to_owned());
    }
}

#[cfg(test)]
mod tests;
