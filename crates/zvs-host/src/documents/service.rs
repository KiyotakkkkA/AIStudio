use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, ChatCitationDto, DOCUMENT_BYTES_PAGE_MAX, DOCUMENT_TEXT_PAGE_MAX,
    DocumentDto, DocumentId, DocumentKind, DocumentOfficeFormat, DocumentPageDto, DocumentRef,
    DocumentRendering, Timestamp,
};

use super::cache::{DocumentCache, cache_file_name, eviction_plan};
use super::kind::{Detected, Detection, OfficeFormat, code_language, detect, pdf_page_estimate};
use super::text::{decode, first_line, paginate};
use crate::data::{Database, Repositories, schema::DocumentRow};
use crate::drivers::core::SharedTextCore;
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::external::{PathRevealer, system_path_revealer};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::platform::long_path::extended;
use crate::services::stored::{stored_id, unsigned};
use crate::workspace::{ForbiddenFolders, canonical};

pub const DOCUMENT_CACHE_CEILING: u64 = 2 * 1024 * 1024 * 1024;
pub const DOCUMENT_OPEN_MAX: u64 = 512 * 1024 * 1024;
pub const DOCUMENT_TEXT_MAX: u64 = 32 * 1024 * 1024;

pub const DOCUMENT_NOT_FOUND: &str = "Документ не найден";
pub const DOCUMENT_NOT_OPEN: &str = "Документ не открыт";
pub const DOCUMENT_PAGE_NOT_FOUND: &str = "Страница документа не найдена";
pub const DOCUMENT_FILE_NOT_FOUND: &str = "Файл не найден";
pub const DOCUMENT_SOURCE_CHANGED: &str = "Файл документа изменён или удалён";
pub const DOCUMENT_NOT_A_FILE: &str = "Путь не является файлом";
pub const DOCUMENT_INVALID_PATH: &str = "Некорректный путь к файлу";
pub const DOCUMENT_FORBIDDEN: &str = "Файл находится в папке, закрытой для доступа";
pub const DOCUMENT_TOO_LARGE: &str = "Файл слишком большой для просмотра";
pub const DOCUMENT_UNREADABLE: &str = "Не удалось прочитать файл";
pub const DOCUMENT_CACHE_FAILED: &str = "Не удалось подготовить документ к просмотру";
pub const STORE_DOCUMENT_NOT_FOUND: &str = "Документ хранилища не найден";
pub const CONVERSION_UNAVAILABLE: &str = "Преобразование документов в PDF недоступно: подключите LibreOffice в разделе «Расширения → Компоненты»";

fn conversion_unavailable(format: OfficeFormat) -> AppError {
    AppError::new(AppErrorCode::UnsupportedFormat, CONVERSION_UNAVAILABLE)
        .with_detail("format", format.extension())
}

#[async_trait]
pub trait DocumentConverter: Send + Sync {
    async fn ready(&self) -> bool {
        true
    }

    async fn convert_to_pdf(
        &self,
        source: &Path,
        format: OfficeFormat,
        target: &Path,
    ) -> Result<(), AppError>;
}

pub type SharedDocumentConverter = Arc<dyn DocumentConverter>;

pub struct DocumentServiceOptions {
    pub database: Arc<Database>,
    pub text: SharedTextCore,
    pub cache_dir: PathBuf,
    pub cache_ceiling: u64,
    pub open_max: u64,
    pub text_max: u64,
    pub text_page_max: usize,
    pub bytes_page_max: usize,
    pub forbidden: Option<ForbiddenFolders>,
    pub converter: Option<SharedDocumentConverter>,
    pub reveal: PathRevealer,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl DocumentServiceOptions {
    pub fn new(database: Arc<Database>, text: SharedTextCore, cache_dir: PathBuf) -> Self {
        Self {
            database,
            text,
            cache_dir,
            cache_ceiling: DOCUMENT_CACHE_CEILING,
            open_max: DOCUMENT_OPEN_MAX,
            text_max: DOCUMENT_TEXT_MAX,
            text_page_max: DOCUMENT_TEXT_PAGE_MAX,
            bytes_page_max: DOCUMENT_BYTES_PAGE_MAX,
            forbidden: None,
            converter: None,
            reveal: system_path_revealer(),
            clock: Arc::new(SystemClock),
            logger: silent_logger(),
        }
    }
}

struct OpenEntry {
    hash: String,
    count: usize,
}

struct Inner {
    options: DocumentServiceOptions,
    cache: DocumentCache,
    open: Mutex<HashMap<String, OpenEntry>>,
}

#[derive(Clone)]
pub struct DocumentService {
    inner: Arc<Inner>,
}

struct Prepared {
    kind: DocumentKind,
    mime_type: &'static str,
    office_format: Option<DocumentOfficeFormat>,
    cached_file: String,
    page_count: Option<u64>,
    part_count: u64,
    copy_source: bool,
}

fn rendering(kind: DocumentKind, office_format: Option<DocumentOfficeFormat>) -> DocumentRendering {
    match (kind, office_format) {
        (_, None) => DocumentRendering::Native,
        (DocumentKind::Office, Some(_)) => DocumentRendering::Approximate,
        (_, Some(_)) => DocumentRendering::Converted,
    }
}

fn not_found(message: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, message)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn io_failure(path: &Path, error: &std::io::Error) -> AppError {
    let (code, message) = match error.kind() {
        std::io::ErrorKind::NotFound => (AppErrorCode::NotFound, DOCUMENT_FILE_NOT_FOUND),
        std::io::ErrorKind::PermissionDenied => {
            (AppErrorCode::PermissionDenied, DOCUMENT_UNREADABLE)
        }
        _ => (AppErrorCode::ValidationFailed, DOCUMENT_UNREADABLE),
    };
    AppError::new(code, message).with_detail("path", path_text(path))
}

fn cache_failure(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, DOCUMENT_CACHE_FAILED)
        .with_detail("raw", error.to_string())
}

fn part_count_of(size: u64, page: usize) -> u64 {
    size.div_ceil(u64::try_from(page).unwrap_or(u64::MAX))
        .max(1)
}

fn count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_owned())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tokio::task::spawn_blocking(work).await.map_err(|error| {
        AppError::new(AppErrorCode::Unknown, DOCUMENT_UNREADABLE)
            .with_detail("raw", error.to_string())
    })?
}

fn read_source(path: PathBuf, max: u64) -> Result<Vec<u8>, AppError> {
    let metadata = std::fs::metadata(extended(&path)).map_err(|error| io_failure(&path, &error))?;
    if !metadata.is_file() {
        return Err(
            AppError::new(AppErrorCode::ValidationFailed, DOCUMENT_NOT_A_FILE)
                .with_detail("path", path_text(&path)),
        );
    }
    if metadata.len() > max {
        return Err(
            AppError::new(AppErrorCode::LimitExceeded, DOCUMENT_TOO_LARGE)
                .with_detail("sizeBytes", metadata.len())
                .with_detail("maxBytes", max),
        );
    }
    std::fs::read(extended(&path)).map_err(|error| io_failure(&path, &error))
}

impl DocumentService {
    pub fn new(options: DocumentServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                cache: DocumentCache::new(options.cache_dir.clone()),
                options,
                open: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn options(&self) -> &DocumentServiceOptions {
        &self.inner.options
    }

    fn open_state(&self) -> MutexGuard<'_, HashMap<String, OpenEntry>> {
        self.inner
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn read<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, rusqlite::Error>,
    ) -> Result<T, AppError> {
        self.options()
            .database
            .with_connection(|connection| Ok(work(&Repositories::new(connection))?))
    }

    fn is_open(&self, id: &str) -> bool {
        self.open_state().contains_key(id)
    }

    fn release(&self, id: &str) {
        let mut open = self.open_state();
        if let Some(entry) = open.get_mut(id) {
            entry.count = entry.count.saturating_sub(1);
            if entry.count == 0 {
                open.remove(id);
            }
        }
    }

    fn store_document(&self, id: &str) -> Result<(String, String), AppError> {
        self.read(|repositories| repositories.vector_documents.find_by_id(id))?
            .map(|row| (row.store_id, row.source_path))
            .ok_or_else(|| not_found(STORE_DOCUMENT_NOT_FOUND).with_detail("documentId", id))
    }

    fn resolve(&self, reference: &DocumentRef) -> Result<(PathBuf, Option<u64>), AppError> {
        match reference {
            DocumentRef::Path { path } => {
                let requested = PathBuf::from(path);
                if !requested.is_absolute() {
                    return Err(AppError::new(
                        AppErrorCode::ValidationFailed,
                        DOCUMENT_INVALID_PATH,
                    )
                    .with_detail("path", path.clone()));
                }
                let resolved =
                    canonical(&requested).map_err(|error| io_failure(&requested, &error))?;
                let folder = resolved.parent().unwrap_or(&resolved);
                if self
                    .options()
                    .forbidden
                    .as_ref()
                    .is_some_and(|forbidden| forbidden.refuses(folder))
                {
                    return Err(
                        AppError::new(AppErrorCode::PermissionDenied, DOCUMENT_FORBIDDEN)
                            .with_detail("path", path_text(&resolved)),
                    );
                }
                Ok((resolved, None))
            }
            DocumentRef::Citation { citation } => self.resolve_citation(citation),
            DocumentRef::StoreDocument { document_id } => {
                let (_, source) = self.store_document(&document_id.to_string())?;
                Ok((self.indexed_path(&source)?, None))
            }
        }
    }

    fn resolve_citation(
        &self,
        citation: &ChatCitationDto,
    ) -> Result<(PathBuf, Option<u64>), AppError> {
        let (store_id, source) = self.store_document(&citation.document_id)?;
        if store_id != citation.store_id.to_string() {
            return Err(not_found(STORE_DOCUMENT_NOT_FOUND)
                .with_detail("documentId", citation.document_id.clone()));
        }
        Ok((self.indexed_path(&source)?, citation.page))
    }

    fn indexed_path(&self, source: &str) -> Result<PathBuf, AppError> {
        let path = PathBuf::from(source);
        canonical(&path).map_err(|error| io_failure(&path, &error))
    }

    async fn prepare(
        &self,
        source: &Path,
        hash: &str,
        bytes: &[u8],
        detection: Detection,
    ) -> Result<Prepared, AppError> {
        match detection {
            Detection::Native(detected) => self.prepare_native(hash, bytes, detected),
            Detection::Office(format) => self.prepare_office(source, hash, bytes, format).await,
        }
    }

    fn prepare_native(
        &self,
        hash: &str,
        bytes: &[u8],
        detected: Detected,
    ) -> Result<Prepared, AppError> {
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let (page_count, part_count) = match detected.kind {
            DocumentKind::Pdf => (
                pdf_page_estimate(bytes),
                part_count_of(size, self.options().bytes_page_max),
            ),
            DocumentKind::Image => (Some(1), part_count_of(size, self.options().bytes_page_max)),
            DocumentKind::Office => (None, part_count_of(size, self.options().bytes_page_max)),
            DocumentKind::Markdown
            | DocumentKind::Code
            | DocumentKind::Csv
            | DocumentKind::Text => {
                if size > self.options().text_max {
                    return Err(
                        AppError::new(AppErrorCode::LimitExceeded, DOCUMENT_TOO_LARGE)
                            .with_detail("sizeBytes", size)
                            .with_detail("maxBytes", self.options().text_max),
                    );
                }
                let decoded = decode(bytes);
                let pages = paginate(&decoded.text, self.options().text_page_max).len();
                let pages = u64::try_from(pages).unwrap_or(u64::MAX);
                (Some(pages), pages)
            }
        };
        Ok(Prepared {
            kind: detected.kind,
            mime_type: detected.mime_type,
            office_format: None,
            cached_file: cache_file_name(hash, detected.extension),
            page_count,
            part_count,
            copy_source: true,
        })
    }

    async fn ready_converter(&self) -> Option<SharedDocumentConverter> {
        let converter = self.options().converter.clone()?;
        converter.ready().await.then_some(converter)
    }

    async fn prepare_office(
        &self,
        source: &Path,
        hash: &str,
        bytes: &[u8],
        format: OfficeFormat,
    ) -> Result<Prepared, AppError> {
        let cached_file = cache_file_name(hash, "pdf");
        if !self.inner.cache.contains(&cached_file) {
            let Some(converter) = self.ready_converter().await else {
                return Ok(Self::approximate(
                    hash,
                    bytes,
                    format,
                    self.options().bytes_page_max,
                ));
            };
            self.convert(&converter, source, format, &cached_file)
                .await?;
            let stale = cache_file_name(hash, format.extension());
            if self.inner.cache.contains(&stale) {
                let _ = self.inner.cache.remove(&stale);
            }
        }
        self.converted(cached_file, format).await
    }

    fn approximate(hash: &str, bytes: &[u8], format: OfficeFormat, page_max: usize) -> Prepared {
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        Prepared {
            kind: DocumentKind::Office,
            mime_type: format.mime_type(),
            office_format: Some(format.wire()),
            cached_file: cache_file_name(hash, format.extension()),
            page_count: None,
            part_count: part_count_of(size, page_max),
            copy_source: true,
        }
    }

    async fn convert(
        &self,
        converter: &SharedDocumentConverter,
        source: &Path,
        format: OfficeFormat,
        cached_file: &str,
    ) -> Result<(), AppError> {
        std::fs::create_dir_all(extended(self.inner.cache.dir()))
            .map_err(|error| cache_failure(&error))?;
        let temporary = self.inner.cache.temporary(cached_file);
        let converted = converter.convert_to_pdf(source, format, &temporary).await;
        if let Err(error) = converted {
            let _ = std::fs::remove_file(extended(&temporary));
            return Err(error);
        }
        self.inner
            .cache
            .commit(&temporary, cached_file)
            .map_err(|error| cache_failure(&error))
    }

    async fn converted(
        &self,
        cached_file: String,
        format: OfficeFormat,
    ) -> Result<Prepared, AppError> {
        let path = self.inner.cache.path(&cached_file);
        let converted =
            blocking(move || std::fs::read(extended(&path)).map_err(|error| cache_failure(&error)))
                .await?;
        let size = u64::try_from(converted.len()).unwrap_or(u64::MAX);
        Ok(Prepared {
            kind: DocumentKind::Pdf,
            mime_type: "application/pdf",
            office_format: Some(format.wire()),
            cached_file,
            page_count: pdf_page_estimate(&converted),
            part_count: part_count_of(size, self.options().bytes_page_max),
            copy_source: false,
        })
    }

    fn record(
        &self,
        source: &Path,
        hash: &str,
        size: u64,
        prepared: &Prepared,
    ) -> Result<DocumentRow, AppError> {
        let now = self.options().clock.now();
        let mut open = self.open_state();
        let row = self.read(|repositories| {
            repositories.documents.record_opened(&DocumentRow {
                id: new_id().hyphenated().to_string(),
                content_hash: hash.to_owned(),
                source_path: path_text(source),
                kind: prepared.kind,
                mime_type: prepared.mime_type.to_owned(),
                office_format: prepared.office_format,
                page_count: prepared.page_count.map(count),
                part_count: count(prepared.part_count),
                size_bytes: count(size),
                cached_file: prepared.cached_file.clone(),
                opened_at: now,
            })
        })?;
        open.entry(row.id.clone())
            .or_insert_with(|| OpenEntry {
                hash: row.content_hash.clone(),
                count: 0,
            })
            .count += 1;
        Ok(row)
    }

    async fn materialize(&self, name: &str, bytes: Vec<u8>) -> Result<(), AppError> {
        if self.inner.cache.contains(name) {
            return Ok(());
        }
        let cache = self.inner.cache.clone();
        let name = name.to_owned();
        blocking(move || {
            cache
                .store(&name, &bytes)
                .map_err(|error| cache_failure(&error))
        })
        .await
    }

    pub async fn open(&self, reference: DocumentRef) -> Result<DocumentDto, AppError> {
        let (source, initial_page) = self.resolve(&reference)?;
        let max = self.options().open_max;
        let bytes = blocking({
            let source = source.clone();
            move || read_source(source, max)
        })
        .await?;
        let detection = detect(&source, &bytes)?;
        let hash = self.options().text.hash(bytes.clone()).await?;
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let prepared = self.prepare(&source, &hash, &bytes, detection).await?;
        let row = self.record(&source, &hash, size, &prepared)?;
        if prepared.copy_source
            && let Err(error) = self.materialize(&row.cached_file, bytes).await
        {
            self.release(&row.id);
            return Err(error);
        }
        self.evict();
        self.dto(&row, true, initial_page)
    }

    pub fn close(&self, id: &DocumentId) -> Result<(), AppError> {
        let key = id.to_string();
        if self.is_open(&key) {
            self.release(&key);
            return Ok(());
        }
        self.read(|repositories| repositories.documents.find_by_id(&key))?
            .map(|_| ())
            .ok_or_else(|| not_found(DOCUMENT_NOT_FOUND))
    }

    pub fn list(&self) -> Result<Vec<DocumentDto>, AppError> {
        let rows = self.read(|repositories| repositories.documents.list())?;
        let open: HashSet<String> = self.open_state().keys().cloned().collect();
        rows.iter()
            .map(|row| self.dto(row, open.contains(&row.id), None))
            .collect()
    }

    pub fn reveal(&self, id: &DocumentId) -> Result<(), AppError> {
        let row = self.row(id)?;
        let source = PathBuf::from(&row.source_path);
        let resolved = canonical(&source).map_err(|error| io_failure(&source, &error))?;
        (self.options().reveal)(&resolved)
    }

    fn row(&self, id: &DocumentId) -> Result<DocumentRow, AppError> {
        self.read(|repositories| repositories.documents.find_by_id(&id.to_string()))?
            .ok_or_else(|| not_found(DOCUMENT_NOT_FOUND))
    }

    pub async fn page(&self, id: &DocumentId, index: u64) -> Result<DocumentPageDto, AppError> {
        let row = self.row(id)?;
        if !self.is_open(&row.id) {
            return Err(not_found(DOCUMENT_NOT_OPEN));
        }
        let part_count = unsigned(row.part_count)?;
        if index >= part_count {
            return Err(not_found(DOCUMENT_PAGE_NOT_FOUND)
                .with_detail("index", index)
                .with_detail("partCount", part_count));
        }
        let path = self.cached(&row).await?;
        if row.kind.is_text() {
            let page_max = self.options().text_page_max;
            blocking(move || text_page(&path, index, part_count, page_max)).await
        } else {
            let page_max = self.options().bytes_page_max;
            blocking(move || bytes_page(&path, index, part_count, page_max)).await
        }
    }

    async fn cached(&self, row: &DocumentRow) -> Result<PathBuf, AppError> {
        let path = self.inner.cache.path(&row.cached_file);
        if self.inner.cache.contains(&row.cached_file) {
            return Ok(path);
        }
        let source = PathBuf::from(&row.source_path);
        let max = self.options().open_max;
        let changed =
            || not_found(DOCUMENT_SOURCE_CHANGED).with_detail("path", row.source_path.clone());
        let bytes = blocking({
            let source = source.clone();
            move || read_source(source, max)
        })
        .await
        .map_err(|_| changed())?;
        if self.options().text.hash(bytes.clone()).await? != row.content_hash {
            return Err(changed());
        }
        match detect(&source, &bytes).map_err(|_| changed())? {
            Detection::Office(format) if row.kind == DocumentKind::Pdf => {
                let converter = self
                    .ready_converter()
                    .await
                    .ok_or_else(|| conversion_unavailable(format))?;
                self.convert(&converter, &source, format, &row.cached_file)
                    .await?;
            }
            Detection::Native(_) | Detection::Office(_) => {
                self.materialize(&row.cached_file, bytes).await?;
            }
        }
        Ok(path)
    }

    fn evict(&self) {
        let open = self.open_state();
        let protected: HashSet<String> = open.values().map(|entry| entry.hash.clone()).collect();
        let outcome = self
            .inner
            .cache
            .entries()
            .map_err(|error| cache_failure(&error))
            .and_then(|entries| {
                let last_opened: HashMap<String, Timestamp> = self
                    .read(|repositories| repositories.documents.list())?
                    .into_iter()
                    .map(|row| (row.content_hash, row.opened_at))
                    .collect();
                let plan = eviction_plan(
                    &entries,
                    &last_opened,
                    &protected,
                    self.options().cache_ceiling,
                );
                for entry in plan {
                    self.inner
                        .cache
                        .remove(&entry.name)
                        .map_err(|error| cache_failure(&error))?;
                    self.read(|repositories| repositories.documents.remove_by_hash(&entry.hash))?;
                }
                Ok(())
            });
        drop(open);
        if let Err(error) = outcome {
            self.options().logger.log(
                LogLevel::Warn,
                "documents",
                "Could not evict cached documents",
                &[("error", json!(error.message))],
            );
        }
    }

    fn dto(
        &self,
        row: &DocumentRow,
        open: bool,
        initial_page: Option<u64>,
    ) -> Result<DocumentDto, AppError> {
        Ok(DocumentDto {
            id: stored_id(DocumentId::parse, &row.id)?,
            content_hash: row.content_hash.clone(),
            name: file_name(&row.source_path),
            source_path: row.source_path.clone(),
            kind: row.kind,
            mime_type: row.mime_type.clone(),
            rendering: rendering(row.kind, row.office_format),
            office_format: row.office_format,
            language: (row.kind == DocumentKind::Code)
                .then(|| code_language(Path::new(&row.source_path)))
                .flatten()
                .map(str::to_owned),
            size_bytes: unsigned(row.size_bytes)?,
            page_count: row.page_count.map(unsigned).transpose()?,
            part_count: unsigned(row.part_count)?,
            initial_page,
            open,
            opened_at: row.opened_at,
        })
    }
}

fn text_page(
    path: &Path,
    index: u64,
    part_count: u64,
    page_max: usize,
) -> Result<DocumentPageDto, AppError> {
    let bytes = std::fs::read(extended(path)).map_err(|error| cache_failure(&error))?;
    let decoded = decode(&bytes);
    let pages = paginate(&decoded.text, page_max);
    let range = usize::try_from(index)
        .ok()
        .and_then(|index| pages.get(index))
        .cloned()
        .ok_or_else(|| not_found(DOCUMENT_PAGE_NOT_FOUND))?;
    Ok(DocumentPageDto::Text {
        index,
        part_count,
        first_line: first_line(&decoded.text, range.start),
        encoding: decoded.encoding,
        text: decoded.text[range].to_owned(),
        untrusted: true,
    })
}

fn bytes_page(
    path: &Path,
    index: u64,
    part_count: u64,
    page_max: usize,
) -> Result<DocumentPageDto, AppError> {
    let mut file = std::fs::File::open(extended(path)).map_err(|error| cache_failure(&error))?;
    let total_bytes = file
        .metadata()
        .map_err(|error| cache_failure(&error))?
        .len();
    let page = u64::try_from(page_max).unwrap_or(u64::MAX);
    let offset = index.saturating_mul(page).min(total_bytes);
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| cache_failure(&error))?;
    let mut buffer =
        Vec::with_capacity(page_max.min(usize::try_from(total_bytes - offset).unwrap_or(page_max)));
    file.take(page)
        .read_to_end(&mut buffer)
        .map_err(|error| cache_failure(&error))?;
    Ok(DocumentPageDto::Bytes {
        index,
        part_count,
        offset,
        total_bytes,
        data: STANDARD.encode(&buffer),
    })
}
