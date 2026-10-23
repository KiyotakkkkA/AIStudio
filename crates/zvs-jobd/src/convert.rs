use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use zvs_core::Error;
use zvs_core::Result;
use zvs_core::convert::{DEFAULT_CONVERT_TIMEOUT, OfficeConverter, SofficeConverter};

use crate::jobs::JobContext;

pub const CONVERT_JOB: &str = "job.document.convert";
pub const MAX_CONVERT_TIMEOUT_MS: u64 = 60 * 60 * 1000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConvertParams {
    executable: String,
    source: String,
    target: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

fn work_dir(target: &Path) -> Result<PathBuf> {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Error::InvalidInput("The target has no file name".to_owned()))?;
    let parent = target
        .parent()
        .ok_or_else(|| Error::InvalidInput("The target has no folder".to_owned()))?;
    Ok(parent.join(format!(".{name}.work")))
}

pub async fn convert(params: Value, context: &JobContext) -> Result<Value> {
    let params: ConvertParams = serde_json::from_value(params).map_err(|error| {
        Error::InvalidInput(format!("Invalid {CONVERT_JOB} parameters: {error}"))
    })?;
    let timeout = match params.timeout_ms {
        Some(0) => return Err(Error::InvalidInput("timeoutMs must be positive".to_owned())),
        Some(ms) if ms > MAX_CONVERT_TIMEOUT_MS => {
            return Err(Error::InvalidInput(format!(
                "timeoutMs is at most {MAX_CONVERT_TIMEOUT_MS}"
            )));
        }
        Some(ms) => Duration::from_millis(ms),
        None => DEFAULT_CONVERT_TIMEOUT,
    };
    let source = PathBuf::from(&params.source);
    let target = PathBuf::from(&params.target);
    let work = work_dir(&target)?;
    let mut converter = SofficeConverter::new(&params.executable);
    converter.timeout = timeout;
    context.progress(0, 1, Some("Преобразование в PDF".to_owned()));
    let token = context.token().clone();
    let outcome = {
        let work = work.clone();
        let target = target.clone();
        tokio::task::spawn_blocking(move || -> Result<u64> {
            let _ = std::fs::remove_dir_all(&work);
            let produced = converter.convert_to_pdf(&source, &work, &token)?;
            std::fs::rename(&produced, &target)?;
            Ok(std::fs::metadata(&target)?.len())
        })
        .await
        .map_err(|error| Error::Backend(format!("Conversion task failed: {error}")))?
    };
    let _ = tokio::fs::remove_dir_all(&work).await;
    match outcome {
        Ok(bytes) => {
            context.progress(1, 1, Some("Преобразование завершено".to_owned()));
            Ok(json!({ "target": params.target, "bytes": bytes }))
        }
        Err(error) => {
            let _ = tokio::fs::remove_file(&target).await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Response;
    use tokio::sync::mpsc::unbounded_channel;
    use zvs_core::CancellationToken;

    fn context() -> (JobContext, tokio::sync::mpsc::UnboundedReceiver<Response>) {
        let (sender, receiver) = unbounded_channel();
        (
            JobContext::new("c", sender, CancellationToken::new(), Duration::ZERO),
            receiver,
        )
    }

    #[test]
    fn the_work_folder_sits_beside_the_target() {
        assert_eq!(
            work_dir(Path::new("/cache/abc.pdf.tmp")).unwrap(),
            PathBuf::from("/cache/.abc.pdf.tmp.work")
        );
    }

    #[tokio::test]
    async fn bad_parameters_are_rejected_before_any_process_starts() {
        let (context, _receiver) = context();
        for params in [
            json!({}),
            json!({ "executable": "s", "source": "a", "target": "b", "nope": 1 }),
            json!({ "executable": "s", "source": "a", "target": "b", "timeoutMs": 0 }),
        ] {
            let error = convert(params, &context).await.unwrap_err();
            assert_eq!(error.code(), "VALIDATION_FAILED");
        }
    }

    #[tokio::test]
    async fn a_failed_conversion_leaves_neither_the_target_nor_the_work_folder() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("abc.pdf.tmp");
        let (context, _receiver) = context();
        let error = convert(
            json!({
                "executable": "zvs-no-such-converter",
                "source": directory.path().join("missing.docx"),
                "target": target,
            }),
            &context,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), "NOT_FOUND");
        assert!(!target.exists());
        assert!(!work_dir(&target).unwrap().exists());
    }

    #[tokio::test]
    async fn a_cancelled_job_does_not_start_the_converter() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("a.docx");
        std::fs::write(&source, b"PK").unwrap();
        let target = directory.path().join("a.pdf.tmp");
        let (sender, _receiver) = unbounded_channel();
        let token = CancellationToken::new();
        token.cancel();
        let context = JobContext::new("c", sender, token, Duration::ZERO);
        let error = convert(
            json!({ "executable": "zvs-no-such-converter", "source": source, "target": target }),
            &context,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), "RUN_CANCELLED");
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn a_known_docx_converts_to_its_two_pages_when_libreoffice_is_available() {
        let Some(executable) = std::env::var_os("ZVS_SOFFICE") else {
            return;
        };
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("two-pages.docx");
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("two-pages.pdf.tmp");
        let (context, _receiver) = context();
        let result = convert(
            json!({ "executable": executable.to_string_lossy(), "source": fixture, "target": target }),
            &context,
        )
        .await
        .expect("converts");
        assert!(result["bytes"].as_u64().unwrap_or_default() > 0);
        let pages = zvs_core::pdf::extract_pages(&std::fs::read(&target).unwrap()).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(!work_dir(&target).unwrap().exists());
    }
}
