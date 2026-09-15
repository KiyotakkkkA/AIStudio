//! Document extraction as a sidecar job.
//!
//! Text comes back inline; page images do not. A scanned page is a megabyte or two of JPEG, and
//! the protocol is newline-delimited JSON on a pipe — so images are written to a directory the
//! caller names and reported by path.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_core::{
    CancellationToken, Error, Result,
    pdf::{self, PdfNote, PdfPage},
};

use crate::jobs::JobContext;

pub const DOCUMENT_JOB: &str = "job.document.extract";

pub const MAX_DOCUMENT_BYTES: u64 = 512 * 1024 * 1024;
/// How many scanned pages one document may hand to OCR. OCR is the slow stage by a wide margin.
pub const DEFAULT_MAX_IMAGES: usize = 50;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtractParams {
    path: String,
    /// Below this many characters a page counts as scanned. Zero disables image extraction.
    #[serde(default)]
    min_chars_per_page: usize,
    /// Where page images are written. Absent means the caller wants text only.
    #[serde(default)]
    images_dir: Option<String>,
    #[serde(default)]
    max_images: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractedImage {
    page: u32,
    path: String,
    media_type: String,
    bytes: u64,
}

pub async fn extract(params: Value, context: &JobContext) -> Result<Value> {
    let params: ExtractParams = serde_json::from_value(params).map_err(|error| {
        Error::InvalidInput(format!("Invalid {DOCUMENT_JOB} parameters: {error}"))
    })?;
    let path = PathBuf::from(&params.path);
    let metadata = tokio::fs::metadata(&path).await?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::InvalidInput(format!(
            "The document is larger than {MAX_DOCUMENT_BYTES} bytes"
        )));
    }

    context.progress(0, 1, Some("Чтение документа".to_owned()));
    let bytes = tokio::fs::read(&path).await?;
    cancelled(context.token())?;

    // Parsing is CPU-bound and can run for seconds on a large file; keeping it off the reactor
    // means cancellation and progress still get served while it works.
    let pages = tokio::task::spawn_blocking(move || pdf::extract_pages(&bytes))
        .await
        .map_err(|error| Error::Backend(format!("Extraction task failed: {error}")))??;

    let total = u64::try_from(pages.len()).unwrap_or(u64::MAX).max(1);
    context.progress(
        total,
        total,
        Some(format!("Страниц прочитано: {}", pages.len())),
    );
    cancelled(context.token())?;

    let scanned = scanned_pages(&pages, params.min_chars_per_page);
    let mut notes: Vec<PdfNote> = Vec::new();
    let mut images: Vec<ExtractedImage> = Vec::new();

    if let Some(directory) = params.images_dir.as_deref()
        && !scanned.is_empty()
    {
        let budget = params.max_images.unwrap_or(DEFAULT_MAX_IMAGES);
        let (written, extra) =
            write_images(&path, directory, &scanned, budget, context.token()).await?;
        images = written;
        notes = extra;
    }

    Ok(json!({
        "pages": pages,
        "images": images,
        "notes": notes,
    }))
}

fn cancelled(token: &CancellationToken) -> Result<()> {
    if token.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

fn scanned_pages(pages: &[PdfPage], min_chars: usize) -> BTreeSet<u32> {
    if min_chars == 0 {
        return BTreeSet::new();
    }
    pages
        .iter()
        .filter(|page| page.chars < min_chars)
        .map(|page| page.number)
        .collect()
}

async fn write_images(
    document: &Path,
    directory: &str,
    scanned: &BTreeSet<u32>,
    budget: usize,
    token: &CancellationToken,
) -> Result<(Vec<ExtractedImage>, Vec<PdfNote>)> {
    tokio::fs::create_dir_all(directory).await?;
    let bytes = tokio::fs::read(document).await?;
    let wanted = scanned.clone();
    let (found, mut notes) = tokio::task::spawn_blocking(move || pdf::page_images(&bytes, &wanted))
        .await
        .map_err(|error| Error::Backend(format!("Image extraction task failed: {error}")))??;

    let stem = document
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let safe: String = stem
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(48)
        .collect();

    let mut written = Vec::new();
    for image in found {
        cancelled(token)?;
        if written.len() >= budget {
            notes.push(PdfNote {
                page: image.page,
                reason: format!("распознавание ограничено первыми {budget} страницами"),
            });
            break;
        }
        let extension = if image.media_type == "image/jpeg" {
            "jpg"
        } else {
            "jp2"
        };
        let target = Path::new(directory).join(format!("{safe}-p{}.{extension}", image.page));
        let size = u64::try_from(image.bytes.len()).unwrap_or(u64::MAX);
        tokio::fs::write(&target, &image.bytes).await?;
        written.push(ExtractedImage {
            page: image.page,
            path: target.to_string_lossy().into_owned(),
            media_type: image.media_type,
            bytes: size,
        });
    }
    Ok((written, notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Response;
    use std::time::Duration;
    use tokio::sync::mpsc::unbounded_channel;

    fn context() -> (JobContext, tokio::sync::mpsc::UnboundedReceiver<Response>) {
        let (sender, receiver) = unbounded_channel();
        (
            JobContext::new("d", sender, CancellationToken::new(), Duration::ZERO),
            receiver,
        )
    }

    #[test]
    fn a_thin_page_counts_as_scanned_and_a_zero_threshold_disables_ocr() {
        let pages = vec![
            PdfPage {
                number: 1,
                text: "x".repeat(500),
                chars: 500,
            },
            PdfPage {
                number: 2,
                text: String::new(),
                chars: 0,
            },
        ];
        assert_eq!(scanned_pages(&pages, 200), BTreeSet::from([2]));
        assert!(scanned_pages(&pages, 0).is_empty());
    }

    #[tokio::test]
    async fn a_missing_file_is_a_not_found_error() {
        let (context, _receiver) = context();
        let error = extract(json!({ "path": "no/such/file.pdf" }), &context)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "NOT_FOUND");
    }

    #[tokio::test]
    async fn bad_parameters_are_rejected_before_any_io() {
        let (context, _receiver) = context();
        for params in [json!({}), json!({ "path": "a.pdf", "nope": 1 })] {
            let error = extract(params, &context).await.unwrap_err();
            assert_eq!(error.code(), "VALIDATION_FAILED");
        }
    }
}
