use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use zvs_contract::{
    AdapterFamily, AppError, AppErrorCode, AuthMode, ProviderCapability, ProviderKind,
    ProviderStatus, Timestamp,
};

use super::secret::{SecretRepository, SecretUsageDraft};
use crate::data::DataError;
use crate::data::schema::columns::{EnumColumn, json_text};
use crate::data::schema::{ProviderRow, ProviderSettings, SecretConsumerKind};
use crate::platform::ids::new_id;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProviderFilter {
    pub capability: Option<ProviderCapability>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderDraft {
    pub id: Option<String>,
    pub kind: ProviderKind,
    pub adapter: AdapterFamily,
    pub auth_mode: AuthMode,
    pub name: String,
    pub base_url: String,
    pub secret_id: Option<String>,
    pub account_id: Option<String>,
    pub capabilities: Vec<ProviderCapability>,
    pub settings: ProviderSettings,
    pub enabled: bool,
    pub status: ProviderStatus,
    pub status_detail: Option<String>,
    pub last_probe_at: Option<Timestamp>,
    pub last_latency_ms: Option<i64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ProviderDraft {
    pub fn new(
        kind: ProviderKind,
        name: impl Into<String>,
        base_url: impl Into<String>,
        capabilities: Vec<ProviderCapability>,
        now: Timestamp,
    ) -> Self {
        Self {
            id: None,
            kind,
            adapter: AdapterFamily::OpenaiCompatible,
            auth_mode: AuthMode::Api,
            name: name.into(),
            base_url: base_url.into(),
            secret_id: None,
            account_id: None,
            capabilities,
            settings: ProviderSettings::default(),
            enabled: true,
            status: ProviderStatus::Unknown,
            status_detail: None,
            last_probe_at: None,
            last_latency_ms: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProviderPatch {
    pub kind: Option<ProviderKind>,
    pub adapter: Option<AdapterFamily>,
    pub auth_mode: Option<AuthMode>,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub secret_id: Option<Option<String>>,
    pub account_id: Option<Option<String>>,
    pub capabilities: Option<Vec<ProviderCapability>>,
    pub settings: Option<ProviderSettings>,
    pub enabled: Option<bool>,
    pub status: Option<ProviderStatus>,
    pub status_detail: Option<Option<String>>,
    pub last_probe_at: Option<Option<Timestamp>>,
    pub last_latency_ms: Option<Option<i64>>,
    pub updated_at: Option<Timestamp>,
}

impl ProviderPatch {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    fn assignments(&self) -> rusqlite::Result<Vec<(&'static str, SqlValue)>> {
        let text = |value: &str| SqlValue::Text(value.to_owned());
        let nullable_text = |value: &Option<String>| value.as_deref().map_or(SqlValue::Null, text);
        let nullable_integer =
            |value: &Option<i64>| value.map_or(SqlValue::Null, SqlValue::Integer);
        let mut assignments = Vec::new();
        let mut push = |column: &'static str, value: Option<SqlValue>| {
            if let Some(value) = value {
                assignments.push((column, value));
            }
        };
        push("kind", self.kind.map(|kind| text(kind.as_str())));
        push(
            "adapter",
            self.adapter.map(|adapter| text(adapter.as_str())),
        );
        push("auth_mode", self.auth_mode.map(|mode| text(mode.as_str())));
        push("name", self.name.as_deref().map(text));
        push("base_url", self.base_url.as_deref().map(text));
        push("secret_id", self.secret_id.as_ref().map(nullable_text));
        push("account_id", self.account_id.as_ref().map(nullable_text));
        push(
            "settings",
            self.settings
                .as_ref()
                .map(|settings| json_text(&settings.0).map(SqlValue::Text))
                .transpose()?,
        );
        push(
            "enabled",
            self.enabled
                .map(|enabled| SqlValue::Integer(enabled.into())),
        );
        push("status", self.status.map(|status| text(status.as_str())));
        push(
            "status_detail",
            self.status_detail.as_ref().map(nullable_text),
        );
        push(
            "last_probe_at",
            self.last_probe_at.as_ref().map(nullable_integer),
        );
        push(
            "last_latency_ms",
            self.last_latency_ms.as_ref().map(nullable_integer),
        );
        Ok(assignments)
    }
}

fn unique(capabilities: &[ProviderCapability]) -> Vec<ProviderCapability> {
    let mut kept = Vec::new();
    for capability in capabilities {
        if !kept.contains(capability) {
            kept.push(*capability);
        }
    }
    kept
}

struct Integrity<'a> {
    id: &'a str,
    auth_mode: &'a str,
    secret_id: Option<&'a str>,
    account_id: Option<&'a str>,
    status: ProviderStatus,
}

fn require_auth_mode_integrity(row: &Integrity<'_>) -> Result<(), AppError> {
    let failure = |message: &str| {
        AppError::new(AppErrorCode::ValidationFailed, message)
            .with_detail("providerId", row.id)
            .with_detail("authMode", row.auth_mode)
            .with_detail("hasSecret", row.secret_id.is_some())
            .with_detail("hasAccount", row.account_id.is_some())
    };
    if row.auth_mode == AuthMode::Account.as_str() {
        if row.secret_id.is_some() {
            return Err(failure(
                "Провайдер в режиме аккаунта не может ссылаться на секрет",
            ));
        }
        if row.account_id.is_none()
            && !matches!(
                row.status,
                ProviderStatus::NeedsRelink | ProviderStatus::Failed
            )
        {
            return Err(failure(
                "Провайдер в режиме аккаунта должен быть привязан к аккаунту",
            ));
        }
        return Ok(());
    }
    if row.account_id.is_some() {
        return Err(failure(
            "Провайдер в режиме API не может быть привязан к аккаунту",
        ));
    }
    Ok(())
}

pub fn model_not_owned() -> AppError {
    AppError::new(AppErrorCode::Conflict, "Модель не принадлежит провайдеру")
}

pub struct ProviderRepository<'c> {
    connection: &'c Connection,
}

impl<'c> ProviderRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self, filter: &ProviderFilter) -> rusqlite::Result<Vec<ProviderRow>> {
        let mut conditions = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if let Some(capability) = filter.capability {
            conditions.push(match capability {
                ProviderCapability::Text => "cap_text = ?",
                ProviderCapability::Embedding => "cap_embedding = ?",
                ProviderCapability::Image => "cap_image = ?",
            });
            values.push(SqlValue::Integer(1));
        }
        if let Some(enabled) = filter.enabled {
            conditions.push("enabled = ?");
            values.push(SqlValue::Integer(enabled.into()));
        }
        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM provider{clause} ORDER BY name ASC, id ASC",
                ProviderRow::COLUMNS
            ))?
            .query_map(params_from_iter(values), ProviderRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<ProviderRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM provider WHERE id = ?1",
                ProviderRow::COLUMNS
            ))?
            .query_row([id], ProviderRow::from_row)
            .optional()
    }

    pub fn create(&self, draft: &ProviderDraft) -> Result<ProviderRow, DataError> {
        let id = draft
            .id
            .clone()
            .unwrap_or_else(|| new_id().hyphenated().to_string());
        require_auth_mode_integrity(&Integrity {
            id: &id,
            auth_mode: draft.auth_mode.as_str(),
            secret_id: draft.secret_id.as_deref(),
            account_id: draft.account_id.as_deref(),
            status: draft.status,
        })?;
        let capabilities = unique(&draft.capabilities);
        let row = self
            .connection
            .prepare_cached(&format!(
                "INSERT INTO provider (id, kind, adapter, auth_mode, name, base_url, secret_id, account_id, capabilities, cap_text, cap_embedding, cap_image, enabled, settings, status, status_detail, last_probe_at, last_latency_ms, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20) RETURNING {}",
                ProviderRow::COLUMNS
            ))?
            .query_row(
                params![
                    id,
                    EnumColumn(draft.kind),
                    EnumColumn(draft.adapter),
                    EnumColumn(draft.auth_mode),
                    draft.name,
                    draft.base_url,
                    draft.secret_id,
                    draft.account_id,
                    json_text(&capabilities)?,
                    capabilities.contains(&ProviderCapability::Text),
                    capabilities.contains(&ProviderCapability::Embedding),
                    capabilities.contains(&ProviderCapability::Image),
                    draft.enabled,
                    draft.settings,
                    EnumColumn(draft.status),
                    draft.status_detail,
                    draft.last_probe_at,
                    draft.last_latency_ms,
                    draft.created_at,
                    draft.updated_at,
                ],
                ProviderRow::from_row,
            )?;
        if let Some(secret_id) = &row.secret_id {
            SecretRepository::new(self.connection).add_usage(&SecretUsageDraft {
                secret_id,
                consumer_kind: SecretConsumerKind::Provider,
                consumer_id: &row.id,
                created_at: row.created_at,
            })?;
        }
        Ok(row)
    }

    pub fn update(
        &self,
        id: &str,
        patch: &ProviderPatch,
        now: Timestamp,
    ) -> Result<Option<ProviderRow>, DataError> {
        let Some(previous) = self.find_by_id(id)? else {
            return Ok(None);
        };
        if patch.is_empty() {
            return Ok(Some(previous));
        }
        let auth_mode = patch
            .auth_mode
            .map_or(previous.auth_mode.as_str(), |mode| mode.as_str());
        let secret_id = patch
            .secret_id
            .as_ref()
            .map_or(previous.secret_id.as_deref(), Option::as_deref);
        let account_id = patch
            .account_id
            .as_ref()
            .map_or(previous.account_id.as_deref(), Option::as_deref);
        require_auth_mode_integrity(&Integrity {
            id,
            auth_mode,
            secret_id,
            account_id,
            status: patch.status.unwrap_or(previous.status),
        })?;
        let capabilities = unique(
            patch
                .capabilities
                .as_deref()
                .unwrap_or(&previous.capabilities),
        );
        let mut assignments = patch.assignments()?;
        assignments.extend([
            ("capabilities", SqlValue::Text(json_text(&capabilities)?)),
            (
                "cap_text",
                SqlValue::Integer(capabilities.contains(&ProviderCapability::Text).into()),
            ),
            (
                "cap_embedding",
                SqlValue::Integer(capabilities.contains(&ProviderCapability::Embedding).into()),
            ),
            (
                "cap_image",
                SqlValue::Integer(capabilities.contains(&ProviderCapability::Image).into()),
            ),
            (
                "updated_at",
                SqlValue::Integer(patch.updated_at.unwrap_or(now)),
            ),
        ]);
        let set = assignments
            .iter()
            .map(|(column, _)| format!("{column} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = assignments
            .into_iter()
            .map(|(_, value)| value)
            .chain([SqlValue::Text(id.to_owned())]);
        let Some(row) = self
            .connection
            .prepare_cached(&format!(
                "UPDATE provider SET {set} WHERE id = ? RETURNING {}",
                ProviderRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), ProviderRow::from_row)
            .optional()?
        else {
            return Ok(None);
        };
        if previous.secret_id != row.secret_id {
            let secrets = SecretRepository::new(self.connection);
            if let Some(old) = &previous.secret_id {
                secrets.remove_usage(old, SecretConsumerKind::Provider, id)?;
            }
            if let Some(new) = &row.secret_id {
                secrets.add_usage(&SecretUsageDraft {
                    secret_id: new,
                    consumer_kind: SecretConsumerKind::Provider,
                    consumer_id: id,
                    created_at: row.updated_at,
                })?;
            }
        }
        Ok(Some(row))
    }

    pub fn count_by_account(&self, account_id: &str) -> rusqlite::Result<u64> {
        self.connection
            .prepare_cached("SELECT count(*) FROM provider WHERE account_id = ?1")?
            .query_row([account_id], |row| row.get::<_, i64>(0))
            .map(i64::unsigned_abs)
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        if let Some(secret_id) = self.find_by_id(id)?.and_then(|row| row.secret_id) {
            SecretRepository::new(self.connection).remove_usage(
                &secret_id,
                SecretConsumerKind::Provider,
                id,
            )?;
        }
        self.connection
            .prepare_cached("DELETE FROM provider WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    pub fn set_status(
        &self,
        id: &str,
        status: ProviderStatus,
        detail: Option<String>,
        latency: Option<i64>,
        now: Timestamp,
    ) -> Result<Option<ProviderRow>, DataError> {
        self.update(
            id,
            &ProviderPatch {
                status: Some(status),
                status_detail: Some(detail),
                last_latency_ms: Some(latency),
                last_probe_at: Some(Some(now)),
                updated_at: Some(now),
                ..ProviderPatch::default()
            },
            now,
        )
    }

    pub fn set_default_model(
        &self,
        id: &str,
        model_id: Option<&str>,
        now: Timestamp,
    ) -> Result<Option<ProviderRow>, DataError> {
        if self.find_by_id(id)?.is_none() {
            return Ok(None);
        }
        if let Some(model_id) = model_id {
            let owned = self
                .connection
                .prepare_cached("SELECT 1 FROM model WHERE id = ?1 AND provider_id = ?2")?
                .query_row(params![model_id, id], |_| Ok(()))
                .optional()?
                .is_some();
            if !owned {
                return Err(model_not_owned().into());
            }
        }
        Ok(self
            .connection
            .prepare_cached(&format!(
                "UPDATE provider SET default_model_id = ?1, updated_at = ?2 WHERE id = ?3 RETURNING {}",
                ProviderRow::COLUMNS
            ))?
            .query_row(params![model_id, now, id], ProviderRow::from_row)
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::data::repositories::{ModelDraft, Repositories, SecretDraft};
    use crate::testing::{CLOCK_ORIGIN, temporary_database};
    use zvs_contract::SecretScope;

    const NOW: Timestamp = CLOCK_ORIGIN;

    fn draft() -> ProviderDraft {
        ProviderDraft::new(
            ProviderKind::Ollama,
            "Local",
            "http://localhost:11434",
            vec![ProviderCapability::Text],
            100,
        )
    }

    fn secret(repositories: &Repositories<'_>, name: &str) -> rusqlite::Result<String> {
        Ok(repositories
            .secrets
            .create(&SecretDraft {
                id: None,
                r#type: "custom".to_owned(),
                name: name.to_owned(),
                scope: SecretScope::Personal,
                cipher: None,
                cipher_version: 1,
                hint: None,
                fields: "{}".to_owned(),
                tags: "[]".to_owned(),
                note: None,
                rotation_days: None,
                rotates_at: None,
                created_at: 100,
                updated_at: 100,
            })?
            .id)
    }

    fn message(error: DataError) -> String {
        error.to_string()
    }

    #[test]
    fn capabilities_stay_synchronised_and_filters_combine_with_enabled_state() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let providers = Repositories::new(connection).providers;
                let text = providers.create(&draft())?;
                let multi = providers.create(&ProviderDraft {
                    name: "Multi".to_owned(),
                    capabilities: vec![
                        ProviderCapability::Embedding,
                        ProviderCapability::Image,
                        ProviderCapability::Image,
                    ],
                    enabled: false,
                    ..draft()
                })?;
                assert_eq!(
                    multi.capabilities,
                    [ProviderCapability::Embedding, ProviderCapability::Image]
                );
                let filter = |capability, enabled| ProviderFilter {
                    capability: Some(capability),
                    enabled,
                };
                assert_eq!(
                    providers.list(&filter(ProviderCapability::Text, None))?,
                    std::slice::from_ref(&text)
                );
                assert_eq!(
                    providers.list(&filter(ProviderCapability::Embedding, Some(false)))?,
                    [multi]
                );
                assert!(
                    providers
                        .list(&filter(ProviderCapability::Image, Some(true)))?
                        .is_empty()
                );
                assert_eq!(providers.list(&ProviderFilter::default())?.len(), 2);
                providers.update(
                    &text.id,
                    &ProviderPatch {
                        capabilities: Some(vec![ProviderCapability::Image]),
                        settings: Some(
                            ProviderSettings::default()
                                .with("temperature", json!(0.7))
                                .with("topK", json!(40))
                                .with("topP", json!(0.95))
                                .with("maxOutputTokens", json!(4096))
                                .with("timeoutSeconds", json!(60)),
                        ),
                        ..ProviderPatch::default()
                    },
                    NOW,
                )?;
                let updated = providers.find_by_id(&text.id)?.expect("a row");
                assert!(!updated.cap_text);
                assert!(!updated.cap_embedding);
                assert!(updated.cap_image);
                assert_eq!(updated.settings.timeout_seconds(), Some(60.0));
                assert_eq!(updated.settings.timeout_text(), "60");
                assert_eq!(updated.updated_at, NOW);
                assert!(
                    providers
                        .list(&filter(ProviderCapability::Text, None))?
                        .is_empty()
                );
                providers.update(
                    &text.id,
                    &ProviderPatch {
                        capabilities: Some(Vec::new()),
                        ..ProviderPatch::default()
                    },
                    NOW,
                )?;
                assert!(!providers.find_by_id(&text.id)?.expect("a row").cap_image);
                for (capability, column) in [
                    ("text", "cap_text"),
                    ("embedding", "cap_embedding"),
                    ("image", "cap_image"),
                ] {
                    let plan = connection
                        .prepare(&format!(
                            "EXPLAIN QUERY PLAN SELECT * FROM provider WHERE {column} = 1"
                        ))?
                        .query_map([], |row| row.get::<_, String>(3))?
                        .collect::<rusqlite::Result<Vec<_>>>()?
                        .join(" ");
                    assert!(
                        plan.contains(&format!("provider_cap_{capability}_idx")),
                        "{plan}"
                    );
                }
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn names_are_unique_within_each_kind_and_failed_writes_retain_secret_usage() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let key = secret(&repositories, "Key")?;
                let providers = &repositories.providers;
                let row = providers.create(&ProviderDraft {
                    secret_id: Some(key.clone()),
                    ..draft()
                })?;
                let duplicate = providers
                    .create(&ProviderDraft {
                        secret_id: Some(key.clone()),
                        ..draft()
                    })
                    .expect_err("refuses");
                assert!(message(duplicate).contains("UNIQUE"));
                providers.create(&ProviderDraft {
                    kind: ProviderKind::OpenaiCompatible,
                    ..draft()
                })?;
                assert_eq!(repositories.secrets.count_usage(&key)?, 1);
                let dangling = providers
                    .update(
                        &row.id,
                        &ProviderPatch {
                            secret_id: Some(Some("missing".to_owned())),
                            ..ProviderPatch::default()
                        },
                        NOW,
                    )
                    .expect_err("refuses");
                assert!(message(dangling).contains("FOREIGN KEY"));
                assert_eq!(
                    providers.find_by_id(&row.id)?.expect("a row").secret_id,
                    Some(key.clone())
                );
                assert_eq!(repositories.secrets.count_usage(&key)?, 1);
                let missing = providers
                    .create(&ProviderDraft {
                        name: "Missing".to_owned(),
                        secret_id: Some("missing".to_owned()),
                        ..draft()
                    })
                    .expect_err("refuses");
                assert!(message(missing).contains("FOREIGN KEY"));
                assert_eq!(providers.list(&ProviderFilter::default())?.len(), 2);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn secret_usage_follows_repointing_clearing_and_provider_removal() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let first = secret(&repositories, "First")?;
                let second = secret(&repositories, "Second")?;
                let (providers, secrets) = (&repositories.providers, &repositories.secrets);
                let row = providers.create(&ProviderDraft {
                    secret_id: Some(first.clone()),
                    ..draft()
                })?;
                let usage = secrets.list_usage(&first)?;
                assert_eq!(usage[0].consumer_kind, SecretConsumerKind::Provider);
                assert_eq!(usage[0].consumer_id, row.id);
                assert_eq!(
                    secrets
                        .remove(&first)
                        .map_err(message)
                        .expect_err("refuses"),
                    "CONFLICT: Секрет используется (1) и не может быть удалён"
                );
                assert!(
                    connection
                        .execute("DELETE FROM secret WHERE id = ?1", [&first])
                        .expect_err("restricted")
                        .to_string()
                        .contains("FOREIGN KEY")
                );
                let repoint = |secret_id: Option<String>| ProviderPatch {
                    secret_id: Some(secret_id),
                    ..ProviderPatch::default()
                };
                providers.update(&row.id, &repoint(Some(second.clone())), NOW)?;
                assert_eq!(secrets.count_usage(&first)?, 0);
                assert_eq!(secrets.count_usage(&second)?, 1);
                providers.update(
                    &row.id,
                    &ProviderPatch {
                        name: Some("Renamed".to_owned()),
                        ..ProviderPatch::default()
                    },
                    NOW,
                )?;
                assert_eq!(secrets.count_usage(&second)?, 1);
                providers.update(&row.id, &repoint(None), NOW)?;
                assert_eq!(secrets.count_usage(&second)?, 0);
                providers.update(&row.id, &repoint(Some(first.clone())), NOW)?;
                providers.remove(&row.id)?;
                assert_eq!(secrets.count_usage(&first)?, 0);
                secrets.remove(&first)?;
                assert_eq!(secrets.find_by_id(&first)?, None);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn provider_and_usage_changes_participate_in_an_enclosing_unit_of_work() {
        let database = temporary_database();
        let key = database
            .with_connection(|connection| Ok(secret(&Repositories::new(connection), "Key")?))
            .expect("a secret");
        let aborted = database.transaction(|transaction| {
            Repositories::new(transaction)
                .providers
                .create(&ProviderDraft {
                    secret_id: Some(key.clone()),
                    ..draft()
                })?;
            Err::<(), DataError>(AppError::new(AppErrorCode::Unknown, "abort").into())
        });
        assert!(aborted.is_err());
        let row = database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                assert!(
                    repositories
                        .providers
                        .list(&ProviderFilter::default())?
                        .is_empty()
                );
                assert_eq!(repositories.secrets.count_usage(&key)?, 0);
                repositories.providers.create(&ProviderDraft {
                    secret_id: Some(key.clone()),
                    ..draft()
                })
            })
            .expect("creates");
        let aborted = database.transaction(|transaction| {
            Repositories::new(transaction).providers.remove(&row.id)?;
            Err::<(), DataError>(AppError::new(AppErrorCode::Unknown, "abort").into())
        });
        assert!(aborted.is_err());
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                assert_eq!(
                    repositories
                        .providers
                        .find_by_id(&row.id)?
                        .expect("a row")
                        .secret_id,
                    Some(key.clone())
                );
                assert_eq!(repositories.secrets.count_usage(&key)?, 1);
                Ok(())
            })
            .expect("reads");
    }

    #[test]
    fn default_models_must_belong_to_the_provider_and_deletion_cascades_models() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let (providers, models) = (&repositories.providers, &repositories.models);
                let row = providers.create(&draft())?;
                let other = providers.create(&ProviderDraft {
                    name: "Other".to_owned(),
                    ..draft()
                })?;
                let entry = models
                    .replace_for_provider(&row.id, &[ModelDraft::new("gpt-oss:120b", "GPT OSS", 200)], NOW)?
                    .remove(0);
                let foreign = providers
                    .set_default_model(&other.id, Some(&entry.id), NOW)
                    .expect_err("refuses");
                assert!(matches!(foreign, DataError::App(ref error) if error.code == AppErrorCode::Conflict));
                assert!(providers.set_default_model(&row.id, Some("missing"), NOW).is_err());
                providers.set_default_model(&row.id, Some(&entry.id), NOW)?;
                providers.set_default_model(&row.id, None, NOW)?;
                assert_eq!(providers.find_by_id(&row.id)?.expect("a row").default_model_id, None);
                providers.remove(&row.id)?;
                assert_eq!(models.find_by_id(&entry.id)?, None);
                assert!(providers.find_by_id(&other.id)?.is_some());
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn status_records_probe_time_and_clears_previous_details_and_latency() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let providers = Repositories::new(connection).providers;
                let row = providers.create(&draft())?;
                assert_eq!(row.status, ProviderStatus::Unknown);
                providers.set_status(
                    &row.id,
                    ProviderStatus::Failed,
                    Some("Connection refused".to_owned()),
                    Some(412),
                    200,
                )?;
                let failed = providers.find_by_id(&row.id)?.expect("a row");
                assert_eq!(failed.last_probe_at, Some(200));
                assert_eq!(failed.last_latency_ms, Some(412));
                assert_eq!(failed.status_detail.as_deref(), Some("Connection refused"));
                let ok = providers
                    .set_status(&row.id, ProviderStatus::Ok, None, None, 300)?
                    .expect("a row");
                assert_eq!(ok.status, ProviderStatus::Ok);
                assert_eq!(ok.status_detail, None);
                assert_eq!(ok.last_latency_ms, None);
                assert_eq!(ok.updated_at, 300);
                assert_eq!(
                    providers.update(
                        "missing",
                        &ProviderPatch {
                            enabled: Some(false),
                            ..ProviderPatch::default()
                        },
                        NOW
                    )?,
                    None
                );
                assert_eq!(providers.set_default_model("missing", None, NOW)?, None);
                providers.remove("missing")?;
                assert_eq!(
                    providers.update(&row.id, &ProviderPatch::default(), NOW + 1)?,
                    Some(ok)
                );
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn the_adapter_and_auth_mode_default_and_a_stray_value_is_kept_verbatim() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let providers = Repositories::new(connection).providers;
                let row = providers.create(&draft())?;
                assert_eq!(row.adapter, "openai-compatible");
                assert_eq!(row.auth_mode, "api");
                assert_eq!(row.adapter_family(), Some(AdapterFamily::OpenaiCompatible));
                let qwen = providers
                    .update(
                        &row.id,
                        &ProviderPatch {
                            adapter: Some(AdapterFamily::QwenWeb),
                            ..ProviderPatch::default()
                        },
                        NOW,
                    )?
                    .expect("a row");
                assert_eq!(qwen.adapter, "qwen-web");
                connection.execute(
                    "UPDATE provider SET adapter = 'made-up' WHERE id = ?1",
                    [&row.id],
                )?;
                let stray = providers.find_by_id(&row.id)?.expect("a row");
                assert_eq!(stray.adapter, "made-up");
                assert_eq!(stray.adapter_family(), None);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn auth_mode_integrity_is_checked_before_anything_is_written() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let key = secret(&repositories, "Key")?;
                let providers = &repositories.providers;
                let account_with_secret = providers
                    .create(&ProviderDraft {
                        auth_mode: AuthMode::Account,
                        secret_id: Some(key.clone()),
                        ..draft()
                    })
                    .expect_err("refuses");
                let DataError::App(error) = account_with_secret else {
                    panic!("an app error");
                };
                assert_eq!(error.code, AppErrorCode::ValidationFailed);
                assert_eq!(
                    error.message,
                    "Провайдер в режиме аккаунта не может ссылаться на секрет"
                );
                assert_eq!(
                    error
                        .details
                        .as_ref()
                        .and_then(|details| details.get("hasSecret")),
                    Some(&json!(true))
                );
                let unlinked = providers
                    .create(&ProviderDraft {
                        auth_mode: AuthMode::Account,
                        ..draft()
                    })
                    .expect_err("refuses");
                assert!(message(unlinked).contains("должен быть привязан"));
                let relink = providers.create(&ProviderDraft {
                    auth_mode: AuthMode::Account,
                    status: ProviderStatus::NeedsRelink,
                    ..draft()
                })?;
                let mixed = providers
                    .update(
                        &relink.id,
                        &ProviderPatch {
                            auth_mode: Some(AuthMode::Api),
                            account_id: Some(Some("account-1".to_owned())),
                            ..ProviderPatch::default()
                        },
                        NOW,
                    )
                    .expect_err("refuses");
                assert!(message(mixed).contains("режиме API"));
                assert_eq!(providers.list(&ProviderFilter::default())?.len(), 1);
                assert_eq!(repositories.secrets.count_usage(&key)?, 0);
                Ok(())
            })
            .expect("the repository works");
    }
}
