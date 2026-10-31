use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::{AppError, AppErrorCode, ModelCapability, Timestamp};

use super::provider::ProviderRepository;
use crate::data::DataError;
use crate::data::schema::ModelRow;
use crate::data::schema::columns::json_text;
use crate::drivers::ai::ports::DiscoveredModel;
use crate::platform::ids::new_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDraft {
    pub external_id: String,
    pub is_free: Option<bool>,
    pub no_training: Option<bool>,
    pub display_name: String,
    pub family: Option<String>,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub size_bytes: Option<i64>,
    pub capabilities: Vec<ModelCapability>,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub discovered_at: Timestamp,
}

impl ModelDraft {
    pub fn new(
        external_id: impl Into<String>,
        display_name: impl Into<String>,
        discovered_at: Timestamp,
    ) -> Self {
        Self {
            external_id: external_id.into(),
            is_free: None,
            no_training: None,
            display_name: display_name.into(),
            family: None,
            context_window: None,
            max_output: None,
            size_bytes: None,
            capabilities: Vec::new(),
            available: true,
            unavailable_reason: None,
            discovered_at,
        }
    }

    pub fn discovered(model: &DiscoveredModel, discovered_at: Timestamp) -> Self {
        Self {
            is_free: model.is_free,
            no_training: model.no_training,
            family: model.family.clone(),
            context_window: model.context_window,
            max_output: model.max_output,
            size_bytes: model.size_bytes,
            capabilities: model.capabilities.clone(),
            ..Self::new(
                model.external_id.clone(),
                model.display_name.clone(),
                discovered_at,
            )
        }
    }
}

pub fn provider_not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, "Провайдер не найден")
}

pub struct ModelRepository<'c> {
    connection: &'c Connection,
}

impl<'c> ModelRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_provider(
        &self,
        provider_id: &str,
        selected_only: bool,
    ) -> rusqlite::Result<Vec<ModelRow>> {
        let sql = if selected_only {
            format!(
                "SELECT {} FROM model INNER JOIN provider ON model.provider_id = provider.id \
                 WHERE provider.id = ?1 AND EXISTS (SELECT 1 FROM json_each(provider.settings, '$.selectedModelIds') WHERE value = model.external_id) \
                 ORDER BY model.external_id ASC",
                ModelRow::qualified_columns()
            )
        } else {
            format!(
                "SELECT {} FROM model WHERE provider_id = ?1 ORDER BY external_id ASC",
                ModelRow::COLUMNS
            )
        };
        self.connection
            .prepare_cached(&sql)?
            .query_map([provider_id], ModelRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<ModelRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM model WHERE id = ?1",
                ModelRow::COLUMNS
            ))?
            .query_row([id], ModelRow::from_row)
            .optional()
    }

    /// Records the context window a vendor reported for a turn, for models whose catalog
    /// entry has none (Copilot auto selection resolves per turn).
    pub fn set_context_window(
        &self,
        provider_id: &str,
        external_id: &str,
        window: i64,
    ) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE model SET context_window = ?3 WHERE provider_id = ?1 AND external_id = ?2",
            )?
            .execute(params![provider_id, external_id, window])
    }

    pub fn replace_for_provider(
        &self,
        provider_id: &str,
        models: &[ModelDraft],
        now: Timestamp,
    ) -> Result<Vec<ModelRow>, DataError> {
        let owner = ProviderRepository::new(self.connection)
            .find_by_id(provider_id)?
            .ok_or_else(provider_not_found)?;
        let previous = self
            .list_by_provider(provider_id, false)?
            .into_iter()
            .map(|row| (row.external_id, (row.id, row.context_window)))
            .collect::<HashMap<_, _>>();
        self.connection
            .prepare_cached("DELETE FROM model WHERE provider_id = ?1")?
            .execute([provider_id])?;
        for draft in models {
            let (id, measured) = previous.get(&draft.external_id).cloned().map_or_else(
                || (new_id().hyphenated().to_string(), None),
                |(id, window)| (id, window),
            );
            self.connection
                .prepare_cached(
                    "INSERT INTO model (id, provider_id, external_id, is_free, no_training, display_name, family, context_window, max_output, size_bytes, capabilities, available, unavailable_reason, discovered_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                )?
                .execute(params![
                    id,
                    provider_id,
                    draft.external_id,
                    draft.is_free,
                    draft.no_training,
                    draft.display_name,
                    draft.family,
                    draft.context_window.or(measured),
                    draft.max_output,
                    draft.size_bytes,
                    json_text(&draft.capabilities)?,
                    draft.available,
                    draft.unavailable_reason,
                    draft.discovered_at,
                ])?;
        }
        let rows = self.list_by_provider(provider_id, false)?;
        if let Some(default) = &owner.default_model_id
            && !rows.iter().any(|row| &row.id == default)
        {
            self.connection
                .prepare_cached(
                    "UPDATE provider SET default_model_id = NULL, updated_at = ?1 WHERE id = ?2",
                )?
                .execute(params![now, provider_id])?;
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::{ProviderCapability, ProviderKind};

    use super::*;
    use crate::data::repositories::{ProviderDraft, ProviderPatch, Repositories};
    use crate::data::schema::ProviderSettings;
    use crate::testing::{CLOCK_ORIGIN, temporary_database};

    const NOW: Timestamp = CLOCK_ORIGIN;

    fn provider(name: &str) -> ProviderDraft {
        ProviderDraft::new(
            ProviderKind::Ollama,
            name,
            "http://localhost:11434",
            vec![ProviderCapability::Text],
            100,
        )
    }

    fn discovered() -> ModelDraft {
        ModelDraft::new("gpt-oss:120b", "GPT OSS", 200)
    }

    #[test]
    fn discovery_replaces_stale_models_and_retains_metadata_and_stable_ids() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let (providers, models) = (&repositories.providers, &repositories.models);
                let row = providers.create(&provider("Local"))?;
                let other = providers.create(&provider("Other"))?;
                let initial = models
                    .replace_for_provider(
                        &row.id,
                        &[
                            discovered(),
                            ModelDraft {
                                external_id: "stale".to_owned(),
                                ..discovered()
                            },
                        ],
                        NOW,
                    )?
                    .remove(0);
                models.replace_for_provider(&other.id, &[discovered()], NOW)?;
                providers.set_default_model(&row.id, Some(&initial.id), NOW)?;
                let refreshed = models
                    .replace_for_provider(
                        &row.id,
                        &[ModelDraft {
                            family: Some("gpt-oss".to_owned()),
                            context_window: Some(128_000),
                            max_output: Some(4096),
                            size_bytes: Some(65_000_000_000),
                            capabilities: vec![
                                ModelCapability::Tools,
                                ModelCapability::Vision,
                                ModelCapability::Streaming,
                                ModelCapability::Reasoning,
                                ModelCapability::Code,
                            ],
                            available: false,
                            unavailable_reason: Some("Not available on this plan".to_owned()),
                            discovered_at: 300,
                            ..discovered()
                        }],
                        NOW,
                    )?
                    .remove(0);
                assert_eq!(refreshed.id, initial.id);
                assert!(!refreshed.available);
                assert_eq!(
                    refreshed.unavailable_reason.as_deref(),
                    Some("Not available on this plan")
                );
                assert_eq!(refreshed.size_bytes, Some(65_000_000_000));
                assert_eq!(refreshed.capabilities.len(), 5);
                assert_eq!(models.find_by_id(&refreshed.id)?, Some(refreshed.clone()));
                assert_eq!(
                    providers
                        .find_by_id(&row.id)?
                        .expect("a row")
                        .default_model_id,
                    Some(initial.id.clone())
                );
                assert_eq!(models.list_by_provider(&other.id, false)?.len(), 1);
                models.replace_for_provider(&row.id, &[], NOW + 5)?;
                assert!(models.list_by_provider(&row.id, false)?.is_empty());
                let cleared = providers.find_by_id(&row.id)?.expect("a row");
                assert_eq!(cleared.default_model_id, None);
                assert_eq!(cleared.updated_at, NOW + 5);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn failed_discovery_rolls_back_deletions_inserts_and_default_selection() {
        let database = temporary_database();
        let (row, previous) = database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let row = repositories.providers.create(&provider("Local"))?;
                let previous =
                    repositories
                        .models
                        .replace_for_provider(&row.id, &[discovered()], NOW)?;
                repositories
                    .providers
                    .set_default_model(&row.id, Some(&previous[0].id), NOW)?;
                Ok((row, previous))
            })
            .expect("seeds");
        let duplicate = ModelDraft {
            external_id: "duplicate".to_owned(),
            ..discovered()
        };
        let failed = database.transaction(|transaction| {
            Repositories::new(transaction).models.replace_for_provider(
                &row.id,
                &[duplicate.clone(), duplicate.clone()],
                NOW,
            )
        });
        assert!(failed.is_err());
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                assert_eq!(repositories.models.list_by_provider(&row.id, false)?, previous);
                assert_eq!(
                    repositories
                        .providers
                        .find_by_id(&row.id)?
                        .expect("a row")
                        .default_model_id,
                    Some(previous[0].id.clone())
                );
                let missing = repositories
                    .models
                    .replace_for_provider("missing", &[], NOW)
                    .expect_err("refuses");
                assert!(
                    matches!(missing, DataError::App(ref error) if error.code == AppErrorCode::NotFound)
                );
                Ok(())
            })
            .expect("reads");
    }

    #[test]
    fn a_unique_violation_names_the_constraint() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let row = repositories.providers.create(&provider("Local"))?;
                let error = repositories
                    .models
                    .replace_for_provider(&row.id, &[discovered(), discovered()], NOW)
                    .expect_err("refuses");
                assert!(error.to_string().contains("UNIQUE"), "{error}");
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn selected_models_are_the_ones_named_in_the_provider_settings() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let row = repositories.providers.create(&provider("Local"))?;
                repositories.models.replace_for_provider(
                    &row.id,
                    &[
                        ModelDraft::new("b", "B", 1),
                        ModelDraft::new("a", "A", 1),
                        ModelDraft::new("c", "C", 1),
                    ],
                    NOW,
                )?;
                let ids = |selected| {
                    repositories
                        .models
                        .list_by_provider(&row.id, selected)
                        .map(|rows| {
                            rows.into_iter()
                                .map(|row| row.external_id)
                                .collect::<Vec<_>>()
                        })
                };
                assert_eq!(ids(false)?, ["a", "b", "c"]);
                assert!(ids(true)?.is_empty());
                repositories.providers.update(
                    &row.id,
                    &ProviderPatch {
                        settings: Some(ProviderSettings::default().with(
                            ProviderSettings::SELECTED_MODEL_IDS,
                            json!(["c", "a", "zzz"]),
                        )),
                        ..ProviderPatch::default()
                    },
                    NOW,
                )?;
                assert_eq!(ids(true)?, ["a", "c"]);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn a_discovered_model_becomes_a_draft_with_every_field() {
        let model = DiscoveredModel {
            is_free: Some(true),
            no_training: Some(false),
            family: Some("openai".to_owned()),
            context_window: Some(8),
            max_output: Some(4),
            size_bytes: Some(2),
            capabilities: vec![ModelCapability::Streaming],
            ..DiscoveredModel::named("openai/gpt")
        };
        let draft = ModelDraft::discovered(&model, 9);
        assert_eq!(draft.external_id, "openai/gpt");
        assert_eq!(draft.display_name, "openai/gpt");
        assert_eq!(draft.is_free, Some(true));
        assert_eq!(draft.no_training, Some(false));
        assert_eq!(draft.capabilities, [ModelCapability::Streaming]);
        assert!(draft.available);
        assert_eq!(draft.discovered_at, 9);
    }
}
