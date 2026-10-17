use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, VectorOcrConfig, VectorOcrLanguage};

use super::extraction::{
    ExtractionInput, Extractor, Segment, is_blank, js_basename, js_extname, plain_segments,
};
use crate::drivers::sidecar::{JobRunOptions, SharedSidecarJobs};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::long_path::to_extended_path;
use crate::runtimes::RuntimeService;

pub const DOCUMENT_JOB: &str = "job.document.extract";
pub const OCR_OFF: &str = "распознавание выключено — включите OCR в настройках хранилища";
pub const IMAGE_UNREADABLE: &str = "Этот формат изображения не читается";
pub const DOCUMENT_UNREADABLE: &str = "Не удалось прочитать документ";

pub const IMAGE_MEDIA_TYPES: &[(&str, &str)] = &[
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".webp", "image/webp"),
    (".bmp", "image/bmp"),
    (".gif", "image/gif"),
];

pub fn image_extensions() -> Vec<&'static str> {
    IMAGE_MEDIA_TYPES
        .iter()
        .map(|(extension, _)| *extension)
        .collect()
}

#[async_trait]
pub trait OcrPort: Send + Sync {
    async fn read_image(
        &self,
        model_ref: &str,
        media_type: &str,
        bytes: &[u8],
        language: VectorOcrLanguage,
        cancel: &CancellationToken,
    ) -> Result<String, AppError>;
}

pub type SharedOcr = Arc<dyn OcrPort>;

#[async_trait]
impl OcrPort for RuntimeService {
    async fn read_image(
        &self,
        model_ref: &str,
        media_type: &str,
        bytes: &[u8],
        language: VectorOcrLanguage,
        cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        Self::read_image(self, model_ref, media_type, bytes, language, cancel).await
    }
}

#[derive(Clone)]
pub struct DocumentExtractorOptions {
    pub jobs: SharedSidecarJobs,
    pub ocr: Option<SharedOcr>,
    pub scratch_dir: PathBuf,
    pub logger: SharedLogger,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtractedPage {
    number: Value,
    text: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExtractedImage {
    page: Value,
    path: String,
    media_type: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtractedNote {
    page: Value,
    reason: String,
}

#[derive(Debug, Default)]
struct DocumentReport {
    pages: Vec<ExtractedPage>,
    images: Vec<ExtractedImage>,
    notes: Vec<ExtractedNote>,
}

struct UsableOcr<'a> {
    port: &'a SharedOcr,
    config: &'a VectorOcrConfig,
}

fn usable_ocr<'a>(
    input: &'a ExtractionInput,
    options: &'a DocumentExtractorOptions,
) -> Option<UsableOcr<'a>> {
    let port = options.ocr.as_ref()?;
    let config = input.context.ocr.as_ref()?;
    if !config.enabled || config.model_ref.is_empty() {
        return None;
    }
    Some(UsableOcr { port, config })
}

fn page_label(page: &Value) -> String {
    match page {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn page_key(page: &Value) -> String {
    page_label(page)
}

fn items<T: for<'de> Deserialize<'de> + Default>(report: &Value, key: &str) -> Vec<T> {
    report
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| serde_json::from_value(item.clone()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

async fn run_job(
    jobs: &SharedSidecarJobs,
    params: Value,
    cancel: &CancellationToken,
) -> Result<DocumentReport, AppError> {
    let result = jobs
        .run(
            DOCUMENT_JOB,
            params,
            JobRunOptions {
                cancel: Some(cancel.clone()),
                on_progress: None,
            },
        )
        .await?;
    if !result.is_object() {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            DOCUMENT_UNREADABLE,
        ));
    }
    Ok(DocumentReport {
        pages: items(&result, "pages"),
        images: items(&result, "images"),
        notes: items(&result, "notes"),
    })
}

async fn read_bytes(path: &str) -> Result<Vec<u8>, AppError> {
    let path = PathBuf::from(path);
    tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|_| super::worker_failed())?
        .map_err(|error| AppError::new(AppErrorCode::NativeError, error.to_string()))
}

async fn recognise_all(
    report: &DocumentReport,
    ocr: Option<&UsableOcr<'_>>,
    input: &ExtractionInput,
    options: &DocumentExtractorOptions,
) -> Result<HashMap<String, String>, AppError> {
    let mut recognised = HashMap::new();
    let Some(ocr) = ocr else {
        return Ok(recognised);
    };
    let cancel = input.context.cancel();
    for image in &report.images {
        if cancel.is_cancelled() {
            return Err(super::cancelled());
        }
        let outcome = async {
            let bytes = read_bytes(&image.path).await?;
            ocr.port
                .read_image(
                    &ocr.config.model_ref,
                    &image.media_type,
                    &bytes,
                    ocr.config.language,
                    &cancel,
                )
                .await
        }
        .await;
        match outcome {
            Ok(text) => {
                let trimmed = zvs_contract::validate::js_trim(&text);
                if !trimmed.is_empty() {
                    recognised.insert(page_key(&image.page), trimmed.to_owned());
                }
            }
            Err(error) if cancel.is_cancelled() => return Err(error),
            Err(error) => {
                input.context.note(format!(
                    "{}, с. {}: распознать не удалось — {}",
                    js_basename(&input.path),
                    page_label(&image.page),
                    error.message
                ));
                options.logger.log(
                    LogLevel::Warn,
                    "indexing",
                    "Could not OCR a page",
                    &[
                        ("path", json!(input.path)),
                        ("page", image.page.clone()),
                        ("error", json!(error.to_string())),
                    ],
                );
            }
        }
    }
    Ok(recognised)
}

fn positive_page(number: &Value) -> Option<u32> {
    number
        .as_u64()
        .filter(|page| *page > 0)
        .and_then(|page| u32::try_from(page).ok())
        .or_else(|| {
            number
                .as_f64()
                .filter(|page| page.fract() == 0.0 && *page > 0.0 && *page <= f64::from(u32::MAX))
                .map(|page| page as u32)
        })
}

fn assemble(pages: Vec<ExtractedPage>, recognised: &HashMap<String, String>) -> Vec<Segment> {
    pages
        .into_iter()
        .map(|page| Segment {
            text: recognised
                .get(&page_key(&page.number))
                .cloned()
                .unwrap_or(page.text),
            section: Vec::new(),
            page: positive_page(&page.number),
        })
        .filter(|segment| !is_blank(&segment.text))
        .collect()
}

pub struct PdfExtractor {
    options: DocumentExtractorOptions,
}

impl PdfExtractor {
    pub const fn new(options: DocumentExtractorOptions) -> Self {
        Self { options }
    }

    async fn extract_into(
        &self,
        input: &ExtractionInput,
        scratch: Option<&PathBuf>,
    ) -> Result<Vec<Segment>, AppError> {
        let ocr = usable_ocr(input, &self.options);
        let mut params = serde_json::Map::new();
        params.insert(
            "path".to_owned(),
            json!(to_extended_path(&input.path, cfg!(windows))),
        );
        params.insert(
            "minCharsPerPage".to_owned(),
            json!(ocr.as_ref().map_or(0, |ocr| ocr.config.min_chars_per_page)),
        );
        if let Some(scratch) = scratch {
            params.insert(
                "imagesDir".to_owned(),
                json!(scratch.to_string_lossy().into_owned()),
            );
        }
        let report = run_job(
            &self.options.jobs,
            Value::Object(params),
            &input.context.cancel(),
        )
        .await?;
        for note in &report.notes {
            input.context.note(format!(
                "{}, с. {}: {}",
                js_basename(&input.path),
                page_label(&note.page),
                note.reason
            ));
        }
        let recognised = recognise_all(&report, ocr.as_ref(), input, &self.options).await?;
        Ok(assemble(report.pages, &recognised))
    }
}

#[async_trait]
impl Extractor for PdfExtractor {
    async fn extract(&self, input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
        let scratch = usable_ocr(input, &self.options).map(|_| {
            self.options
                .scratch_dir
                .join(new_id().hyphenated().to_string())
        });
        let outcome = self.extract_into(input, scratch.as_ref()).await;
        if let Some(scratch) = scratch {
            let _ = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(scratch)).await;
        }
        outcome
    }
}

pub struct ImageExtractor {
    options: DocumentExtractorOptions,
}

impl ImageExtractor {
    pub const fn new(options: DocumentExtractorOptions) -> Self {
        Self { options }
    }
}

#[async_trait]
impl Extractor for ImageExtractor {
    async fn extract(&self, input: &ExtractionInput) -> Result<Vec<Segment>, AppError> {
        let Some(ocr) = usable_ocr(input, &self.options) else {
            return Err(AppError::new(AppErrorCode::UnsupportedFormat, OCR_OFF));
        };
        let extension = js_extname(&input.path).to_lowercase();
        let Some((_, media_type)) = IMAGE_MEDIA_TYPES
            .iter()
            .find(|(candidate, _)| *candidate == extension)
        else {
            return Err(AppError::new(
                AppErrorCode::UnsupportedFormat,
                IMAGE_UNREADABLE,
            ));
        };
        let text = ocr
            .port
            .read_image(
                &ocr.config.model_ref,
                media_type,
                &input.bytes,
                ocr.config.language,
                &input.context.cancel(),
            )
            .await?;
        self.options.logger.log(
            LogLevel::Debug,
            "indexing",
            "Read an image with the vision model",
            &[
                ("path", json!(input.path)),
                ("chars", json!(text.encode_utf16().count())),
            ],
        );
        Ok(plain_segments(text))
    }
}

#[cfg(test)]
mod tests;
