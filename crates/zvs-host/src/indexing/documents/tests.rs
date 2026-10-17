use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::drivers::sidecar::SidecarJobs;
use crate::indexing::extraction::{ExtractionContext, NoteSink, segments_text};
use crate::platform::logger::silent_logger;
use crate::testing::{TemporaryDirectory, temporary_directory};

#[derive(Default)]
struct ScriptedJobs {
    result: Value,
    calls: Mutex<Vec<(String, Value)>>,
}

impl ScriptedJobs {
    fn returning(result: Value) -> Arc<Self> {
        Arc::new(Self {
            result,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().expect("calls").clone()
    }
}

#[async_trait]
impl SidecarJobs for ScriptedJobs {
    async fn run(
        &self,
        job: &str,
        params: Value,
        _options: JobRunOptions,
    ) -> Result<Value, AppError> {
        self.calls
            .lock()
            .expect("calls")
            .push((job.to_owned(), params));
        Ok(self.result.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrCall {
    model_ref: String,
    media_type: String,
    bytes: Vec<u8>,
    language: VectorOcrLanguage,
}

struct StubOcr {
    answer: Result<String, String>,
    calls: Mutex<Vec<OcrCall>>,
}

impl StubOcr {
    fn answering(text: &str) -> Arc<Self> {
        Arc::new(Self {
            answer: Ok(text.to_owned()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn failing(message: &str) -> Arc<Self> {
        Arc::new(Self {
            answer: Err(message.to_owned()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<OcrCall> {
        self.calls.lock().expect("calls").clone()
    }
}

#[async_trait]
impl OcrPort for StubOcr {
    async fn read_image(
        &self,
        model_ref: &str,
        media_type: &str,
        bytes: &[u8],
        language: VectorOcrLanguage,
        _cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        self.calls.lock().expect("calls").push(OcrCall {
            model_ref: model_ref.to_owned(),
            media_type: media_type.to_owned(),
            bytes: bytes.to_vec(),
            language,
        });
        self.answer
            .clone()
            .map_err(|message| AppError::new(AppErrorCode::NativeError, message))
    }
}

const MODEL: &str = "curated:model:qwen2.5-vl-7b-instruct-q4_k_m";

fn ocr_config(language: VectorOcrLanguage, model_ref: &str) -> VectorOcrConfig {
    VectorOcrConfig {
        enabled: true,
        model_ref: model_ref.to_owned(),
        language,
        min_chars_per_page: 200,
    }
}

fn options(
    jobs: Arc<ScriptedJobs>,
    ocr: Arc<StubOcr>,
    workspace: &TemporaryDirectory,
) -> DocumentExtractorOptions {
    DocumentExtractorOptions {
        jobs,
        ocr: Some(ocr),
        scratch_dir: workspace.join("scratch"),
        logger: silent_logger(),
    }
}

fn input(path: &str, ocr: Option<VectorOcrConfig>, note: Option<NoteSink>) -> ExtractionInput {
    ExtractionInput {
        path: path.to_owned(),
        bytes: Vec::new(),
        context: ExtractionContext {
            ocr,
            cancel: None,
            note,
        },
    }
}

fn segment(text: &str, page: Option<u32>) -> Segment {
    Segment::new(text, &[], page)
}

#[tokio::test]
async fn a_pdf_with_a_text_layer_is_returned_as_is_and_ocr_is_never_asked_for() {
    let workspace = temporary_directory("documents-");
    let jobs = ScriptedJobs::returning(json!({
        "pages": [
            { "number": 1, "text": "Первая страница", "chars": 15 },
            { "number": 2, "text": "Вторая страница", "chars": 15 },
        ],
        "images": [],
        "notes": [],
    }));
    let ocr = StubOcr::answering("never");
    let extractor = PdfExtractor::new(options(jobs.clone(), ocr.clone(), &workspace));

    let segments = extractor
        .extract(&input("a.pdf", None, None))
        .await
        .expect("extracts");

    assert_eq!(
        segments,
        [
            segment("Первая страница", Some(1)),
            segment("Вторая страница", Some(2)),
        ]
    );
    assert!(ocr.calls().is_empty());
    let calls = jobs.calls();
    assert_eq!(calls[0].0, DOCUMENT_JOB);
    assert_eq!(calls[0].1["minCharsPerPage"], json!(0));
    assert!(calls[0].1.get("imagesDir").is_none());
}

#[tokio::test]
async fn a_scanned_page_is_replaced_by_what_the_vision_model_read_not_merged_with_it() {
    let workspace = temporary_directory("documents-");
    let scan = workspace.join("page-2.jpg");
    std::fs::write(&scan, b"jpeg-bytes").expect("writes");
    let jobs = ScriptedJobs::returning(json!({
        "pages": [
            { "number": 1, "text": "Настоящий текст страницы один".repeat(10), "chars": 290 },
            { "number": 2, "text": "стр. 2", "chars": 6 },
        ],
        "images": [{ "page": 2, "path": scan.to_string_lossy(), "mediaType": "image/jpeg" }],
        "notes": [],
    }));
    let ocr = StubOcr::answering("  Распознанный текст скана  ");
    let extractor = PdfExtractor::new(options(jobs.clone(), ocr.clone(), &workspace));

    let segments = extractor
        .extract(&input(
            "scan.pdf",
            Some(ocr_config(VectorOcrLanguage::Rus, MODEL)),
            None,
        ))
        .await
        .expect("extracts");
    let text = segments_text(&segments);

    assert_eq!(segments[1], segment("Распознанный текст скана", Some(2)));
    assert!(text.contains("Распознанный текст скана"));
    assert!(!text.contains("стр. 2"));
    let params = &jobs.calls()[0].1;
    assert_eq!(params["minCharsPerPage"], json!(200));
    let images_dir = PathBuf::from(params["imagesDir"].as_str().expect("an images dir"));
    assert!(images_dir.starts_with(workspace.join("scratch")));
    assert!(!images_dir.exists());
    assert_eq!(
        ocr.calls(),
        [OcrCall {
            model_ref: MODEL.to_owned(),
            media_type: "image/jpeg".to_owned(),
            bytes: b"jpeg-bytes".to_vec(),
            language: VectorOcrLanguage::Rus,
        }]
    );
}

#[tokio::test]
async fn a_page_the_model_cannot_read_is_noted_and_the_rest_of_the_document_survives() {
    let workspace = temporary_directory("documents-");
    let scan = workspace.join("page-2.jpg");
    std::fs::write(&scan, b"jpeg-bytes").expect("writes");
    let jobs = ScriptedJobs::returning(json!({
        "pages": [
            { "number": 1, "text": "Читаемая страница", "chars": 300 },
            { "number": 2, "text": "", "chars": 0 },
        ],
        "images": [{ "page": 2, "path": scan.to_string_lossy(), "mediaType": "image/jpeg" }],
        "notes": [{ "page": 3, "reason": "на странице нет ни текста, ни встроенного изображения" }],
    }));
    let notes = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink: NoteSink = {
        let notes = Arc::clone(&notes);
        Arc::new(move |message| notes.lock().expect("notes").push(message))
    };
    let extractor = PdfExtractor::new(options(
        jobs,
        StubOcr::failing("модель не запустилась"),
        &workspace,
    ));

    let segments = extractor
        .extract(&input(
            "scan.pdf",
            Some(ocr_config(VectorOcrLanguage::Auto, MODEL)),
            Some(sink),
        ))
        .await
        .expect("extracts");

    assert_eq!(segments, [segment("Читаемая страница", Some(1))]);
    let notes = notes.lock().expect("notes").clone();
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().any(|note| note.contains("с. 3")));
    assert!(
        notes
            .iter()
            .any(|note| note.contains("модель не запустилась"))
    );
    assert!(notes.iter().all(|note| note.starts_with("scan.pdf, с. ")));
}

#[tokio::test]
async fn ocr_that_is_switched_on_but_has_no_model_chosen_stays_switched_off() {
    let workspace = temporary_directory("documents-");
    let jobs = ScriptedJobs::returning(json!({
        "pages": [{ "number": 1, "text": "a", "chars": 1 }],
        "images": [],
        "notes": [],
    }));
    let ocr = StubOcr::answering("never");
    let extractor = PdfExtractor::new(options(jobs.clone(), ocr.clone(), &workspace));

    extractor
        .extract(&input(
            "a.pdf",
            Some(ocr_config(VectorOcrLanguage::Auto, "")),
            None,
        ))
        .await
        .expect("extracts");

    assert_eq!(jobs.calls()[0].1["minCharsPerPage"], json!(0));
    assert!(ocr.calls().is_empty());
}

#[tokio::test]
async fn a_malformed_job_answer_is_refused_and_missing_lists_read_as_empty() {
    let workspace = temporary_directory("documents-");
    let extractor = PdfExtractor::new(options(
        ScriptedJobs::returning(json!([1, 2])),
        StubOcr::answering("never"),
        &workspace,
    ));
    let error = extractor
        .extract(&input("a.pdf", None, None))
        .await
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, DOCUMENT_UNREADABLE);

    let extractor = PdfExtractor::new(options(
        ScriptedJobs::returning(json!({ "pages": "nope" })),
        StubOcr::answering("never"),
        &workspace,
    ));
    assert!(
        extractor
            .extract(&input("a.pdf", None, None))
            .await
            .expect("extracts")
            .is_empty()
    );
}

#[tokio::test]
async fn an_image_file_is_read_by_the_vision_model_and_refused_when_ocr_is_off() {
    let workspace = temporary_directory("documents-");
    let ocr = StubOcr::answering("Текст с фотографии");
    let extractor = ImageExtractor::new(options(
        ScriptedJobs::returning(json!({})),
        ocr.clone(),
        &workspace,
    ));
    let mut photo = input(
        "photo.png",
        Some(ocr_config(VectorOcrLanguage::Auto, MODEL)),
        None,
    );
    photo.bytes = b"png-bytes".to_vec();

    assert!(image_extensions().contains(&".png"));
    assert_eq!(
        extractor.extract(&photo).await.expect("extracts"),
        [segment("Текст с фотографии", None)]
    );
    assert_eq!(ocr.calls()[0].media_type, "image/png");
    assert_eq!(ocr.calls()[0].bytes, b"png-bytes");

    photo.context.ocr = None;
    let error = extractor.extract(&photo).await.expect_err("declines");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(error.message, OCR_OFF);
}

#[tokio::test]
async fn an_image_with_an_unknown_extension_is_declined() {
    let workspace = temporary_directory("documents-");
    let extractor = ImageExtractor::new(options(
        ScriptedJobs::returning(json!({})),
        StubOcr::answering("x"),
        &workspace,
    ));
    let error = extractor
        .extract(&input(
            "scan.tiff",
            Some(ocr_config(VectorOcrLanguage::Auto, MODEL)),
            None,
        ))
        .await
        .expect_err("declines");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(error.message, IMAGE_UNREADABLE);
}
