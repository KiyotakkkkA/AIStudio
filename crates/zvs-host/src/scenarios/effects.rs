use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};
use zvs_contract::{AppError, AppErrorCode};

use super::nodes::ScenarioEffects;
use crate::notifications::NotificationService;
use crate::services::ChatService;

pub const FILE_PATH_INVALID: &str =
    "Путь к файлу должен быть относительным и не выходить за папку результатов сценариев";
pub const FILE_WRITE_FAILED: &str = "Не удалось сохранить файл";
pub const SCENARIO_FILES_DIR: &str = "scenario-files";
pub const SCENARIO_NOTIFICATION_TAG: &str = "scenario";

pub struct HostScenarioEffects {
    pub chat: ChatService,
    pub notifications: Option<NotificationService>,
    pub files_dir: PathBuf,
}

pub fn output_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let candidate = Path::new(relative.trim());
    let safe = !relative.trim().is_empty()
        && candidate
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !safe {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            FILE_PATH_INVALID,
        ));
    }
    Ok(root.join(candidate))
}

impl ScenarioEffects for HostScenarioEffects {
    fn post_message(&self, conversation_id: &str, text: &str) -> Result<Value, AppError> {
        let message = self.chat.post_message(conversation_id, text)?;
        Ok(json!({ "conversationId": conversation_id, "messageId": message.id }))
    }

    fn create_conversation(&self, title: &str, text: &str) -> Result<Value, AppError> {
        let conversation = self.chat.start_conversation(title, text)?;
        Ok(json!({ "conversationId": conversation.id, "title": conversation.title }))
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), AppError> {
        if let Some(notifications) = &self.notifications {
            notifications.announce(SCENARIO_NOTIFICATION_TAG, title, body);
        }
        Ok(())
    }

    fn write_file(&self, path: &str, text: &str) -> Result<Value, AppError> {
        let target = output_path(&self.files_dir, path)?;
        let failed = |_| AppError::new(AppErrorCode::NativeError, FILE_WRITE_FAILED);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        std::fs::write(&target, text).map_err(failed)?;
        Ok(json!({ "path": target.to_string_lossy() }))
    }
}
