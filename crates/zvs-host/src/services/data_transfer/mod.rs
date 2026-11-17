mod archive;
mod restore;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use zvs_contract::{
    AppError, AppErrorCode, TransferChunkInput, TransferExportDto, TransferImportDto,
    TransferImportInput, TransferPreviewDto, TransferRef, TransferSelection, TransferUploadInput,
};

use crate::data::Database;
use crate::data::repositories::data_transfer::{TransferRepository, invalid_package, text};
use crate::platform::clock::SharedClock;
use crate::platform::paths::StudioPaths;
use crate::platform::picker::SharedPicker;

use archive::{Member, Package};

pub fn io_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::NativeError,
        "Не удалось выполнить перенос данных",
    )
    .with_detail("cause", error.to_string())
}

pub fn failure(code: AppErrorCode, reason: &str) -> AppError {
    AppError::new(code, "Не удалось выполнить перенос данных").with_detail("reason", reason)
}

struct Upload {
    directory: tempfile::TempDir,
    file: File,
    expected: u64,
    received: u64,
    prepared: Option<(Package, String)>,
}

struct Inner {
    paths: StudioPaths,
    database: Arc<Database>,
    clock: SharedClock,
    uploads: Mutex<HashMap<String, Arc<Mutex<Upload>>>>,
    operation: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct DataTransferService {
    inner: Arc<Inner>,
}

impl DataTransferService {
    pub fn new(
        paths: StudioPaths,
        database: Arc<Database>,
        clock: SharedClock,
    ) -> Result<Self, AppError> {
        restore::recover(&paths, &database)?;
        Ok(Self {
            inner: Arc::new(Inner {
                paths,
                database,
                clock,
                uploads: Mutex::new(HashMap::new()),
                operation: tokio::sync::Mutex::new(()),
            }),
        })
    }

    pub async fn inventory(&self) -> Result<TransferPreviewDto, AppError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let categories = archive::all_categories();
            let records = inner.database.with_connection(|connection| {
                TransferRepository::new(connection).export(&categories)
            })?;
            let mut package = Package {
                format: "zvs-dbox".to_owned(),
                version: 1,
                producer_version: env!("CARGO_PKG_VERSION").to_owned(),
                created_at: inner.clock.now(),
                categories,
                records,
                files: Vec::new(),
                missing_files: false,
            };
            collect_files(&inner.paths, &mut package, None)?;
            Ok(package.preview())
        })
        .await
        .map_err(io_error)?
    }

    pub async fn export(
        &self,
        selection: TransferSelection,
        picker: Option<SharedPicker>,
    ) -> Result<Option<TransferExportDto>, AppError> {
        let _guard = self
            .inner
            .operation
            .try_lock()
            .map_err(|_| failure(AppErrorCode::Conflict, "busy"))?;
        let picker = picker.ok_or_else(|| failure(AppErrorCode::NativeError, "picker"))?;
        let name = format!(
            "zvs-{}.dbox",
            crate::platform::paths::utc_stamp(self.inner.clock.now())
        );
        let Some(path) = picker.save_as(&name).await? else {
            return Ok(None);
        };
        if !path.to_ascii_lowercase().ends_with(".dbox") {
            return Err(failure(AppErrorCode::ValidationFailed, "extension"));
        }
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let target = PathBuf::from(&path);
            let parent = target
                .parent()
                .ok_or_else(invalid_package)?
                .canonicalize()
                .map_err(io_error)?;
            let state_root = inner
                .paths
                .root
                .canonicalize()
                .unwrap_or_else(|_| inner.paths.root.clone());
            if target.starts_with(&inner.paths.root) || parent.starts_with(&state_root) {
                return Err(failure(AppErrorCode::ValidationFailed, "destination"));
            }
            let workspace = tempfile::tempdir().map_err(io_error)?;
            let blobs = workspace.path().join("blobs");
            fs::create_dir(&blobs).map_err(io_error)?;
            let package = inner.database.with_connection(|connection| {
                let repository = TransferRepository::new(connection);
                repository.check_idle()?;
                let mut package = Package {
                    format: "zvs-dbox".to_owned(),
                    version: 1,
                    producer_version: env!("CARGO_PKG_VERSION").to_owned(),
                    created_at: inner.clock.now(),
                    categories: selection.categories.clone(),
                    records: repository.export(&selection.categories)?,
                    files: Vec::new(),
                    missing_files: false,
                };
                collect_files(&inner.paths, &mut package, Some(&blobs))?;
                Ok(package)
            })?;
            archive::write(&package, &blobs, &target)?;
            Ok(Some(TransferExportDto { path }))
        })
        .await
        .map_err(io_error)?
    }

    pub fn begin(&self, input: TransferUploadInput) -> Result<TransferRef, AppError> {
        let mut uploads = self
            .inner
            .uploads
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if uploads.len() >= 2 {
            return Err(failure(AppErrorCode::Conflict, "busy"));
        }
        let directory = tempfile::tempdir().map_err(io_error)?;
        require_space(directory.path(), input.bytes)?;
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(directory.path().join("upload.dbox"))
            .map_err(io_error)?;
        let id = uuid::Uuid::new_v4().to_string();
        uploads.insert(
            id.clone(),
            Arc::new(Mutex::new(Upload {
                directory,
                file,
                expected: input.bytes,
                received: 0,
                prepared: None,
            })),
        );
        Ok(TransferRef { id })
    }

    pub fn append(&self, input: TransferChunkInput) -> Result<(), AppError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&input.data)
            .map_err(|_| invalid_package())?;
        if bytes.is_empty() || bytes.len() > 512 * 1024 {
            return Err(invalid_package());
        }
        let uploads = self
            .inner
            .uploads
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let upload = uploads
            .get(&input.id)
            .cloned()
            .ok_or_else(|| failure(AppErrorCode::NotFound, "session"))?;
        drop(uploads);
        let mut upload = upload
            .try_lock()
            .map_err(|_| failure(AppErrorCode::Conflict, "busy"))?;
        if upload.prepared.is_some()
            || upload.received != input.offset
            || upload.received + bytes.len() as u64 > upload.expected
        {
            return Err(invalid_package());
        }
        upload.file.write_all(&bytes).map_err(io_error)?;
        upload.received += bytes.len() as u64;
        Ok(())
    }

    pub async fn inspect(&self, input: TransferRef) -> Result<TransferPreviewDto, AppError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let uploads = inner.uploads.lock().unwrap_or_else(PoisonError::into_inner);
            let upload = uploads
                .get(&input.id)
                .cloned()
                .ok_or_else(|| failure(AppErrorCode::NotFound, "session"))?;
            drop(uploads);
            let mut upload = upload.lock().unwrap_or_else(PoisonError::into_inner);
            if upload.received != upload.expected {
                return Err(invalid_package());
            }
            if let Some((package, hash)) = &upload.prepared {
                return import_preview(&inner.database, package, hash);
            }
            upload.file.flush().map_err(io_error)?;
            let prepared = archive::read(
                &upload.directory.path().join("upload.dbox"),
                &upload.directory.path().join("blobs"),
            )?;
            let preview = import_preview(&inner.database, &prepared.0, &prepared.1)?;
            upload.prepared = Some(prepared);
            Ok(preview)
        })
        .await
        .map_err(io_error)?
    }

    pub async fn import(&self, input: TransferImportInput) -> Result<TransferImportDto, AppError> {
        let _guard = self
            .inner
            .operation
            .try_lock()
            .map_err(|_| failure(AppErrorCode::Conflict, "busy"))?;
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let uploads = inner.uploads.lock().unwrap_or_else(PoisonError::into_inner);
            let upload = uploads
                .get(&input.id)
                .cloned()
                .ok_or_else(|| failure(AppErrorCode::NotFound, "session"))?;
            drop(uploads);
            let upload = upload.lock().unwrap_or_else(PoisonError::into_inner);
            let (package, hash) = upload.prepared.as_ref().ok_or_else(invalid_package)?;
            let result = restore::restore(
                &inner.paths,
                &inner.database,
                inner.clock.now(),
                package,
                hash,
                &upload.directory.path().join("blobs"),
                &input.categories,
            )?;
            inner
                .uploads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&input.id);
            Ok(result)
        })
        .await
        .map_err(io_error)?
    }

    pub fn dispose(&self, input: TransferRef) {
        self.inner
            .uploads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&input.id);
    }
}

fn add_file(
    paths: &StudioPaths,
    package: &mut Package,
    category: zvs_contract::TransferCategory,
    owner: &str,
    source: &Path,
    relative: String,
    blobs: Option<&Path>,
) -> Result<(), AppError> {
    if !archive::safe_relative(&relative) {
        return Err(invalid_package());
    }
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(_) if blobs.is_none() => {
            package.missing_files = true;
            return Ok(());
        }
        Err(_) => return Err(failure(AppErrorCode::NotFound, "missingFiles")),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid_package());
    }
    let (hash, bytes) = ("0".repeat(64), metadata.len());
    package.files.push(Member {
        category,
        owner: owner.to_owned(),
        path: relative,
        original: source.to_string_lossy().into_owned(),
        stored_original: paths.stored_paths().encode(source),
        hash,
        bytes,
    });
    if package.files.len() > archive::MAX_FILES {
        return Err(invalid_package());
    }
    Ok(())
}

fn tree(
    paths: &StudioPaths,
    package: &mut Package,
    category: zvs_contract::TransferCategory,
    owner: &str,
    base: &Path,
    directory: &Path,
    blobs: Option<&Path>,
) -> Result<(), AppError> {
    if directory
        .strip_prefix(base)
        .map_err(io_error)?
        .components()
        .count()
        > 64
    {
        return Err(invalid_package());
    }
    if fs::symlink_metadata(directory).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(invalid_package());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(error)),
    };
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        let kind = entry.file_type().map_err(io_error)?;
        if kind.is_symlink() {
            return Err(invalid_package());
        }
        if kind.is_dir() {
            tree(paths, package, category, owner, base, &entry.path(), blobs)?;
        } else if kind.is_file() {
            let relative = entry
                .path()
                .strip_prefix(base)
                .map_err(io_error)?
                .to_string_lossy()
                .replace('\\', "/");
            add_file(
                paths,
                package,
                category,
                owner,
                &entry.path(),
                relative,
                blobs,
            )?;
        }
    }
    Ok(())
}

fn collect_files(
    paths: &StudioPaths,
    package: &mut Package,
    blobs: Option<&Path>,
) -> Result<(), AppError> {
    use zvs_contract::TransferCategory::{Conversations, Skills, Stores};
    for record in package.records.clone() {
        match record.kind.as_str() {
            "conversation" => {
                let id = text(&record, "id")?;
                let base = paths.conversations_dir.join(id);
                for directory in ["attachments", "work", "outputs", "notes"] {
                    tree(
                        paths,
                        package,
                        Conversations,
                        id,
                        &base,
                        &base.join(directory),
                        blobs,
                    )?;
                }
            }
            "skill" => {
                let base = paths.stored_paths().decode(text(&record, "path")?);
                let canonical = match base.canonicalize() {
                    Ok(path) => path,
                    Err(_) => {
                        package.missing_files = true;
                        continue;
                    }
                };
                let library = paths.skills_dir.canonicalize().map_err(io_error)?;
                if !canonical.starts_with(library) {
                    return Err(invalid_package());
                }
                tree(
                    paths,
                    package,
                    Skills,
                    text(&record, "id")?,
                    &base,
                    &base,
                    blobs,
                )?;
            }
            "vector_document" => {
                let source = paths.stored_paths().decode(text(&record, "source_path")?);
                let name = source
                    .file_name()
                    .ok_or_else(invalid_package)?
                    .to_string_lossy();
                add_file(
                    paths,
                    package,
                    Stores,
                    text(&record, "store_id")?,
                    &source,
                    format!("{}/{name}", text(&record, "id")?),
                    blobs,
                )?;
            }
            _ => {}
        }
    }
    if let Some(blobs) = blobs {
        if package.missing_files {
            return Err(failure(AppErrorCode::NotFound, "missingFiles"));
        }
        let total = package
            .files
            .iter()
            .try_fold(0_u64, |sum, member| sum.checked_add(member.bytes))
            .ok_or_else(invalid_package)?;
        if total > archive::MAX_BYTES {
            return Err(failure(AppErrorCode::ValidationFailed, "size"));
        }
        require_space(blobs, total)?;
        for member in &mut package.files {
            let (hash, bytes) = archive::snapshot(Path::new(&member.original), blobs)?;
            if bytes != member.bytes {
                return Err(failure(AppErrorCode::Conflict, "changed"));
            }
            member.hash = hash;
        }
    }
    Ok(())
}

pub fn require_space(path: &Path, bytes: u64) -> Result<(), AppError> {
    let free = crate::platform::disk::free_bytes(path).map_err(io_error)?;
    if free < bytes.saturating_add(16 * 1024 * 1024) {
        return Err(failure(AppErrorCode::NativeError, "space"));
    }
    Ok(())
}

fn import_preview(
    database: &Database,
    package: &Package,
    hash: &str,
) -> Result<TransferPreviewDto, AppError> {
    database.with_connection(|connection| {
        let repository = TransferRepository::new(connection);
        let mut preview = package.preview();
        preview.blocked_categories = repository.blocked_categories(&package.records)?;
        for category in &package.categories {
            if repository.imported(hash, *category)? {
                preview.imported_categories.push(*category);
            }
        }
        Ok(preview)
    })
}
