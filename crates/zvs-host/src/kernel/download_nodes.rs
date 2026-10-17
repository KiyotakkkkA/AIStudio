use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{AppError, AppErrorCode, DownloadFetchInput, DownloadReportDto, HostEventDraft};

use super::registry::NodeRegistry;
use super::types::{NodeDef, PermissionRequirement, StepContext};
use crate::downloads::DownloadProgress;

pub const DOWNLOADS_UNAVAILABLE: &str = "Сервис загрузок недоступен";

pub struct DownloadFetchNode;

#[async_trait]
impl NodeDef for DownloadFetchNode {
    type Input = DownloadFetchInput;
    type Output = DownloadReportDto;

    fn node_type(&self) -> &str {
        "download.fetch"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("download.fetch")
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: DownloadFetchInput,
    ) -> Result<DownloadReportDto, AppError> {
        context.check_cancelled()?;
        let Some(downloads) = &context.services.downloads else {
            return Err(AppError::new(AppErrorCode::Conflict, DOWNLOADS_UNAVAILABLE));
        };
        let progress = context.clone();
        downloads
            .execute(
                &input.download_id.to_string(),
                Some(context.cancel.clone()),
                Some(Arc::new(move |update: DownloadProgress| {
                    let _ = progress.emit(HostEventDraft::Progress {
                        done: update.done,
                        total: update.total,
                    });
                })),
            )
            .await
    }
}

pub fn register_download_nodes(registry: &NodeRegistry) -> Result<(), AppError> {
    registry.register(DownloadFetchNode)
}
