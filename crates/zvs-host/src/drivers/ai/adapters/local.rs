use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AppErrorCode, AuthMode, ToolMode};

use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::cancelled;
use crate::drivers::ai::ports::{DiscoveredModel, EmbeddingDriver};
use crate::platform::logger::{LogLevel, SharedLogger};

pub const LOCAL_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::Local,
    auth_modes: &[AuthMode::Api],
    streaming: false,
    live_model_list: true,
    embedding: true,
    image: false,
    honours: Honours::NONE,
    tools: ToolMode::Native,
};

pub const WEIGHT_EXTENSIONS: [&str; 4] = ["gguf", "safetensors", "bin", "onnx"];
pub const MODEL_DIRECTORIES: [&str; 2] = ["models", "embeddings"];
pub const NO_ENGINE_MESSAGE: &str =
    "Локальный движок эмбеддингов не подключён: файл модели виден, но посчитать вектор нечем";

#[async_trait]
pub trait LocalModelStore: Send + Sync {
    async fn list(&self, cancel: &CancellationToken) -> Result<Vec<DiscoveredModel>, AppError>;
}

#[async_trait]
pub trait LocalEmbeddingEngine: Send + Sync {
    async fn embed(
        &self,
        model_name: &str,
        texts: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError>;
}

pub type SharedModelStore = Arc<dyn LocalModelStore>;
pub type SharedEmbeddingEngine = Arc<dyn LocalEmbeddingEngine>;

pub struct FileSystemModelStore {
    root: PathBuf,
    directories: Vec<String>,
}

impl FileSystemModelStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            directories: MODEL_DIRECTORIES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }

    #[must_use]
    pub fn with_directories(mut self, directories: Vec<String>) -> Self {
        self.directories = directories;
        self
    }

    fn scan(
        directory: &Path,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return Ok(Vec::new());
        };
        let mut models = Vec::new();
        for entry in entries.flatten() {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = Path::new(&name);
            let Some(extension) = path
                .extension()
                .map(|extension| extension.to_string_lossy().to_lowercase())
                .filter(|extension| WEIGHT_EXTENSIONS.contains(&extension.as_str()))
            else {
                continue;
            };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let stem = path
                .file_stem()
                .map_or_else(|| name.clone(), |stem| stem.to_string_lossy().into_owned());
            models.push(DiscoveredModel {
                display_name: stem,
                family: Some(extension),
                size_bytes: i64::try_from(metadata.len()).ok(),
                ..DiscoveredModel::named(name)
            });
        }
        Ok(models)
    }
}

fn collate(left: &str, right: &str) -> Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

#[async_trait]
impl LocalModelStore for FileSystemModelStore {
    async fn list(&self, cancel: &CancellationToken) -> Result<Vec<DiscoveredModel>, AppError> {
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for folder in &self.directories {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            for model in Self::scan(&self.root.join(folder), cancel)? {
                if seen.insert(model.external_id.clone()) {
                    found.push(model);
                }
            }
        }
        found.sort_by(|left, right| collate(&left.display_name, &right.display_name));
        Ok(found)
    }
}

pub struct LocalEmbeddingAdapter {
    store: SharedModelStore,
    engine: Option<SharedEmbeddingEngine>,
    logger: SharedLogger,
}

impl LocalEmbeddingAdapter {
    pub fn new(
        store: SharedModelStore,
        engine: Option<SharedEmbeddingEngine>,
        logger: SharedLogger,
    ) -> Self {
        Self {
            store,
            engine,
            logger,
        }
    }
}

#[async_trait]
impl EmbeddingDriver for LocalEmbeddingAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        LOCAL_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let Some(engine) = &self.engine else {
            return Err(AppError::new(AppErrorCode::Conflict, NO_ENGINE_MESSAGE)
                .with_detail("family", "local")
                .with_detail("model", model)
                .with_detail("texts", texts.len()));
        };
        engine.embed(model, texts, cancel).await
    }

    fn dimensions(&self) -> Option<usize> {
        None
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let models = self.store.list(cancel).await?;
        self.logger.log(
            LogLevel::Debug,
            "ai",
            "Listed the local model files",
            &[("count", json!(models.len()))],
        );
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::platform::logger::silent_logger;
    use crate::testing::{TemporaryDirectory, temporary_directory};

    fn put(workspace: &TemporaryDirectory, folder: &str, name: &str, bytes: usize) {
        let directory = workspace.join(folder);
        std::fs::create_dir_all(&directory).expect("a folder");
        std::fs::write(directory.join(name), vec![0_u8; bytes]).expect("a file");
    }

    fn store(workspace: &TemporaryDirectory) -> SharedModelStore {
        Arc::new(FileSystemModelStore::new(workspace.path()))
    }

    #[derive(Default)]
    struct RecordingEngine(Mutex<Vec<(String, Vec<String>)>>);

    #[async_trait]
    impl LocalEmbeddingEngine for RecordingEngine {
        async fn embed(
            &self,
            model_name: &str,
            texts: &[String],
            _cancel: &CancellationToken,
        ) -> Result<Vec<Vec<f32>>, AppError> {
            self.0
                .lock()
                .expect("calls")
                .push((model_name.to_owned(), texts.to_vec()));
            Ok(vec![vec![1.0, 0.0]])
        }
    }

    #[tokio::test]
    async fn the_local_store_lists_the_weight_files_on_disk_and_ignores_everything_else() {
        let workspace = temporary_directory("local-models-");
        put(&workspace, "embeddings", "bge-m3-FP16.gguf", 64);
        put(&workspace, "models", "bge-reranker-v2-m3-FP16.gguf", 32);
        put(&workspace, "models", "notes.txt", 8);
        put(&workspace, "models", "qwen.gguf.part", 8);
        put(&workspace, "models", "Encoder.ONNX", 4);
        put(&workspace, "mcp", "server-filesystem.tgz", 8);
        std::fs::create_dir_all(workspace.join("models").join("folder.gguf")).expect("a folder");

        let models = store(&workspace)
            .list(&CancellationToken::new())
            .await
            .expect("lists");

        assert_eq!(
            models
                .iter()
                .map(|model| model.external_id.as_str())
                .collect::<Vec<_>>(),
            [
                "bge-m3-FP16.gguf",
                "bge-reranker-v2-m3-FP16.gguf",
                "Encoder.ONNX"
            ]
        );
        assert_eq!(models[0].display_name, "bge-m3-FP16");
        assert_eq!(models[0].size_bytes, Some(64));
        assert_eq!(models[0].family.as_deref(), Some("gguf"));
        assert_eq!(models[2].family.as_deref(), Some("onnx"));
        assert!(models[0].capabilities.is_empty());
    }

    #[tokio::test]
    async fn a_missing_downloads_folder_is_an_empty_list_not_a_failure() {
        let workspace = temporary_directory("local-models-");
        assert!(
            store(&workspace)
                .list(&CancellationToken::new())
                .await
                .expect("lists")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn the_same_file_under_two_folders_is_listed_once() {
        let workspace = temporary_directory("local-models-");
        put(&workspace, "models", "bge-m3-FP16.gguf", 16);
        put(&workspace, "embeddings", "bge-m3-FP16.gguf", 16);
        assert_eq!(
            store(&workspace)
                .list(&CancellationToken::new())
                .await
                .expect("lists")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_cancelled_listing_stops() {
        let workspace = temporary_directory("local-models-");
        put(&workspace, "models", "a.gguf", 1);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = store(&workspace)
            .list(&cancel)
            .await
            .expect_err("cancelled");
        assert_eq!(error.code, AppErrorCode::RunCancelled);
    }

    #[tokio::test]
    async fn the_local_adapter_discovers_models_and_refuses_to_embed_until_an_engine_exists() {
        let workspace = temporary_directory("local-models-");
        put(&workspace, "embeddings", "bge-m3-FP16.gguf", 16);
        let adapter = LocalEmbeddingAdapter::new(store(&workspace), None, silent_logger());
        assert_eq!(adapter.capabilities(), LOCAL_CAPABILITIES);
        assert_eq!(adapter.dimensions(), None);
        assert_eq!(
            adapter
                .list_models(&CancellationToken::new())
                .await
                .expect("lists")
                .len(),
            1
        );
        let error = adapter
            .embed(
                &["привет".to_owned()],
                "bge-m3-FP16.gguf",
                &CancellationToken::new(),
            )
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Conflict);
        assert_eq!(error.message, NO_ENGINE_MESSAGE);
        assert_eq!(
            error.details.map(serde_json::Value::Object),
            Some(json!({ "family": "local", "model": "bge-m3-FP16.gguf", "texts": 1 }))
        );
    }

    #[tokio::test]
    async fn with_an_engine_attached_the_adapter_hands_the_model_name_straight_to_it() {
        let workspace = temporary_directory("local-models-");
        put(&workspace, "embeddings", "bge-m3-FP16.gguf", 16);
        let engine = Arc::new(RecordingEngine::default());
        let adapter =
            LocalEmbeddingAdapter::new(store(&workspace), Some(engine.clone()), silent_logger());
        let vectors = adapter
            .embed(
                &["привет".to_owned()],
                "bge-m3-FP16.gguf",
                &CancellationToken::new(),
            )
            .await
            .expect("embeds");
        assert_eq!(vectors, [vec![1.0, 0.0]]);
        assert_eq!(
            *engine.0.lock().expect("calls"),
            [("bge-m3-FP16.gguf".to_owned(), vec!["привет".to_owned()])]
        );
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            adapter
                .embed(&["x".to_owned()], "m", &cancel)
                .await
                .expect_err("cancelled")
                .code,
            AppErrorCode::RunCancelled
        );
    }

    #[test]
    fn names_sort_case_insensitively() {
        let mut names = vec!["beta", "Alpha", "alpha", "Gamma"];
        names.sort_by(|left, right| collate(left, right));
        assert_eq!(names, ["Alpha", "alpha", "beta", "Gamma"]);
    }
}
