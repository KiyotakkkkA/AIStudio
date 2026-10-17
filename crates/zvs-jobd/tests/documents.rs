use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, VectorOcrConfig, VectorOcrLanguage};
use zvs_host::drivers::sidecar::{SidecarDriver, SidecarOptions};
use zvs_host::indexing::extraction::NoteSink;
use zvs_host::indexing::{
    DocumentExtractorOptions, ExtractionContext, ExtractionInput, Extractor, OcrPort, PdfExtractor,
    Segment,
};
use zvs_host::platform::logger::silent_logger;

fn jobd() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zvs-jobd"))
}

const SCAN: &[u8] = b"\xff\xd8\xff\xe0scanned-page-bytes\xff\xd9";

fn object(number: usize, body: &[u8]) -> Vec<u8> {
    let mut out = format!("{number} 0 obj\n").into_bytes();
    out.extend_from_slice(body);
    out.extend_from_slice(b"\nendobj\n");
    out
}

fn stream(dictionary: &str, content: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dictionary} /Length {} >>\nstream\n", content.len()).into_bytes();
    out.extend_from_slice(content);
    out.extend_from_slice(b"\nendstream");
    out
}

fn mixed_pdf() -> Vec<u8> {
    let text = b"BT /F1 24 Tf 20 100 Td (Hello world from page one) Tj ET".to_vec();
    let draw = b"q 200 0 0 200 0 0 cm /Im1 Do Q".to_vec();
    let bodies: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        stream("", &text),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 8 0 R /Resources << /XObject << /Im1 7 0 R >> >> >>".to_vec(),
        stream(
            "/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode",
            SCAN,
        ),
        stream("", &draw),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(&object(index + 1, body));
    }
    let xref = out.len();
    let size = bodies.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    out
}

#[derive(Default)]
struct RecordingOcr {
    calls: Mutex<Vec<(String, Vec<u8>, VectorOcrLanguage)>>,
}

#[async_trait::async_trait]
impl OcrPort for RecordingOcr {
    async fn read_image(
        &self,
        _model_ref: &str,
        media_type: &str,
        bytes: &[u8],
        language: VectorOcrLanguage,
        _cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        self.calls
            .lock()
            .expect("calls")
            .push((media_type.to_owned(), bytes.to_vec(), language));
        Ok("Распознанный текст скана".to_owned())
    }
}

fn driver() -> SidecarDriver {
    SidecarDriver::new(SidecarOptions {
        logger: silent_logger(),
        backoff: Duration::from_millis(10),
        ping_interval: Duration::from_secs(60),
        ..SidecarOptions::new(jobd())
    })
}

fn input(path: &Path, ocr: Option<VectorOcrConfig>, note: Option<NoteSink>) -> ExtractionInput {
    ExtractionInput {
        path: path.to_string_lossy().into_owned(),
        bytes: Vec::new(),
        context: ExtractionContext {
            ocr,
            cancel: None,
            note,
        },
    }
}

#[tokio::test]
async fn a_text_page_reads_through_the_sidecar_and_a_scanned_page_goes_to_ocr() {
    let workspace = tempfile::tempdir().expect("a directory");
    let file = workspace.path().join("mixed.pdf");
    std::fs::write(&file, mixed_pdf()).expect("writes");
    let scratch = workspace.path().join("scratch");
    let sidecar = driver();
    let ocr = Arc::new(RecordingOcr::default());
    let extractor = PdfExtractor::new(DocumentExtractorOptions {
        jobs: Arc::new(sidecar.clone()),
        ocr: Some(ocr.clone()),
        scratch_dir: scratch.clone(),
        logger: silent_logger(),
    });
    let notes = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink: NoteSink = {
        let notes = Arc::clone(&notes);
        Arc::new(move |message| notes.lock().expect("notes").push(message))
    };

    let segments = extractor
        .extract(&input(
            &file,
            Some(VectorOcrConfig {
                enabled: true,
                model_ref: "curated:model:vision".to_owned(),
                language: VectorOcrLanguage::Rus,
                min_chars_per_page: 5,
            }),
            Some(sink),
        ))
        .await
        .expect("extracts");

    assert_eq!(segments.len(), 2, "{segments:?}");
    assert_eq!(segments[0].page, Some(1));
    assert!(segments[0].text.contains("Hello world from page one"));
    assert_eq!(
        segments[1],
        Segment::new("Распознанный текст скана", &[], Some(2))
    );
    assert_eq!(
        *ocr.calls.lock().expect("calls"),
        [(
            "image/jpeg".to_owned(),
            SCAN.to_vec(),
            VectorOcrLanguage::Rus
        )]
    );
    assert!(notes.lock().expect("notes").is_empty());
    let leftovers = std::fs::read_dir(&scratch)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(leftovers, 0, "the page images are cleaned up");

    let text_only = extractor
        .extract(&input(&file, None, None))
        .await
        .expect("extracts");
    assert_eq!(text_only.len(), 1);
    assert_eq!(text_only[0].page, Some(1));
    assert_eq!(ocr.calls.lock().expect("calls").len(), 1);
    sidecar.dispose().await;
}

#[tokio::test]
async fn a_file_that_is_not_a_pdf_fails_the_job_without_taking_the_sidecar_down() {
    let workspace = tempfile::tempdir().expect("a directory");
    let file = workspace.path().join("fake.pdf");
    std::fs::write(&file, b"%PDF-1.7 but nothing else").expect("writes");
    let good = workspace.path().join("good.pdf");
    std::fs::write(&good, mixed_pdf()).expect("writes");
    let sidecar = driver();
    let extractor = PdfExtractor::new(DocumentExtractorOptions {
        jobs: Arc::new(sidecar.clone()),
        ocr: None,
        scratch_dir: workspace.path().join("scratch"),
        logger: silent_logger(),
    });

    assert!(extractor.extract(&input(&file, None, None)).await.is_err());
    let segments = extractor
        .extract(&input(&good, None, None))
        .await
        .expect("still extracts");
    assert!(segments[0].text.contains("Hello world"));
    sidecar.dispose().await;
}
