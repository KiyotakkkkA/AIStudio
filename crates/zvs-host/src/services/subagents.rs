use std::sync::Arc;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, AuthMode, ProviderCapability, SubagentModelDto, SubagentPreferencesDto,
    ToolMode, Validate, model_tool_support, resolve_tool_mode,
};

use super::settings::SettingService;
use crate::data::{Database, Repositories};
use crate::kernel::TextDrivers;
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub const SUBAGENTS_KEY: &str = "subagents";
pub const BUDGET_CALLS_KEY: &str = "subagents.budget.toolCalls";
pub const BUDGET_TOKENS_KEY: &str = "subagents.budget.tokens";
pub const BUDGET_MINUTES_KEY: &str = "subagents.budget.minutes";
pub const DEFAULT_BUDGET_CALLS: u64 = 20;
pub const DEFAULT_BUDGET_TOKENS: u64 = 200_000;
pub const DEFAULT_BUDGET_MINUTES: u64 = 10;
pub const SUBAGENT_PROVIDER_UNAVAILABLE: &str =
    "Провайдер модели субагента выключен или не поддерживает текст";
pub const SUBAGENT_MODEL_UNAVAILABLE: &str = "Модель субагента недоступна";
pub const SUBAGENT_MODEL_NO_TOOLS: &str = "Модель субагента не поддерживает вызов инструментов";
const LOG_SOURCE: &str = "subagents";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubagentBudget {
    pub tool_calls: u64,
    pub tokens: u64,
    pub minutes: u64,
}

impl Default for SubagentBudget {
    fn default() -> Self {
        Self {
            tool_calls: DEFAULT_BUDGET_CALLS,
            tokens: DEFAULT_BUDGET_TOKENS,
            minutes: DEFAULT_BUDGET_MINUTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsableModel {
    pub provider_id: String,
    pub model_id: String,
    pub external_id: String,
    pub account: bool,
}

#[derive(Clone)]
pub struct SubagentService {
    database: Arc<Database>,
    settings: SettingService,
    drivers: Option<Arc<dyn TextDrivers>>,
    logger: SharedLogger,
}

pub fn usable_model(
    database: &Database,
    drivers: Option<&dyn TextDrivers>,
    model: &SubagentModelDto,
) -> Result<Result<UsableModel, &'static str>, AppError> {
    let (provider, models) = database.with_connection(|connection| {
        let repositories = Repositories::new(connection);
        let provider = repositories.providers.find_by_id(&model.provider_id)?;
        let models = match &provider {
            Some(provider) => repositories.models.list_by_provider(&provider.id, false)?,
            None => Vec::new(),
        };
        Ok((provider, models))
    })?;
    let Some(provider) = provider.filter(|provider| {
        provider.enabled && provider.capabilities.contains(&ProviderCapability::Text)
    }) else {
        return Ok(Err(SUBAGENT_PROVIDER_UNAVAILABLE));
    };
    let Some(row) = models
        .iter()
        .find(|row| row.id == model.model_id || row.external_id == model.model_id)
        .filter(|row| row.available)
    else {
        return Ok(Err(SUBAGENT_MODEL_UNAVAILABLE));
    };
    let capabilities = |row: &crate::data::schema::ModelRow| -> Vec<&'static str> {
        row.capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect()
    };
    let siblings: Vec<Vec<&str>> = models.iter().map(capabilities).collect();
    let sibling_slices: Vec<&[&str]> = siblings.iter().map(Vec::as_slice).collect();
    let supports = model_tool_support(&capabilities(row), &sibling_slices);
    let adapter = drivers
        .and_then(|drivers| drivers.text(&provider.id).ok())
        .map(|driver| driver.capabilities().tools);
    let callable = match adapter {
        Some(adapter) => resolve_tool_mode(adapter, supports) != ToolMode::None,
        None => supports != Some(false),
    };
    if !callable {
        return Ok(Err(SUBAGENT_MODEL_NO_TOOLS));
    }
    Ok(Ok(UsableModel {
        provider_id: provider.id.clone(),
        model_id: model.model_id.clone(),
        external_id: row.external_id.clone(),
        account: provider.auth() == Some(AuthMode::Account),
    }))
}

pub fn account_provider(database: &Database, provider_id: &str) -> Result<bool, AppError> {
    database.with_connection(|connection| {
        Ok(Repositories::new(connection)
            .providers
            .find_by_id(provider_id)?
            .is_some_and(|provider| provider.auth() == Some(AuthMode::Account)))
    })
}

impl SubagentService {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self {
            settings: SettingService::new(Arc::clone(&database), clock),
            database,
            drivers: None,
            logger: silent_logger(),
        }
    }

    #[must_use]
    pub fn with_drivers(self, drivers: Arc<dyn TextDrivers>) -> Self {
        Self {
            drivers: Some(drivers),
            ..self
        }
    }

    #[must_use]
    pub fn with_logger(self, logger: SharedLogger) -> Self {
        Self { logger, ..self }
    }

    fn stored(&self) -> Result<SubagentPreferencesDto, AppError> {
        let Some(setting) = self.settings.get(SUBAGENTS_KEY)? else {
            return Ok(SubagentPreferencesDto::default());
        };
        Ok(
            serde_json::from_value::<SubagentPreferencesDto>(setting.value)
                .ok()
                .filter(|stored| stored.validate().is_ok())
                .unwrap_or_else(|| {
                    self.logger.log(
                        LogLevel::Warn,
                        LOG_SOURCE,
                        "Stored subagent preferences are malformed; using the defaults",
                        &[],
                    );
                    SubagentPreferencesDto::default()
                }),
        )
    }

    fn usable(
        &self,
        model: &SubagentModelDto,
    ) -> Result<Result<UsableModel, &'static str>, AppError> {
        usable_model(&self.database, self.drivers.as_deref(), model)
    }

    pub fn get(&self) -> Result<SubagentPreferencesDto, AppError> {
        let stored = self.stored()?;
        let mut models = Vec::with_capacity(stored.models.len());
        for model in &stored.models {
            if self.usable(model)?.is_ok() {
                models.push(model.clone());
            }
        }
        let sanitized = SubagentPreferencesDto {
            models,
            ..stored.clone()
        };
        if sanitized != stored {
            self.logger.log(
                LogLevel::Info,
                LOG_SOURCE,
                "Subagent models that are gone were dropped",
                &[(
                    "dropped",
                    json!(stored.models.len() - sanitized.models.len()),
                )],
            );
            self.settings.set(SUBAGENTS_KEY, &json!(sanitized))?;
        }
        Ok(sanitized)
    }

    pub fn set(&self, input: &SubagentPreferencesDto) -> Result<SubagentPreferencesDto, AppError> {
        input.validate()?;
        for model in &input.models {
            if let Err(problem) = self.usable(model)? {
                return Err(AppError::new(AppErrorCode::ValidationFailed, problem)
                    .with_detail("modelId", model.model_id.clone()));
            }
        }
        self.settings.set(SUBAGENTS_KEY, &json!(input))?;
        Ok(input.clone())
    }

    pub fn preferences(&self) -> SubagentPreferencesDto {
        self.stored().unwrap_or_default()
    }

    pub fn budget(&self) -> SubagentBudget {
        let fallback = SubagentBudget::default();
        let read = |key: &str, fallback: u64| {
            self.settings
                .get(key)
                .ok()
                .flatten()
                .and_then(|setting| setting.value.as_u64())
                .filter(|value| *value > 0)
                .unwrap_or(fallback)
        };
        SubagentBudget {
            tool_calls: read(BUDGET_CALLS_KEY, fallback.tool_calls),
            tokens: read(BUDGET_TOKENS_KEY, fallback.tokens),
            minutes: read(BUDGET_MINUTES_KEY, fallback.minutes),
        }
    }

    pub fn selected(
        &self,
        drivers: Option<&dyn TextDrivers>,
    ) -> Result<Option<UsableModel>, AppError> {
        let drivers = drivers.or(self.drivers.as_deref());
        for model in &self.preferences().models {
            if let Ok(usable) = usable_model(&self.database, drivers, model)? {
                return Ok(Some(usable));
            }
        }
        Ok(None)
    }

    pub fn database(&self) -> &Database {
        &self.database
    }
}

#[cfg(test)]
mod tests;
