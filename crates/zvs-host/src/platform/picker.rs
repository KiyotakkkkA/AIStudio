use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{AppError, VectorSourceKind};

#[async_trait]
pub trait PickerPort: Send + Sync {
    async fn pick(&self, kind: VectorSourceKind) -> Result<Vec<String>, AppError>;
}

pub type SharedPicker = Arc<dyn PickerPort>;
