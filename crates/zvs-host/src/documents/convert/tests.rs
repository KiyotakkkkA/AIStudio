use std::sync::Mutex;

use serde_json::json;
use zvs_contract::{RunListFilter, RunStatus};

use super::*;
use crate::drivers::sidecar::SidecarJobs;
use crate::platform::events::EventBus;
use crate::services::RunServiceOptions;
use crate::testing::{TemporaryDatabase, temporary_database};

enum Script {
    Writes(&'static [u8]),
    Fails(AppErrorCode),
    Hangs,
}

struct ScriptedJobs {
    script: Script,
    calls: Mutex<Vec<(String, Value)>>,
}

#[async_trait]
impl SidecarJobs for ScriptedJobs {
    async fn run(
        &self,
        job: &str,
        params: Value,
        options: JobRunOptions,
    ) -> Result<Value, AppError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((job.to_owned(), params.clone()));
        let target = PathBuf::from(params["target"].as_str().unwrap_or_default());
        match self.script {
            Script::Writes(bytes) => {
                if let Some(sink) = options.on_progress {
                    sink(JobProgress {
                        done: 1,
                        total: 1,
                        rate: None,
                        message: None,
                    });
                }
                std::fs::write(&target, bytes).expect("writes the pdf");
                Ok(json!({ "bytes": bytes.len() }))
            }
            Script::Fails(code) => {
                std::fs::write(&target, b"partial").expect("writes a partial file");
                Err(AppError::new(code, "sidecar said no"))
            }
            Script::Hangs => {
                let cancel = options.cancel.unwrap_or_default();
                cancel.cancelled().await;
                Err(AppError::new(AppErrorCode::RunCancelled, "cancelled"))
            }
        }
    }
}

struct Office(Option<PathBuf>);

#[async_trait]
impl OfficeLocator for Office {
    async fn office_executable(&self) -> Option<PathBuf> {
        self.0.clone()
    }
}

struct Harness {
    runs: RunService,
    jobs: Arc<ScriptedJobs>,
    cache: tempfile::TempDir,
    _database: TemporaryDatabase,
}

fn harness(script: Script, timeout: Duration) -> Harness {
    let database = temporary_database();
    let cache = tempfile::tempdir().expect("a cache folder");
    let jobs = Arc::new(ScriptedJobs {
        script,
        calls: Mutex::new(Vec::new()),
    });
    let registry = Arc::new(NodeRegistry::new());
    registry
        .register(DocumentConvertNode {
            jobs: jobs.clone(),
            cache_dir: cache.path().to_path_buf(),
            timeout,
        })
        .expect("registers");
    let runs = RunService::new(RunServiceOptions::new(
        database.shared(),
        EventBus::default(),
        registry,
    ))
    .expect("a run service");
    Harness {
        runs,
        jobs,
        cache,
        _database: database,
    }
}

impl Harness {
    fn converter(&self, office: Option<&str>) -> KernelDocumentConverter {
        KernelDocumentConverter::new(
            self.runs.clone(),
            Arc::new(Office(office.map(PathBuf::from))),
        )
    }

    fn source(&self) -> PathBuf {
        let source = self.cache.path().join("..").join("letter.docx");
        std::fs::write(&source, b"PK").expect("writes");
        source
    }

    fn target(&self) -> PathBuf {
        self.cache.path().join("abc.pdf.tmp")
    }

    fn last_run(&self) -> zvs_contract::RunSummaryDto {
        self.runs
            .list(&RunListFilter::default())
            .expect("lists runs")
            .items
            .into_iter()
            .next()
            .expect("a run")
    }
}

#[tokio::test]
async fn a_conversion_runs_as_a_titled_job_through_the_sidecar() {
    let harness = harness(Script::Writes(b"%PDF-1.7"), CONVERT_TIMEOUT);
    let converter = harness.converter(Some("C:/LibreOffice/program/soffice.com"));
    assert!(converter.ready().await);
    let target = harness.target();
    converter
        .convert_to_pdf(&harness.source(), OfficeFormat::Docx, &target)
        .await
        .expect("converts");
    assert_eq!(std::fs::read(&target).expect("reads"), b"%PDF-1.7");
    let calls = harness
        .jobs
        .calls
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, DOCUMENT_CONVERT_JOB);
    assert_eq!(
        calls[0].1["executable"],
        json!("C:/LibreOffice/program/soffice.com")
    );
    let run = harness.last_run();
    assert_eq!(run.kind, RunKind::Job);
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.title, "Преобразование «letter.docx» в PDF");
}

#[tokio::test]
async fn without_libreoffice_the_converter_is_not_ready() {
    let harness = harness(Script::Writes(b"%PDF"), CONVERT_TIMEOUT);
    let converter = harness.converter(None);
    assert!(!converter.ready().await);
    let error = converter
        .convert_to_pdf(&harness.source(), OfficeFormat::Xlsx, &harness.target())
        .await
        .expect_err("no converter");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert!(
        harness
            .jobs
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
}

#[tokio::test]
async fn an_unreadable_document_fails_with_a_message_naming_its_format_and_leaves_no_file() {
    let harness = harness(
        Script::Fails(AppErrorCode::UnsupportedFormat),
        CONVERT_TIMEOUT,
    );
    let target = harness.target();
    let error = harness
        .converter(Some("soffice"))
        .convert_to_pdf(&harness.source(), OfficeFormat::Pptx, &target)
        .await
        .expect_err("fails");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(error.message, conversion_failed("PPTX"));
    assert!(!target.exists());
}

#[tokio::test]
async fn a_hanging_conversion_is_timed_out_and_its_job_cancelled() {
    let harness = harness(Script::Hangs, Duration::ZERO);
    let target = harness.target();
    let started = std::time::Instant::now();
    let error = harness
        .converter(Some("soffice"))
        .convert_to_pdf(&harness.source(), OfficeFormat::Docx, &target)
        .await
        .expect_err("times out");
    assert!(started.elapsed() < TIMEOUT_MARGIN + Duration::from_secs(10));
    assert_eq!(error.code, AppErrorCode::LimitExceeded);
    assert_eq!(error.message, CONVERSION_TIMED_OUT);
    assert!(!target.exists());
}

#[tokio::test]
async fn cancelling_the_run_kills_the_job_and_leaves_no_partial_file() {
    let harness = harness(Script::Hangs, CONVERT_TIMEOUT);
    let target = harness.target();
    let converter = harness.converter(Some("soffice"));
    let source = harness.source();
    let runs = harness.runs.clone();
    let cancel = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if let Some(run) = runs
                .list(&RunListFilter::default())
                .expect("lists")
                .items
                .into_iter()
                .find(|run| run.status == RunStatus::Running)
            {
                runs.cancel(&run.id).await.expect("cancels");
                break;
            }
        }
    });
    let error = converter
        .convert_to_pdf(&source, OfficeFormat::Docx, &target)
        .await
        .expect_err("cancelled");
    cancel.await.expect("the canceller finishes");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert!(!target.exists());
}

#[tokio::test]
async fn the_node_refuses_a_target_outside_the_document_cache() {
    let harness = harness(Script::Writes(b"%PDF"), CONVERT_TIMEOUT);
    let outside = tempfile::tempdir().expect("another folder");
    let error = harness
        .converter(Some("soffice"))
        .convert_to_pdf(
            &harness.source(),
            OfficeFormat::Docx,
            &outside.path().join("x.pdf"),
        )
        .await
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::PermissionDenied);
    assert!(
        harness
            .jobs
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
}
