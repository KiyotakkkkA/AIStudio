use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AddVectorSourceInput, AppError, AppErrorCode, DocumentId, VectorDocumentDto, VectorSourceDto,
    VectorStoreId,
};

use super::stored::{stored_id, unsigned};
use super::vector_stores::VectorStoreService;
use crate::data::repositories::VectorSourceDraft;
use crate::data::schema::{VectorDocumentRow, VectorSourceRow};
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};
pub use crate::platform::long_path::from_extended_path;

pub const DEFAULT_ANN_THRESHOLD: usize = 100_000;
pub const SOURCE_NOT_FOUND: &str = "Источник не найден";
pub const DOCUMENT_NOT_FOUND: &str = "Документ не найден";

pub fn resolve_path(path: &str, working_directory: &Path) -> PathBuf {
    let candidate = Path::new(path);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        working_directory.join(candidate)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if resolved.parent().is_some() {
                    resolved.pop();
                }
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved
}

pub fn source_dto(row: &VectorSourceRow) -> Result<VectorSourceDto, AppError> {
    Ok(VectorSourceDto {
        id: row.id.clone(),
        store_id: stored_id(VectorStoreId::parse, &row.store_id)?,
        kind: row.kind,
        path: row.path.clone(),
        include: row.include.clone(),
        exclude: row.exclude.clone(),
        recursive: row.recursive,
        created_at: row.created_at,
    })
}

pub fn document_dto(row: &VectorDocumentRow) -> Result<VectorDocumentDto, AppError> {
    Ok(VectorDocumentDto {
        id: stored_id(DocumentId::parse, &row.id)?,
        store_id: stored_id(VectorStoreId::parse, &row.store_id)?,
        source_path: row.source_path.clone(),
        content_hash: row.content_hash.clone(),
        chunk_count: unsigned(row.chunk_count)?,
        bytes: unsigned(row.bytes)?,
        indexed_at: row.indexed_at,
    })
}

pub fn describe(error: &AppError) -> String {
    match error
        .details
        .as_ref()
        .and_then(|details| details.get("detail"))
        .and_then(Value::as_str)
    {
        Some(detail) if !detail.is_empty() => format!("{}: {detail}", error.message),
        _ => error.message.clone(),
    }
}

pub struct VectorSourceServiceOptions {
    pub database: Arc<Database>,
    pub stores: VectorStoreService,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub working_directory: PathBuf,
    pub ann_threshold: usize,
}

impl VectorSourceServiceOptions {
    pub fn new(
        database: Arc<Database>,
        stores: VectorStoreService,
        logger: SharedLogger,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            stores,
            logger,
            clock,
            working_directory: std::env::current_dir().unwrap_or_default(),
            ann_threshold: DEFAULT_ANN_THRESHOLD,
        }
    }
}

#[derive(Clone)]
pub struct VectorSourceService {
    inner: Arc<VectorSourceServiceOptions>,
}

impl VectorSourceService {
    pub fn new(options: VectorSourceServiceOptions) -> Self {
        Self {
            inner: Arc::new(options),
        }
    }

    pub fn list_sources(&self, store_id: &str) -> Result<Vec<VectorSourceDto>, AppError> {
        self.inner.stores.require_store(store_id)?;
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_sources
                    .list_by_store(store_id)?)
            })?
            .iter()
            .map(source_dto)
            .collect()
    }

    pub fn add_source(&self, input: &AddVectorSourceInput) -> Result<VectorSourceDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let store_id = input.store_id.to_string();
        self.inner.stores.require_store(&store_id)?;
        let path = resolve_path(
            &from_extended_path(js_trim(&input.path)),
            &self.inner.working_directory,
        );
        let trimmed = |patterns: &[String]| -> Vec<String> {
            patterns
                .iter()
                .map(|pattern| js_trim(pattern).to_owned())
                .collect()
        };
        let draft = VectorSourceDraft {
            id: new_id().hyphenated().to_string(),
            store_id,
            kind: input.kind(),
            path: path.to_string_lossy().into_owned(),
            include: trimmed(input.include()),
            exclude: trimmed(input.exclude()),
            recursive: input.recursive(),
            created_at: self.inner.clock.now(),
        };
        let row = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).vector_sources.add(&draft)?)
        })?;
        source_dto(&row)
    }

    pub fn remove_source(&self, id: &str) -> Result<(), AppError> {
        self.inner.database.with_connection(|connection| {
            let sources = Repositories::new(connection).vector_sources;
            if sources.find_by_id(id)?.is_none() {
                return Err(AppError::new(AppErrorCode::NotFound, SOURCE_NOT_FOUND).into());
            }
            Ok(sources.remove(id)?)
        })
    }

    pub fn list_documents(&self, store_id: &str) -> Result<Vec<VectorDocumentDto>, AppError> {
        self.inner.stores.require_store(store_id)?;
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .list_by_store(store_id)?)
            })?
            .iter()
            .map(document_dto)
            .collect()
    }

    pub async fn remove_document(&self, store_id: &str, document_id: &str) -> Result<(), AppError> {
        let row = self
            .inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .find_by_id(document_id)?)
            })?
            .filter(|row| row.store_id == store_id)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, DOCUMENT_NOT_FOUND))?;
        {
            let (store, _lease) = self.inner.stores.lease(store_id)?;
            let path = self.inner.stores.store_path(&store.id)?;
            self.inner
                .stores
                .core()
                .delete_by_source(&path, &row.id)
                .await?;
            self.inner.database.with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .remove(&row.id)?)
            })?;
        }
        self.maintain(store_id).await;
        self.inner.stores.reconcile(store_id).await?;
        Ok(())
    }

    async fn maintain(&self, store_id: &str) {
        let outcome = async {
            let (store, _lease) = self.inner.stores.lease(store_id)?;
            let path = self.inner.stores.store_path(&store.id)?;
            self.inner
                .stores
                .core()
                .maintain(&path, self.inner.ann_threshold)
                .await
        }
        .await;
        if let Err(error) = outcome {
            self.inner.logger.log(
                LogLevel::Warn,
                "indexing",
                "Could not refresh the search indexes",
                &[
                    ("storeId", json!(store_id)),
                    ("error", json!(describe(&error))),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extended_prefix_is_stripped_for_storage() {
        assert_eq!(from_extended_path(r"\\?\C:\docs"), r"C:\docs");
        assert_eq!(
            from_extended_path(r"\\?\UNC\server\share"),
            r"\\server\share"
        );
        assert_eq!(from_extended_path("C:/docs"), "C:/docs");
    }

    #[test]
    fn a_relative_path_resolves_against_the_working_directory_and_dots_collapse() {
        let base = std::env::temp_dir();
        assert_eq!(
            resolve_path("docs/./a/../b", &base),
            base.join("docs").join("b")
        );
        let absolute = base.join("x");
        assert_eq!(
            resolve_path(&absolute.to_string_lossy(), Path::new("ignored")),
            absolute
        );
    }

    #[test]
    fn a_failure_is_described_with_its_detail() {
        let error =
            AppError::new(AppErrorCode::NativeError, "failed").with_detail("detail", "disk");
        assert_eq!(describe(&error), "failed: disk");
        assert_eq!(
            describe(&AppError::new(AppErrorCode::NativeError, "failed")),
            "failed"
        );
    }
}
