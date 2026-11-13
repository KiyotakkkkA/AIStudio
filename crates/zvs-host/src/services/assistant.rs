use std::sync::Arc;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, AssistantPreferencesDto, ProviderCapability, SummarizerModelDto,
    Validate,
};

use super::settings::SettingService;
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub const ASSISTANT_KEY: &str = "assistant";
pub const SUMMARIZER_PROVIDER_UNAVAILABLE: &str =
    "Провайдер модели для сводок недоступен или не поддерживает генерацию текста";
pub const SUMMARIZER_MODEL_UNAVAILABLE: &str = "Модель для сводок недоступна";
pub const SUMMARIZER_DEFAULT_WINDOW: i64 = 8192;
pub const SUMMARIZER_DEFAULT_OUTPUT: i64 = 1024;
const LOG_SOURCE: &str = "assistant";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarizerModel {
    pub provider_id: String,
    pub model_id: String,
    pub external_id: String,
    pub context_window: i64,
    pub max_output_tokens: i64,
}

pub fn summarizer_model(
    database: &Database,
    provider_id: &str,
    model_id: &str,
) -> Result<Result<SummarizerModel, &'static str>, AppError> {
    let (provider, models) = database.with_connection(|connection| {
        let repositories = Repositories::new(connection);
        let provider = repositories.providers.find_by_id(provider_id)?;
        let models = match &provider {
            Some(provider) => repositories.models.list_by_provider(&provider.id, false)?,
            None => Vec::new(),
        };
        Ok((provider, models))
    })?;
    let Some(provider) = provider.filter(|provider| {
        provider.enabled && provider.capabilities.contains(&ProviderCapability::Text)
    }) else {
        return Ok(Err(SUMMARIZER_PROVIDER_UNAVAILABLE));
    };
    let Some(row) = models
        .iter()
        .find(|row| row.id == model_id || row.external_id == model_id)
        .filter(|row| row.available)
    else {
        return Ok(Err(SUMMARIZER_MODEL_UNAVAILABLE));
    };
    let context_window = row
        .context_window
        .unwrap_or(SUMMARIZER_DEFAULT_WINDOW)
        .max(1);
    let max_output_tokens = row
        .max_output
        .unwrap_or(SUMMARIZER_DEFAULT_OUTPUT)
        .min((context_window / 4).max(1))
        .max(1);
    Ok(Ok(SummarizerModel {
        provider_id: provider.id,
        model_id: model_id.to_owned(),
        external_id: row.external_id.clone(),
        context_window,
        max_output_tokens,
    }))
}

#[derive(Clone)]
pub struct AssistantService {
    database: Arc<Database>,
    settings: SettingService,
    logger: SharedLogger,
}

impl AssistantService {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self {
            settings: SettingService::new(Arc::clone(&database), clock),
            database,
            logger: silent_logger(),
        }
    }

    #[must_use]
    pub fn with_logger(self, logger: SharedLogger) -> Self {
        Self { logger, ..self }
    }

    fn stored(&self) -> Result<AssistantPreferencesDto, AppError> {
        let Some(setting) = self.settings.get(ASSISTANT_KEY)? else {
            return Ok(AssistantPreferencesDto::default());
        };
        Ok(
            serde_json::from_value::<AssistantPreferencesDto>(setting.value)
                .ok()
                .filter(|stored| stored.validate().is_ok())
                .unwrap_or_else(|| {
                    self.logger.log(
                        LogLevel::Warn,
                        LOG_SOURCE,
                        "Stored assistant preferences are malformed; using the defaults",
                        &[],
                    );
                    AssistantPreferencesDto::default()
                }),
        )
    }

    fn usable(
        &self,
        model: &SummarizerModelDto,
    ) -> Result<Result<SummarizerModel, &'static str>, AppError> {
        summarizer_model(&self.database, &model.provider_id, &model.model_id)
    }

    pub fn get(&self) -> Result<AssistantPreferencesDto, AppError> {
        let stored = self.stored()?;
        let mut models = Vec::with_capacity(stored.summarizer_models.len());
        for model in &stored.summarizer_models {
            if self.usable(model)?.is_ok() {
                models.push(model.clone());
            }
        }
        let sanitized = AssistantPreferencesDto {
            summarizer_models: models,
            ..stored.clone()
        };
        if sanitized != stored {
            self.logger.log(
                LogLevel::Info,
                LOG_SOURCE,
                "Summarizer models that are gone were dropped",
                &[(
                    "dropped",
                    json!(stored.summarizer_models.len() - sanitized.summarizer_models.len()),
                )],
            );
            self.settings.set(ASSISTANT_KEY, &json!(sanitized))?;
        }
        Ok(sanitized)
    }

    pub fn set(
        &self,
        input: &AssistantPreferencesDto,
    ) -> Result<AssistantPreferencesDto, AppError> {
        input.validate()?;
        for model in &input.summarizer_models {
            if let Err(problem) = self.usable(model)? {
                return Err(AppError::new(AppErrorCode::ValidationFailed, problem)
                    .with_detail("modelId", model.model_id.clone()));
            }
        }
        self.settings.set(ASSISTANT_KEY, &json!(input))?;
        Ok(input.clone())
    }

    pub fn preferences(&self) -> AssistantPreferencesDto {
        self.stored().unwrap_or_default()
    }

    pub fn summarizers(&self) -> Vec<SummarizerModel> {
        self.preferences()
            .summarizer_models
            .iter()
            .filter_map(|model| self.usable(model).ok().and_then(Result::ok))
            .collect()
    }

    pub fn database(&self) -> &Database {
        &self.database
    }
}

#[cfg(test)]
mod tests;
