use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use zvs_contract::{
    AppError, AppErrorCode, ChatCitationDto, DocumentDto, DocumentId, DocumentKind,
    DocumentOfficeFormat, DocumentPageDto, DocumentRef, DocumentRendering, DocumentTextEncoding,
    VectorStoreId,
};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::{VectorDocumentDraft, VectorStoreDraft};
use crate::drivers::core::InProcessCore;
use crate::platform::clock::FakeClock;
use crate::testing::{TemporaryDatabase, seed_embedding_provider, temporary_database};
use crate::workspace::{ForbiddenFolders, canonical};

const PDF_BYTES: &[u8] = b"%PDF-1.7\n1 0 obj << /Type /Pages /Count 2 >> endobj\n\
    2 0 obj << /Type /Page >> endobj\n3 0 obj << /Type /Page >> endobj\n%%EOF\n";
const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const DOCX_BYTES: &[u8] = b"PK\x03\x04....[Content_Types].xml....PK\x03\x04word/document.xml";
const STORE: &str = "0199aa11-1111-7111-8111-111111111111";
const STORE_DOCUMENT: &str = "0199aa11-2222-7222-8222-222222222222";

struct Fixture {
    service: DocumentService,
    database: TemporaryDatabase,
    folder: tempfile::TempDir,
    clock: Arc<FakeClock>,
    revealed: Arc<Mutex<Vec<PathBuf>>>,
}

fn fixture() -> Fixture {
    fixture_with(|_| {})
}

fn fixture_with(tune: impl FnOnce(&mut DocumentServiceOptions)) -> Fixture {
    let database = temporary_database();
    let folder = tempfile::tempdir().expect("a temporary folder");
    let clock = Arc::new(FakeClock::default());
    let revealed = Arc::new(Mutex::new(Vec::new()));
    let mut options = DocumentServiceOptions::new(
        database.shared(),
        InProcessCore::shared(),
        folder.path().join("cache").join("documents"),
    );
    options.clock = clock.clone();
    options.reveal = {
        let revealed = Arc::clone(&revealed);
        Arc::new(move |path: &Path| {
            revealed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(path.to_path_buf());
            Ok(())
        })
    };
    tune(&mut options);
    Fixture {
        service: DocumentService::new(options),
        database,
        folder,
        clock,
        revealed,
    }
}

fn path_ref(path: &Path) -> DocumentRef {
    DocumentRef::Path {
        path: path.to_string_lossy().into_owned(),
    }
}

fn canonical_text(path: &Path) -> String {
    canonical(path)
        .expect("a canonical path")
        .to_string_lossy()
        .into_owned()
}

impl Fixture {
    fn write(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.folder.path().join("files").join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
        std::fs::write(&path, bytes).expect("writes");
        path
    }

    fn cache(&self) -> DocumentCache {
        DocumentCache::new(self.folder.path().join("cache").join("documents"))
    }

    fn cache_dir(&self) -> PathBuf {
        self.folder.path().join("cache").join("documents")
    }

    fn cached(&self) -> Vec<CacheEntry> {
        self.cache().entries().expect("lists the cache")
    }

    async fn open(&self, path: &Path) -> Result<DocumentDto, AppError> {
        self.clock.advance(1000);
        self.service.open(path_ref(path)).await
    }

    fn seed_store_document(&self, path: &Path) {
        self.database
            .with_connection(|connection| {
                let provider = seed_embedding_provider(connection);
                let repositories = Repositories::new(connection);
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new(STORE, "K", &provider, "m", 3, 1))?;
                repositories
                    .vector_documents
                    .record_indexed(&VectorDocumentDraft {
                        id: STORE_DOCUMENT.to_owned(),
                        store_id: STORE.to_owned(),
                        source_path: path.to_string_lossy().into_owned(),
                        content_hash: "indexed".to_owned(),
                        chunk_count: 1,
                        bytes: 1,
                        indexed_at: 1,
                    })?;
                Ok(())
            })
            .expect("seeds a store document");
    }
}

fn citation(store: &str, source_path: &str, page: Option<u64>) -> ChatCitationDto {
    ChatCitationDto {
        store_id: VectorStoreId::parse(store).expect("uuid"),
        document_id: STORE_DOCUMENT.to_owned(),
        source_path: source_path.to_owned(),
        chunk_index: 0,
        score: 0.5,
        section: String::new(),
        page,
        related: None,
    }
}

async fn text_of(service: &DocumentService, document: &DocumentDto) -> Vec<(u64, String)> {
    let mut pages = Vec::new();
    for index in 0..document.part_count {
        match service.page(&document.id, index).await.expect("a page") {
            DocumentPageDto::Text {
                first_line,
                text,
                untrusted,
                ..
            } => {
                assert!(untrusted);
                pages.push((first_line, text));
            }
            DocumentPageDto::Bytes { .. } => panic!("a text document serves text"),
        }
    }
    pages
}

#[tokio::test]
async fn the_same_content_at_two_paths_is_one_cached_document() {
    let fixture = fixture();
    let first = fixture.write("report.pdf", PDF_BYTES);
    let second = fixture.write("copy/report-final.pdf", PDF_BYTES);

    let opened = fixture.open(&first).await.expect("opens");
    assert_eq!(opened.kind, DocumentKind::Pdf);
    assert_eq!(opened.mime_type, "application/pdf");
    assert_eq!(opened.page_count, Some(2));
    assert_eq!(opened.name, "report.pdf");
    assert!(opened.open);

    let again = fixture.open(&second).await.expect("opens the copy");
    assert_eq!(again.id, opened.id);
    assert_eq!(again.content_hash, opened.content_hash);
    assert_eq!(again.source_path, canonical_text(&second));
    assert_eq!(fixture.cached().len(), 1);
    assert_eq!(
        fixture.cached()[0].name,
        format!("{}.pdf", opened.content_hash)
    );
    assert_eq!(fixture.service.list().expect("lists").len(), 1);
}

#[tokio::test]
async fn a_moved_file_keeps_serving_from_the_cache() {
    let fixture = fixture();
    let path = fixture.write("report.pdf", PDF_BYTES);
    let opened = fixture.open(&path).await.expect("opens");
    std::fs::remove_file(&path).expect("moves the file away");

    let DocumentPageDto::Bytes {
        data, total_bytes, ..
    } = fixture.service.page(&opened.id, 0).await.expect("a page")
    else {
        panic!("a PDF serves bytes");
    };
    assert_eq!(STANDARD.decode(data).expect("base64"), PDF_BYTES);
    assert_eq!(total_bytes, PDF_BYTES.len() as u64);
}

#[tokio::test]
async fn pdf_bytes_are_served_in_ranges() {
    let fixture = fixture_with(|options| options.bytes_page_max = 32);
    let path = fixture.write("report.pdf", PDF_BYTES);
    let opened = fixture.open(&path).await.expect("opens");
    assert_eq!(opened.part_count, PDF_BYTES.len().div_ceil(32) as u64);

    let mut joined = Vec::new();
    for index in 0..opened.part_count {
        let DocumentPageDto::Bytes { offset, data, .. } = fixture
            .service
            .page(&opened.id, index)
            .await
            .expect("a part")
        else {
            panic!("a PDF serves bytes");
        };
        assert_eq!(offset, index * 32);
        joined.extend(STANDARD.decode(data).expect("base64"));
    }
    assert_eq!(joined, PDF_BYTES);
    let beyond = fixture
        .service
        .page(&opened.id, opened.part_count)
        .await
        .expect_err("past the end");
    assert_eq!(beyond.code, AppErrorCode::NotFound);
    assert_eq!(beyond.message, DOCUMENT_PAGE_NOT_FOUND);
}

#[tokio::test]
async fn a_mislabelled_file_fails_without_touching_the_cache() {
    let fixture = fixture();
    let fake = fixture.write("scan.pdf", PNG_BYTES);
    let error = fixture.open(&fake).await.expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(error.message, MISLABELLED_DOCUMENT);
    assert!(fixture.cached().is_empty());
    assert!(fixture.service.list().expect("lists").is_empty());

    let image = fixture.write("scan.png", PNG_BYTES);
    let opened = fixture.open(&image).await.expect("opens the image");
    assert_eq!(opened.kind, DocumentKind::Image);
    assert_eq!(opened.page_count, Some(1));
}

#[tokio::test]
async fn an_office_file_without_a_converter_opens_for_approximate_rendering() {
    let fixture = fixture();
    let docx = fixture.write("letter.docx", DOCX_BYTES);
    let opened = fixture.open(&docx).await.expect("opens");
    assert_eq!(opened.kind, DocumentKind::Office);
    assert_eq!(opened.rendering, DocumentRendering::Approximate);
    assert_eq!(opened.office_format, Some(DocumentOfficeFormat::Docx));
    assert_eq!(opened.page_count, None);
    assert_eq!(
        fixture
            .cached()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        [format!("{}.docx", opened.content_hash)]
    );
    let page = fixture.service.page(&opened.id, 0).await.expect("serves");
    assert!(
        matches!(page, DocumentPageDto::Bytes { total_bytes, .. } if total_bytes == DOCX_BYTES.len() as u64)
    );
}

#[tokio::test]
async fn an_unsupported_format_fails_cleanly() {
    let fixture = fixture();
    let binary = fixture.write("archive.bin", b"\0\x01\x02\x03");
    let error = fixture.open(&binary).await.expect_err("unknown binary");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert!(fixture.cached().is_empty());
    assert!(fixture.service.list().expect("lists").is_empty());
}

struct FakeConverter;

#[async_trait]
impl DocumentConverter for FakeConverter {
    async fn convert_to_pdf(
        &self,
        _source: &Path,
        format: OfficeFormat,
        target: &Path,
    ) -> Result<(), AppError> {
        assert_eq!(format, OfficeFormat::Docx);
        std::fs::write(target, PDF_BYTES).expect("writes the converted file");
        Ok(())
    }
}

#[derive(Default)]
struct CountingConverter {
    ready: std::sync::atomic::AtomicBool,
    calls: std::sync::atomic::AtomicUsize,
    fail: Option<AppErrorCode>,
}

impl CountingConverter {
    fn ready(fail: Option<AppErrorCode>) -> Arc<Self> {
        Arc::new(Self {
            ready: std::sync::atomic::AtomicBool::new(true),
            calls: std::sync::atomic::AtomicUsize::new(0),
            fail,
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl DocumentConverter for CountingConverter {
    async fn ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::SeqCst)
    }

    async fn convert_to_pdf(
        &self,
        _source: &Path,
        _format: OfficeFormat,
        target: &Path,
    ) -> Result<(), AppError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::fs::write(target, b"%PDF-partial").expect("writes a partial file");
        match self.fail {
            Some(code) => Err(AppError::new(code, "failed")),
            None => {
                std::fs::write(target, PDF_BYTES).expect("writes the converted file");
                Ok(())
            }
        }
    }
}

#[tokio::test]
async fn conversion_is_cached_by_source_hash_across_paths() {
    let converter = CountingConverter::ready(None);
    let shared = converter.clone();
    let fixture = fixture_with(move |options| options.converter = Some(shared));
    let first = fixture.write("a/letter.docx", DOCX_BYTES);
    let second = fixture.write("b/copy.docx", DOCX_BYTES);
    let opened = fixture.open(&first).await.expect("converts");
    let again = fixture.open(&second).await.expect("reuses");
    assert_eq!(converter.calls(), 1);
    assert_eq!(opened.id, again.id);
    assert_eq!(again.rendering, DocumentRendering::Converted);
    assert_eq!(again.office_format, Some(DocumentOfficeFormat::Docx));
}

#[tokio::test]
async fn a_converter_that_becomes_ready_replaces_the_approximate_copy() {
    let converter = Arc::new(CountingConverter::default());
    let shared = converter.clone();
    let fixture = fixture_with(move |options| options.converter = Some(shared));
    let docx = fixture.write("letter.docx", DOCX_BYTES);
    let approximate = fixture.open(&docx).await.expect("opens");
    assert_eq!(approximate.rendering, DocumentRendering::Approximate);
    assert_eq!(converter.calls(), 0);
    converter
        .ready
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let converted = fixture.open(&docx).await.expect("converts");
    assert_eq!(converted.id, approximate.id);
    assert_eq!(converted.kind, DocumentKind::Pdf);
    assert_eq!(
        fixture
            .cached()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        [format!("{}.pdf", converted.content_hash)]
    );
}

#[tokio::test]
async fn a_failed_or_cancelled_conversion_leaves_no_partial_file_in_the_cache() {
    for code in [AppErrorCode::UnsupportedFormat, AppErrorCode::RunCancelled] {
        let converter = CountingConverter::ready(Some(code));
        let fixture = fixture_with(move |options| options.converter = Some(converter));
        let docx = fixture.write("letter.docx", DOCX_BYTES);
        let error = fixture.open(&docx).await.expect_err("fails");
        assert_eq!(error.code, code);
        assert!(fixture.cached().is_empty());
        assert!(fixture.service.list().expect("lists").is_empty());
        let leftovers = std::fs::read_dir(fixture.cache_dir())
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftovers, 0);
    }
}

#[tokio::test]
async fn an_office_file_takes_the_pdf_path_through_the_converter() {
    let fixture = fixture_with(|options| options.converter = Some(Arc::new(FakeConverter)));
    let docx = fixture.write("letter.docx", DOCX_BYTES);
    let opened = fixture.open(&docx).await.expect("converts");
    assert_eq!(opened.kind, DocumentKind::Pdf);
    assert_eq!(opened.size_bytes, DOCX_BYTES.len() as u64);
    assert_eq!(opened.page_count, Some(2));
    assert_eq!(
        fixture
            .cached()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>(),
        [format!("{}.pdf", opened.content_hash)]
    );
}

#[tokio::test]
async fn eviction_never_removes_an_open_document() {
    let size = PDF_BYTES.len() as u64;
    let fixture = fixture_with(|options| options.cache_ceiling = size * 2);
    let kept = fixture.write("kept.pdf", PDF_BYTES);
    let kept = fixture.open(&kept).await.expect("opens");
    let mut variants = Vec::new();
    for index in 0..3 {
        let mut bytes = PDF_BYTES.to_vec();
        bytes.extend(format!("%{index}\n").as_bytes());
        variants.push(fixture.write(&format!("v{index}.pdf"), &bytes));
    }

    let first = fixture.open(&variants[0]).await.expect("opens");
    fixture.service.close(&first.id).expect("closes");
    let second = fixture.open(&variants[1]).await.expect("opens");
    let hashes: Vec<String> = fixture
        .cached()
        .into_iter()
        .map(|entry| entry.hash)
        .collect();
    assert!(hashes.contains(&kept.content_hash));
    assert!(hashes.contains(&second.content_hash));
    assert!(!hashes.contains(&first.content_hash));
    let listed: Vec<DocumentId> = fixture
        .service
        .list()
        .expect("lists")
        .into_iter()
        .map(|document| document.id)
        .collect();
    assert!(!listed.contains(&first.id));

    let third = fixture
        .open(&variants[2])
        .await
        .expect("opens over the ceiling");
    let hashes: Vec<String> = fixture
        .cached()
        .into_iter()
        .map(|entry| entry.hash)
        .collect();
    assert_eq!(hashes.len(), 3);
    for document in [&kept, &second, &third] {
        assert!(hashes.contains(&document.content_hash));
        fixture
            .service
            .page(&document.id, 0)
            .await
            .expect("an open document still serves");
    }

    fixture.service.close(&kept.id).expect("closes");
    fixture.service.close(&second.id).expect("closes");
    let reopened = fixture.open(&variants[1]).await.expect("reopens");
    assert_eq!(reopened.id, second.id);
    let hashes: Vec<String> = fixture
        .cached()
        .into_iter()
        .map(|entry| entry.hash)
        .collect();
    assert!(!hashes.contains(&kept.content_hash));
    assert!(hashes.contains(&second.content_hash));
    assert!(hashes.contains(&third.content_hash));
}

#[tokio::test]
async fn a_document_opened_twice_stays_open_until_both_close() {
    let size = PDF_BYTES.len() as u64;
    let fixture = fixture_with(|options| options.cache_ceiling = size);
    let path = fixture.write("a.pdf", PDF_BYTES);
    let first = fixture.open(&path).await.expect("opens");
    fixture.open(&path).await.expect("opens again");
    fixture.service.close(&first.id).expect("closes once");
    fixture
        .service
        .page(&first.id, 0)
        .await
        .expect("still open");
    fixture.service.close(&first.id).expect("closes twice");
    let error = fixture
        .service
        .page(&first.id, 0)
        .await
        .expect_err("closed");
    assert_eq!(error.message, DOCUMENT_NOT_OPEN);
    fixture
        .service
        .close(&first.id)
        .expect("closing again is harmless");
    let listed = fixture.service.list().expect("lists");
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].open);
}

#[tokio::test]
async fn oversized_text_is_paginated_on_lines() {
    let fixture = fixture_with(|options| options.text_page_max = 16);
    let content = "# Заголовок\nстрока один\nline two\nline three\n";
    let path = fixture.write("notes.md", content.as_bytes());
    let opened = fixture.open(&path).await.expect("opens");
    assert_eq!(opened.kind, DocumentKind::Markdown);
    assert!(opened.part_count > 1);
    assert_eq!(opened.page_count, Some(opened.part_count));

    let pages = text_of(&fixture.service, &opened).await;
    assert_eq!(
        pages
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<String>(),
        content
    );
    assert!(pages.iter().all(|(_, text)| text.len() <= 16));
    assert_eq!(pages[0].0, 1);
    assert!(pages.last().expect("a last page").0 > 1);
}

#[tokio::test]
async fn text_comes_back_with_its_encoding_and_language() {
    let fixture = fixture();
    let cp1251 = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2, b'\r', b'\n'];
    let path = fixture.write("legacy.txt", &cp1251);
    let opened = fixture.open(&path).await.expect("opens");
    let DocumentPageDto::Text { encoding, text, .. } =
        fixture.service.page(&opened.id, 0).await.expect("a page")
    else {
        panic!("text serves text");
    };
    assert_eq!(encoding, DocumentTextEncoding::Windows1251);
    assert_eq!(text, "Привет\n");

    let code = fixture.write("main.rs", b"fn main() {}\n");
    let opened = fixture.open(&code).await.expect("opens");
    assert_eq!(opened.kind, DocumentKind::Code);
    assert_eq!(opened.language.as_deref(), Some("rust"));
}

#[tokio::test]
async fn text_too_large_to_view_is_refused() {
    let fixture = fixture_with(|options| options.text_max = 8);
    let path = fixture.write("big.txt", b"0123456789");
    let error = fixture.open(&path).await.expect_err("too large");
    assert_eq!(error.code, AppErrorCode::LimitExceeded);
}

#[tokio::test]
async fn a_deleted_cache_is_rebuilt_from_an_unchanged_source() {
    let fixture = fixture();
    let path = fixture.write("notes.md", b"# Plan\n");
    let opened = fixture.open(&path).await.expect("opens");
    std::fs::remove_dir_all(fixture.cache().dir()).expect("deletes the cache");
    assert_eq!(text_of(&fixture.service, &opened).await[0].1, "# Plan\n");

    std::fs::remove_dir_all(fixture.cache().dir()).expect("deletes the cache");
    std::fs::write(&path, b"# Changed\n").expect("changes the source");
    let error = fixture
        .service
        .page(&opened.id, 0)
        .await
        .expect_err("the source changed");
    assert_eq!(error.message, DOCUMENT_SOURCE_CHANGED);
}

#[tokio::test]
async fn opens_by_store_document_and_by_citation_from_the_indexed_path() {
    let fixture = fixture();
    let indexed = fixture.write("store/guide.md", b"# Guide\n");
    let forged = fixture.write("elsewhere/secret.md", b"# Secret\n");
    fixture.seed_store_document(&indexed);

    let by_id = fixture
        .service
        .open(DocumentRef::StoreDocument {
            document_id: DocumentId::parse(STORE_DOCUMENT).expect("uuid"),
        })
        .await
        .expect("opens by store document");
    assert_eq!(by_id.source_path, canonical_text(&indexed));
    assert_eq!(by_id.initial_page, None);

    let by_citation = fixture
        .service
        .open(DocumentRef::Citation {
            citation: citation(STORE, &forged.to_string_lossy(), Some(3)),
        })
        .await
        .expect("opens by citation");
    assert_eq!(by_citation.id, by_id.id);
    assert_eq!(by_citation.source_path, canonical_text(&indexed));
    assert_eq!(by_citation.initial_page, Some(3));

    let error = fixture
        .service
        .open(DocumentRef::Citation {
            citation: citation("0199aa11-3333-7333-8333-333333333333", "", None),
        })
        .await
        .expect_err("another store");
    assert_eq!(error.code, AppErrorCode::NotFound);
}

#[tokio::test]
async fn path_opens_must_be_absolute_existing_and_allowed() {
    let blocked = tempfile::tempdir().expect("a temporary folder");
    let fixture = fixture_with(|options| {
        options.forbidden = Some(ForbiddenFolders::new(
            &[blocked.path().to_path_buf()],
            &[],
            Arc::new(Vec::new),
        ));
    });
    let relative = fixture
        .service
        .open(DocumentRef::Path {
            path: "notes.md".to_owned(),
        })
        .await
        .expect_err("relative");
    assert_eq!(relative.code, AppErrorCode::ValidationFailed);

    let missing = fixture
        .open(&fixture.folder.path().join("missing.pdf"))
        .await
        .expect_err("missing");
    assert_eq!(missing.code, AppErrorCode::NotFound);

    let inside = blocked.path().join("app.sqlite");
    std::fs::write(&inside, b"SQLite format 3\0").expect("writes");
    let refused = fixture.open(&inside).await.expect_err("forbidden");
    assert_eq!(refused.code, AppErrorCode::PermissionDenied);
}

#[tokio::test]
async fn reveal_shows_the_source_file() {
    let fixture = fixture();
    let path = fixture.write("report.pdf", PDF_BYTES);
    let opened = fixture.open(&path).await.expect("opens");
    fixture.service.reveal(&opened.id).expect("reveals");
    assert_eq!(
        *fixture
            .revealed
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
        [canonical(&path).expect("canonical")]
    );

    std::fs::remove_file(&path).expect("removes the source");
    let error = fixture.service.reveal(&opened.id).expect_err("gone");
    assert_eq!(error.code, AppErrorCode::NotFound);
    let unknown = fixture
        .service
        .reveal(&DocumentId::parse(STORE_DOCUMENT).expect("uuid"))
        .expect_err("unknown");
    assert_eq!(unknown.message, DOCUMENT_NOT_FOUND);
}
