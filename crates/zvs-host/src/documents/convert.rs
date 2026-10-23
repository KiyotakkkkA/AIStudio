use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::oneshot;
use zvs_contract::validate::check;
use zvs_contract::{
    AppError, AppErrorCode, HostEvent, HostEventDraft, RunGraph, RunKind, RunOutcomeDto,
    RunOutcomeStatus, StartRunInput, Validate,
};

use super::kind::OfficeFormat;
use super::service::DocumentConverter;
use crate::drivers::sidecar::{JobProgress, JobRunOptions, SharedSidecarJobs};
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, RunObserver, StepContext};
use crate::services::RunService;
use crate::toolchain::ToolchainService;
use crate::workspace::canonical;

pub const DOCUMENT_CONVERT_NODE: &str = "document.convert";
pub const DOCUMENT_CONVERT_JOB: &str = "job.document.convert";
pub const CONVERT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TIMEOUT_MARGIN: Duration = Duration::from_secs(30);

pub const CONVERSION_TIMED_OUT: &str =
    "Преобразование документа не завершилось за отведённое время";
pub const CONVERSION_TARGET_REFUSED: &str =
    "Преобразованный документ можно сохранить только в кэш документов";
pub const CONVERSION_NOT_STARTED: &str = "Не удалось запустить преобразование документа";
pub const CONVERSION_CANCELLED: &str = "Преобразование документа отменено";

pub const CONVERSION_FAILED_PREFIX: &str = "Не удалось преобразовать документ";

pub fn conversion_failed(format: &str) -> String {
    format!("{CONVERSION_FAILED_PREFIX} {format} в PDF")
}

pub fn conversion_title(name: &str) -> String {
    format!("Преобразование «{name}» в PDF")
}

#[async_trait]
pub trait OfficeLocator: Send + Sync {
    async fn office_executable(&self) -> Option<PathBuf>;
}

pub type SharedOfficeLocator = Arc<dyn OfficeLocator>;

#[async_trait]
impl OfficeLocator for ToolchainService {
    async fn office_executable(&self) -> Option<PathBuf> {
        Self::office_executable(self).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentConvertInput {
    pub executable: String,
    pub source: String,
    pub target: String,
    pub format: String,
}

impl Validate for DocumentConvertInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("executable", &self.executable)?;
        check::non_empty("source", &self.source)?;
        check::non_empty("target", &self.target)?;
        check::non_empty("format", &self.format)?;
        check::max_len("format", &self.format, 8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentConvertOutput {
    pub bytes: u64,
}

impl Validate for DocumentConvertOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct DocumentConvertNode {
    jobs: SharedSidecarJobs,
    cache_dir: PathBuf,
    timeout: Duration,
}

impl DocumentConvertNode {
    pub fn new(jobs: SharedSidecarJobs, cache_dir: PathBuf, timeout: Duration) -> Self {
        Self {
            jobs,
            cache_dir,
            timeout,
        }
    }

    fn inside_cache(&self, target: &Path) -> bool {
        let Some(folder) = target.parent() else {
            return false;
        };
        match (canonical(folder), canonical(&self.cache_dir)) {
            (Ok(folder), Ok(cache)) => folder == cache,
            _ => false,
        }
    }
}

fn job_failure(error: AppError, format: &str) -> AppError {
    match error.code {
        AppErrorCode::RunCancelled => {
            AppError::new(AppErrorCode::RunCancelled, CONVERSION_CANCELLED)
        }
        AppErrorCode::LimitExceeded => {
            AppError::new(AppErrorCode::LimitExceeded, CONVERSION_TIMED_OUT)
        }
        AppErrorCode::SidecarUnavailable | AppErrorCode::Conflict => error,
        AppErrorCode::NotFound | AppErrorCode::NativeError => {
            AppError::new(AppErrorCode::UnsupportedFormat, conversion_failed(format))
                .with_detail("raw", error.message)
        }
        _ => AppError::new(AppErrorCode::UnsupportedFormat, conversion_failed(format))
            .with_detail("raw", error.message),
    }
}

#[async_trait]
impl NodeDef for DocumentConvertNode {
    type Input = DocumentConvertInput;
    type Output = DocumentConvertOutput;

    fn node_type(&self) -> &str {
        DOCUMENT_CONVERT_NODE
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: DocumentConvertInput,
    ) -> Result<DocumentConvertOutput, AppError> {
        context.check_cancelled()?;
        let target = PathBuf::from(&input.target);
        if !self.inside_cache(&target) {
            return Err(AppError::new(
                AppErrorCode::PermissionDenied,
                CONVERSION_TARGET_REFUSED,
            ));
        }
        let format = input.format.to_ascii_uppercase();
        let progress = context.clone();
        let job_cancel = context.cancel.child_token();
        let params = json!({
            "executable": input.executable,
            "source": input.source,
            "target": input.target,
            "timeoutMs": u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX),
        });
        let run = self.jobs.run(
            DOCUMENT_CONVERT_JOB,
            params,
            JobRunOptions {
                cancel: Some(job_cancel.clone()),
                on_progress: Some(Arc::new(move |update: JobProgress| {
                    let _ = progress.emit(HostEventDraft::Progress {
                        done: update.done,
                        total: update.total,
                    });
                })),
            },
        );
        tokio::pin!(run);
        let deadline = tokio::time::sleep(self.timeout + TIMEOUT_MARGIN.min(self.timeout));
        let result = tokio::select! {
            result = &mut run => result,
            () = deadline => {
                job_cancel.cancel();
                let _ = run.await;
                let _ = std::fs::remove_file(&target);
                return Err(AppError::new(
                    AppErrorCode::LimitExceeded,
                    CONVERSION_TIMED_OUT,
                ));
            }
        };
        let value = result.map_err(|error| {
            let _ = std::fs::remove_file(&target);
            job_failure(error, &format)
        })?;
        Ok(DocumentConvertOutput {
            bytes: value
                .get("bytes")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        })
    }
}

pub fn register_document_nodes(
    registry: &NodeRegistry,
    jobs: SharedSidecarJobs,
    cache_dir: PathBuf,
) -> Result<(), AppError> {
    registry.register(DocumentConvertNode::new(jobs, cache_dir, CONVERT_TIMEOUT))
}

pub struct KernelDocumentConverter {
    runs: RunService,
    office: SharedOfficeLocator,
}

impl KernelDocumentConverter {
    pub fn new(runs: RunService, office: SharedOfficeLocator) -> Self {
        Self { runs, office }
    }
}

fn failure_code(message: &str) -> AppErrorCode {
    if message == CONVERSION_TIMED_OUT {
        AppErrorCode::LimitExceeded
    } else if message.starts_with(CONVERSION_FAILED_PREFIX) {
        AppErrorCode::UnsupportedFormat
    } else if message == CONVERSION_TARGET_REFUSED {
        AppErrorCode::PermissionDenied
    } else {
        AppErrorCode::RunFailed
    }
}

fn outcome_error(outcome: RunOutcomeDto) -> AppError {
    let code = match outcome.status {
        RunOutcomeStatus::Cancelled => AppErrorCode::RunCancelled,
        RunOutcomeStatus::Ok | RunOutcomeStatus::Failed => match outcome.code {
            Some(code) if code != AppErrorCode::RunFailed => code,
            _ => outcome
                .message
                .as_deref()
                .map_or(AppErrorCode::RunFailed, failure_code),
        },
    };
    let message = outcome.message.unwrap_or_else(|| match code {
        AppErrorCode::RunCancelled => CONVERSION_CANCELLED.to_owned(),
        _ => CONVERSION_NOT_STARTED.to_owned(),
    });
    AppError::new(code, message)
}

pub fn conversion_graph(input: &DocumentConvertInput) -> RunGraph {
    let mut node = zvs_contract::GraphNode::new("convert", DOCUMENT_CONVERT_NODE);
    node.dependencies = Some(Vec::new());
    node.bindings = Some(std::collections::BTreeMap::new());
    node.input = Some(json!(input));
    RunGraph {
        nodes: vec![node],
        permission_scopes: None,
        allowed_tools: None,
    }
}

#[async_trait]
impl DocumentConverter for KernelDocumentConverter {
    async fn ready(&self) -> bool {
        self.office.office_executable().await.is_some()
    }

    async fn convert_to_pdf(
        &self,
        source: &Path,
        format: OfficeFormat,
        target: &Path,
    ) -> Result<(), AppError> {
        let executable = self.office.office_executable().await.ok_or_else(|| {
            AppError::new(
                AppErrorCode::UnsupportedFormat,
                conversion_failed(format.label()),
            )
        })?;
        let input = DocumentConvertInput {
            executable: executable.to_string_lossy().into_owned(),
            source: source.to_string_lossy().into_owned(),
            target: target.to_string_lossy().into_owned(),
            format: format.extension().to_owned(),
        };
        let name = source.file_name().map_or_else(
            || input.source.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut start = StartRunInput::new(RunKind::Job, conversion_graph(&input));
        start.title = Some(conversion_title(&name));
        start.input = Some(Value::Null);
        start.concurrency = Some(1);
        let (sender, receiver) = oneshot::channel::<RunOutcomeDto>();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let observer: RunObserver = Arc::new(move |event: &HostEvent| {
            if let HostEventDraft::End { outcome } = &event.body
                && let Some(sender) = sender.lock().unwrap_or_else(PoisonError::into_inner).take()
            {
                let _ = sender.send(outcome.clone());
            }
        });
        let (_, subscription) = self.runs.start_with(start, |_, _| Ok(()), Some(observer))?;
        let outcome = receiver
            .await
            .map_err(|_| AppError::new(AppErrorCode::RunFailed, CONVERSION_NOT_STARTED))?;
        drop(subscription);
        if outcome.status == RunOutcomeStatus::Ok && target.is_file() {
            return Ok(());
        }
        Err(outcome_error(outcome))
    }
}

#[cfg(test)]
mod tests;
