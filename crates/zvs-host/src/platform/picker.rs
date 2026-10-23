use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{AppError, AppErrorCode, VectorSourceKind};

pub const SAVE_DIALOG_UNAVAILABLE: &str = "Окно сохранения файла недоступно";

#[async_trait]
pub trait PickerPort: Send + Sync {
    async fn pick(&self, kind: VectorSourceKind) -> Result<Vec<String>, AppError>;

    async fn save_as(&self, _suggested_name: &str) -> Result<Option<String>, AppError> {
        Err(AppError::new(
            AppErrorCode::NotImplemented,
            SAVE_DIALOG_UNAVAILABLE,
        ))
    }
}

pub type SharedPicker = Arc<dyn PickerPort>;
