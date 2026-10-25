use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AppError, AppErrorCode, CreateSkillInput, ProviderCapability, SKILL_TRIGGER_KEY,
    SkillDetailDto, SkillDto, SkillExportInput, SkillExportResultDto, SkillFileContentDto, SkillId,
    SkillImportInput, SkillImportResultDto, SkillModelDto, SkillTriggerSettingsDto,
    UpdateSkillInput, Validate, VectorSourceKind,
};

use super::library::{LoadedSkill, SkillLibrary};
use super::matching::{cosine, lexical_score};
use super::nodes::SKILL_LOAD;
use super::script::ScriptRuntime;
use crate::commands::SHELL_RUN;
use crate::data::repositories::ProviderFilter;
use crate::data::schema::ProviderRow;
use crate::data::{Database, Repositories};
use crate::files::{FS_APPLY, FS_READ};
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::platform::picker::SharedPicker;
use crate::services::SettingService;
use crate::services::chat_tools::OfferedTool;
use crate::services::providers::DriverSource;

pub const NO_PICKER: &str = "Выбор папки недоступен";
pub const EMBEDDING_FAILED: &str = "Skill trigger embeddings failed; lexical matching used";

pub const CATALOGUE_SCRIPTS: &str = "Run a file from a skill's scripts/ folder with {script}, passing skill:<key>/scripts/<file> as argv[0], only when that skill's instructions say to run it.";
pub const CATALOGUE_FILES: &str = "Skill scripts run in this conversation's file folder chat:, which keeps its files between turns. Write working files there with {file} at chat:<name>, give them names that start with _ (for example chat:_spec.json), and pass them to scripts by relative path (_spec.json); never put working files next to the user's data.";
pub const CATALOGUE_INSTRUCTION: &str = "Skills are packaged instructions for specific jobs. When the user's request matches one of the skills below, call {load} with its key before answering and follow the instructions it returns. Read the files it lists with {read} at skill:<key>/<path> only when the instructions need them. Do not load a skill that does not fit the request.";

const EMBEDDING_TIMEOUT: Duration = Duration::from_secs(20);
const EMBEDDING_CACHE_MAX: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCatalogueEntry {
    pub key: String,
    pub description: String,
}

#[async_trait]
pub trait SkillCatalogue: Send + Sync {
    async fn catalogue(&self, query: &str, cancel: &CancellationToken) -> Vec<SkillCatalogueEntry>;
}

pub fn catalogue_text(entries: &[SkillCatalogueEntry], offered: &[OfferedTool]) -> String {
    let name = |key: &str| {
        offered
            .iter()
            .find(|tool| tool.key == key)
            .map_or_else(|| key.to_owned(), |tool| tool.name.clone())
    };
    let lines: Vec<String> = entries
        .iter()
        .map(|entry| format!("- {}: {}", entry.key, entry.description.replace('\n', " ")))
        .collect();
    let mut instruction = CATALOGUE_INSTRUCTION
        .replace("{load}", &name(SKILL_LOAD))
        .replace("{read}", &name(FS_READ));
    if offered.iter().any(|tool| tool.key == SHELL_RUN) {
        instruction.push(' ');
        instruction.push_str(&CATALOGUE_SCRIPTS.replace("{script}", &name(SHELL_RUN)));
        if offered.iter().any(|tool| tool.key == FS_APPLY) {
            instruction.push(' ');
            instruction.push_str(&CATALOGUE_FILES.replace("{file}", &name(FS_APPLY)));
        }
    }
    format!("{instruction}\n<skills>\n{}\n</skills>", lines.join("\n"))
}

pub fn has_scripts(loaded: &LoadedSkill) -> bool {
    loaded
        .files
        .iter()
        .any(|file| file.path.starts_with("scripts/") && ScriptRuntime::of(&file.path).is_some())
}

pub struct SkillServiceOptions {
    pub library: SkillLibrary,
    pub database: Arc<Database>,
    pub drivers: Option<Arc<dyn DriverSource>>,
    pub settings: Option<SettingService>,
    pub picker: Option<SharedPicker>,
    pub logger: SharedLogger,
}

impl SkillServiceOptions {
    pub fn new(library: SkillLibrary, database: Arc<Database>) -> Self {
        Self {
            library,
            database,
            drivers: None,
            settings: None,
            picker: None,
            logger: silent_logger(),
        }
    }
}

type EmbeddingKey = (String, String, String);

struct Inner {
    library: SkillLibrary,
    database: Arc<Database>,
    drivers: Option<Arc<dyn DriverSource>>,
    settings: Option<SettingService>,
    picker: Option<SharedPicker>,
    logger: SharedLogger,
    embeddings: Mutex<HashMap<EmbeddingKey, Vec<f32>>>,
}

#[derive(Clone)]
pub struct SkillService {
    inner: Arc<Inner>,
}

struct Candidate {
    key: String,
    name: String,
    description: String,
}

impl Candidate {
    fn of(skill: &SkillDto) -> Self {
        Self {
            key: skill.key.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
        }
    }

    fn text(&self) -> String {
        format!(
            "{} {} {}",
            self.key.replace('-', " "),
            self.name,
            self.description
        )
    }
}

impl SkillService {
    pub fn new(options: SkillServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                library: options.library,
                database: options.database,
                drivers: options.drivers,
                settings: options.settings,
                picker: options.picker,
                logger: options.logger,
                embeddings: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn library(&self) -> &SkillLibrary {
        &self.inner.library
    }

    pub fn list(&self) -> Result<Vec<SkillDto>, AppError> {
        self.inner.library.reconcile()
    }

    pub fn get(&self, id: &SkillId) -> Result<SkillDetailDto, AppError> {
        self.inner.library.get(id)
    }

    pub fn create(&self, input: &CreateSkillInput) -> Result<SkillDetailDto, AppError> {
        self.inner.library.create(input)
    }

    pub fn update(&self, input: &UpdateSkillInput) -> Result<SkillDetailDto, AppError> {
        self.inner.library.update(input)
    }

    pub fn remove(&self, id: &SkillId) -> Result<(), AppError> {
        self.inner.library.remove(id)
    }

    pub fn set_enabled(&self, id: &SkillId, enabled: bool) -> Result<SkillDto, AppError> {
        self.inner.library.set_enabled(id, enabled)
    }

    pub fn duplicate(&self, id: &SkillId) -> Result<SkillDetailDto, AppError> {
        self.inner.library.duplicate(id)
    }

    pub fn update_bundled(&self, id: &SkillId) -> Result<SkillDetailDto, AppError> {
        self.inner.library.update_bundled(id)
    }

    pub fn update_all_bundled(&self) -> Result<Vec<SkillDto>, AppError> {
        self.inner.library.update_all_bundled()
    }

    pub fn read_file(&self, id: &SkillId, path: &str) -> Result<SkillFileContentDto, AppError> {
        let content = self.inner.library.read_file(id, path)?;
        Ok(SkillFileContentDto {
            path: content.path,
            size: content.size,
            text: content.text,
        })
    }

    async fn pick_folder(&self) -> Result<Option<PathBuf>, AppError> {
        let picker = self
            .inner
            .picker
            .as_ref()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_PICKER))?;
        Ok(picker
            .pick(VectorSourceKind::Folder)
            .await?
            .into_iter()
            .next()
            .map(PathBuf::from))
    }

    pub async fn import(&self, input: &SkillImportInput) -> Result<SkillImportResultDto, AppError> {
        let folder = match &input.path {
            Some(path) => Some(PathBuf::from(js_trim(path))),
            None => self.pick_folder().await?,
        };
        let Some(folder) = folder else {
            return Ok(SkillImportResultDto { skill: None });
        };
        Ok(SkillImportResultDto {
            skill: Some(self.inner.library.import(&folder)?),
        })
    }

    pub async fn export(&self, input: &SkillExportInput) -> Result<SkillExportResultDto, AppError> {
        let folder = match &input.path {
            Some(path) => Some(PathBuf::from(js_trim(path))),
            None => self.pick_folder().await?,
        };
        let Some(folder) = folder else {
            return Ok(SkillExportResultDto { path: None });
        };
        let written = self.inner.library.export(&input.id, &folder)?;
        Ok(SkillExportResultDto {
            path: Some(written.to_string_lossy().into_owned()),
        })
    }

    pub fn trigger_settings(&self) -> SkillTriggerSettingsDto {
        self.inner
            .settings
            .as_ref()
            .and_then(|settings| settings.get(SKILL_TRIGGER_KEY).ok().flatten())
            .and_then(|setting| {
                serde_json::from_value::<SkillTriggerSettingsDto>(setting.value).ok()
            })
            .filter(|settings| settings.validate().is_ok())
            .unwrap_or_default()
    }

    fn read<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, crate::data::DataError>,
    ) -> Result<T, AppError> {
        self.inner
            .database
            .with_connection(|connection| work(&Repositories::new(connection)))
    }

    fn embedding_target(
        &self,
        settings: &SkillTriggerSettingsDto,
    ) -> Result<Option<(ProviderRow, String)>, AppError> {
        self.read(|repositories| {
            let candidates: Vec<(ProviderRow, Option<String>)> = match &settings.embedding {
                Some(SkillModelDto {
                    provider_id,
                    model_id,
                }) => repositories
                    .providers
                    .find_by_id(&provider_id.to_string())?
                    .map(|row| (row, Some(model_id.clone())))
                    .into_iter()
                    .collect(),
                None => repositories
                    .providers
                    .list(&ProviderFilter {
                        capability: Some(ProviderCapability::Embedding),
                        enabled: Some(true),
                    })?
                    .into_iter()
                    .map(|row| {
                        let model = row.default_model_id.clone();
                        (row, model)
                    })
                    .collect(),
            };
            for (provider, model) in candidates {
                let Some(model) = model else {
                    continue;
                };
                if !provider.enabled || !provider.cap_embedding {
                    continue;
                }
                let external = repositories
                    .models
                    .list_by_provider(&provider.id, false)?
                    .into_iter()
                    .find(|row| row.id == model || row.external_id == model)
                    .map_or(model, |row| row.external_id);
                return Ok(Some((provider, external)));
            }
            Ok(None)
        })
    }

    async fn embedding_scores(
        &self,
        message: &str,
        candidates: &[Candidate],
        settings: &SkillTriggerSettingsDto,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<f64>>, AppError> {
        let Some(drivers) = &self.inner.drivers else {
            return Ok(None);
        };
        let Some((provider, model)) = self.embedding_target(settings)? else {
            return Ok(None);
        };
        let driver = drivers
            .ephemeral_driver(&provider)
            .await?
            .embedding
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, EMBEDDING_FAILED))?;
        let key = |text: &str| (provider.id.clone(), model.clone(), text.to_owned());
        let texts: Vec<String> = std::iter::once(message.to_owned())
            .chain(candidates.iter().map(Candidate::text))
            .collect();
        let missing: Vec<String> = {
            let cache = self
                .inner
                .embeddings
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let mut seen = std::collections::HashSet::new();
            texts
                .iter()
                .filter(|text| !cache.contains_key(&key(text)) && seen.insert(text.as_str()))
                .cloned()
                .collect()
        };
        if !missing.is_empty() {
            let scope = cancel.child_token();
            let vectors = tokio::select! {
                result = driver.embed(&missing, &model, &scope) => result?,
                () = tokio::time::sleep(EMBEDDING_TIMEOUT) => {
                    scope.cancel();
                    return Err(AppError::new(AppErrorCode::ProviderUnreachable, EMBEDDING_FAILED));
                }
            };
            if vectors.len() != missing.len() {
                return Err(AppError::new(AppErrorCode::Unknown, EMBEDDING_FAILED));
            }
            let mut cache = self
                .inner
                .embeddings
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if cache.len() + missing.len() > EMBEDDING_CACHE_MAX {
                cache.clear();
            }
            for (text, vector) in missing.iter().zip(vectors) {
                cache.insert(key(text), vector);
            }
        }
        let cache = self
            .inner
            .embeddings
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(query) = cache.get(&key(message)) else {
            return Ok(None);
        };
        Ok(Some(
            candidates
                .iter()
                .map(|candidate| {
                    cache
                        .get(&key(&candidate.text()))
                        .map_or(0.0, |vector| cosine(query, vector))
                })
                .collect(),
        ))
    }

    async fn score(
        &self,
        message: &str,
        candidates: &[Candidate],
        settings: &SkillTriggerSettingsDto,
        cancel: &CancellationToken,
    ) -> Vec<f64> {
        if !candidates.is_empty() {
            match self
                .embedding_scores(message, candidates, settings, cancel)
                .await
            {
                Ok(Some(scores)) => return scores,
                Ok(None) => {}
                Err(error) => self.inner.logger.log(
                    LogLevel::Warn,
                    "skills",
                    EMBEDDING_FAILED,
                    &[("error", json!(error.message))],
                ),
            }
        }
        candidates
            .iter()
            .map(|candidate| lexical_score(message, &candidate.text()))
            .collect()
    }

    pub async fn select(
        &self,
        query: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<SkillDto>, AppError> {
        let settings = self.trigger_settings();
        let usable = self.inner.library.usable()?;
        let limit = usize::try_from(settings.catalogue_limit).unwrap_or(usize::MAX);
        if usable.len() <= limit || js_trim(query).is_empty() {
            return Ok(usable.into_iter().take(limit).collect());
        }
        let candidates: Vec<Candidate> = usable.iter().map(Candidate::of).collect();
        let scores = self.score(query, &candidates, &settings, cancel).await;
        let mut ranked: Vec<(f64, SkillDto)> = scores.into_iter().zip(usable).collect();
        ranked.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.key.cmp(&right.1.key))
        });
        Ok(ranked
            .into_iter()
            .take(limit)
            .map(|(_, skill)| skill)
            .collect())
    }
}

#[async_trait]
impl SkillCatalogue for SkillService {
    async fn catalogue(&self, query: &str, cancel: &CancellationToken) -> Vec<SkillCatalogueEntry> {
        match self.select(query, cancel).await {
            Ok(skills) => skills
                .into_iter()
                .map(|skill| SkillCatalogueEntry {
                    key: skill.key,
                    description: skill.description,
                })
                .collect(),
            Err(error) => {
                self.inner.logger.log(
                    LogLevel::Warn,
                    "skills",
                    "Could not list skills for the chat",
                    &[("error", json!(error.message))],
                );
                Vec::new()
            }
        }
    }
}
