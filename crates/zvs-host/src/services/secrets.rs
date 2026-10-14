use std::sync::Arc;

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, RotationStatus, SecretDto, SecretFields, SecretId, SecretScope,
    SecretSummaryDto, SecretTypeKey, Timestamp,
};

use crate::data::repositories::{
    SecretDraft, SecretFilter, SecretPatch, SecretUsageDraft, secret_in_use,
};
use crate::data::schema::{SecretConsumerKind, SecretRow};
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::services::crypto::{CryptoService, Plaintext};
use crate::services::secret_policy::{compute_hint, rotation_due_at, rotation_status};

pub const CIPHER_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretConsumer {
    pub kind: SecretConsumerKind,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretSummary {
    pub id: String,
    pub r#type: String,
    pub name: String,
    pub scope: SecretScope,
    pub cipher_version: i64,
    pub hint: Option<String>,
    pub usage_count: u64,
    pub fields: SecretFields,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub rotation_days: Option<i64>,
    pub rotates_at: Option<Timestamp>,
    pub rotation: RotationStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretCreateInput {
    pub r#type: String,
    pub name: String,
    pub scope: SecretScope,
    pub value: Option<Plaintext>,
    pub fields: SecretFields,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub rotation_days: Option<i64>,
    pub rotates_at: Option<Timestamp>,
}

impl SecretCreateInput {
    pub fn new(r#type: impl Into<String>, name: impl Into<String>, scope: SecretScope) -> Self {
        Self {
            r#type: r#type.into(),
            name: name.into(),
            scope,
            value: None,
            fields: SecretFields::new(),
            tags: Vec::new(),
            note: None,
            rotation_days: None,
            rotates_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretUpdateInput {
    pub name: Option<String>,
    pub scope: Option<SecretScope>,
    pub value: Option<Plaintext>,
    pub fields: Option<SecretFields>,
    pub tags: Option<Vec<String>>,
    pub note: Option<Option<String>>,
    pub rotation_days: Option<Option<i64>>,
    pub rotates_at: Option<Option<Timestamp>>,
}

pub struct SecretService {
    database: Arc<Database>,
    crypto: Arc<CryptoService>,
    clock: SharedClock,
    logger: SharedLogger,
}

impl SecretService {
    pub fn new(
        database: Arc<Database>,
        crypto: Arc<CryptoService>,
        clock: SharedClock,
        logger: SharedLogger,
    ) -> Self {
        Self {
            database,
            crypto,
            clock,
            logger,
        }
    }

    pub fn is_encryption_available(&self) -> bool {
        self.crypto.is_available()
    }

    pub fn list(&self, filter: &SecretFilter<'_>) -> Result<Vec<SecretSummary>, AppError> {
        let now = self.clock.now();
        let (rows, usage) = self.database.with_connection(|connection| {
            let secrets = Repositories::new(connection).secrets;
            Ok((
                secrets.list_summaries(filter)?,
                secrets.count_usage_by_secret()?,
            ))
        })?;
        rows.into_iter()
            .map(|row| {
                let count = usage.get(&row.id).copied().unwrap_or(0);
                to_summary(row, now, count)
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<SecretSummary, AppError> {
        let now = self.clock.now();
        let (row, count) = self.database.with_connection(|connection| {
            let secrets = Repositories::new(connection).secrets;
            let row = secrets.find_by_id(id)?.ok_or_else(|| not_found(id))?;
            Ok((row, secrets.count_usage(id)?))
        })?;
        to_summary(row, now, count)
    }

    pub fn create(&self, input: SecretCreateInput) -> Result<SecretSummary, AppError> {
        let name = require_text(&input.name, "name")?;
        let value = input.value.as_ref().map(require_value).transpose()?;
        let now = self.clock.now();
        let cipher = value.map(|value| self.crypto.encrypt(value)).transpose()?;
        let draft = SecretDraft {
            id: None,
            r#type: input.r#type,
            name,
            scope: input.scope,
            cipher,
            cipher_version: CIPHER_VERSION,
            hint: value.map(|value| compute_hint(value.expose())),
            fields: Value::Object(input.fields).to_string(),
            tags: json!(input.tags).to_string(),
            note: input.note,
            rotation_days: input.rotation_days,
            rotates_at: input
                .rotates_at
                .or_else(|| rotation_due_at(input.rotation_days, now)),
            created_at: now,
            updated_at: now,
        };
        let created = self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).secrets.create(&draft)?)
        })?;
        self.logger.log(
            LogLevel::Info,
            "secrets",
            "Created a secret",
            &[
                ("secretId", json!(created.id)),
                ("type", json!(created.r#type)),
            ],
        );
        to_summary(created, now, 0)
    }

    pub fn update(&self, id: &str, input: SecretUpdateInput) -> Result<SecretSummary, AppError> {
        self.require(id)?;
        let now = self.clock.now();
        let mut patch = SecretPatch {
            updated_at: Some(now),
            ..SecretPatch::default()
        };
        if let Some(name) = &input.name {
            patch.name = Some(require_text(name, "name")?);
        }
        patch.scope = input.scope;
        patch.fields = input.fields.map(|fields| Value::Object(fields).to_string());
        patch.tags = input.tags.map(|tags| json!(tags).to_string());
        patch.note = input.note;
        if let Some(rotation_days) = input.rotation_days {
            patch.rotation_days = Some(rotation_days);
            patch.rotates_at = Some(
                input
                    .rotates_at
                    .flatten()
                    .or_else(|| rotation_due_at(rotation_days, now)),
            );
        } else if let Some(rotates_at) = input.rotates_at {
            patch.rotates_at = Some(rotates_at);
        }
        let replaced_value = input.value.is_some();
        if let Some(value) = &input.value {
            let value = require_value(value)?;
            patch.cipher = Some(Some(self.crypto.encrypt(value)?));
            patch.hint = Some(Some(compute_hint(value.expose())));
        }
        let (updated, count) = self.database.transaction(|transaction| {
            let secrets = Repositories::new(transaction).secrets;
            let updated = secrets.update(id, &patch)?.ok_or_else(|| not_found(id))?;
            Ok((updated, secrets.count_usage(id)?))
        })?;
        self.logger.log(
            LogLevel::Info,
            "secrets",
            "Updated a secret",
            &[
                ("secretId", json!(id)),
                ("replacedValue", json!(replaced_value)),
            ],
        );
        to_summary(updated, now, count)
    }

    pub fn remove(&self, id: &str) -> Result<(), AppError> {
        self.database.transaction(|transaction| {
            let secrets = Repositories::new(transaction).secrets;
            secrets.find_by_id(id)?.ok_or_else(|| not_found(id))?;
            let usage = secrets.list_usage(id)?;
            if !usage.is_empty() {
                return Err(secret_in_use(id, &usage).into());
            }
            secrets.remove(id)
        })?;
        self.logger.log(
            LogLevel::Info,
            "secrets",
            "Removed a secret",
            &[("secretId", json!(id))],
        );
        Ok(())
    }

    pub fn usage(&self, id: &str) -> Result<Vec<SecretConsumer>, AppError> {
        Ok(self
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).secrets.list_usage(id)?)
            })?
            .into_iter()
            .map(|entry| SecretConsumer {
                kind: entry.consumer_kind,
                id: entry.consumer_id,
            })
            .collect())
    }

    pub fn add_usage(
        &self,
        secret_id: &str,
        kind: SecretConsumerKind,
        consumer_id: &str,
    ) -> Result<(), AppError> {
        let created_at = self.clock.now();
        self.database.transaction(|transaction| {
            let secrets = Repositories::new(transaction).secrets;
            secrets
                .find_by_id(secret_id)?
                .ok_or_else(|| not_found(secret_id))?;
            secrets.add_usage(&SecretUsageDraft {
                secret_id,
                consumer_kind: kind,
                consumer_id,
                created_at,
            })?;
            Ok(())
        })
    }

    pub fn remove_usage(
        &self,
        secret_id: &str,
        kind: SecretConsumerKind,
        consumer_id: &str,
    ) -> Result<(), AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .secrets
                .remove_usage(secret_id, kind, consumer_id)?)
        })
    }

    pub fn resolve(&self, id: &str) -> Result<Plaintext, AppError> {
        let row = self
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .secrets
                    .find_cipher_by_id(id)?)
            })?
            .ok_or_else(|| not_found(id))?;
        match row.cipher.as_deref() {
            Some(cipher) if !cipher.is_empty() => self.crypto.decrypt(cipher),
            _ => Err(AppError::new(
                AppErrorCode::SecretMissing,
                "У секрета нет сохранённого значения",
            )
            .with_detail("secretId", id)),
        }
    }

    fn require(&self, id: &str) -> Result<SecretRow, AppError> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).secrets.find_by_id(id)?)
            })?
            .ok_or_else(|| not_found(id))
    }
}

fn to_summary(row: SecretRow, now: Timestamp, usage_count: u64) -> Result<SecretSummary, AppError> {
    let fields = match parse(&row.id, "fields", &row.fields)? {
        Value::Object(fields) => fields,
        _ => SecretFields::new(),
    };
    let tags = match parse(&row.id, "tags", &row.tags)? {
        Value::Array(tags) => tags
            .into_iter()
            .filter_map(|tag| match tag {
                Value::String(tag) => Some(tag),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(SecretSummary {
        rotation: rotation_status(row.rotates_at, now),
        id: row.id,
        r#type: row.r#type,
        name: row.name,
        scope: row.scope,
        cipher_version: row.cipher_version,
        hint: row.hint,
        usage_count,
        fields,
        tags,
        note: row.note,
        rotation_days: row.rotation_days,
        rotates_at: row.rotates_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn parse(id: &str, column: &str, raw: &str) -> Result<Value, AppError> {
    serde_json::from_str(raw).map_err(|_| {
        AppError::new(AppErrorCode::DbError, "Секрет хранится в повреждённом виде")
            .with_detail("secretId", id)
            .with_detail("column", column)
    })
}

fn empty_field(field: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, "Поле не может быть пустым")
        .with_detail("field", field)
}

fn require_text(value: &str, field: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(empty_field(field))
    } else {
        Ok(trimmed.to_owned())
    }
}

fn require_value(value: &Plaintext) -> Result<&Plaintext, AppError> {
    if value.expose().trim().is_empty() {
        Err(empty_field("value"))
    } else {
        Ok(value)
    }
}

pub fn not_found(id: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, "Секрет не найден").with_detail("secretId", id)
}

pub fn public_not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, "Секрет не найден")
}

pub fn to_secret_summary_dto(summary: &SecretSummary) -> Result<SecretSummaryDto, AppError> {
    let r#type = SecretTypeKey::parse(&summary.r#type).ok_or_else(public_not_found)?;
    let id = SecretId::parse(&summary.id).ok_or_else(|| {
        AppError::new(AppErrorCode::DbError, "Секрет хранится в повреждённом виде")
            .with_detail("secretId", summary.id.clone())
            .with_detail("column", "id")
    })?;
    Ok(SecretSummaryDto {
        id,
        r#type,
        name: summary.name.clone(),
        scope: summary.scope,
        hint: summary.hint.clone(),
        tags: summary.tags.clone(),
        usage_count: summary.usage_count,
        rotation_status: summary.rotation,
        rotates_at: summary.rotates_at,
        updated_at: summary.updated_at,
    })
}

pub fn to_secret_dto(summary: &SecretSummary) -> Result<SecretDto, AppError> {
    let base = to_secret_summary_dto(summary)?;
    Ok(SecretDto {
        id: base.id,
        r#type: base.r#type,
        name: base.name,
        scope: base.scope,
        hint: base.hint,
        tags: base.tags,
        usage_count: base.usage_count,
        rotation_status: base.rotation_status,
        rotates_at: base.rotates_at,
        updated_at: base.updated_at,
        fields: summary.fields.clone(),
        note: summary.note.clone(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::platform::clock::{Clock, FakeClock};
    use crate::platform::logger::{FileLogger, Logger, LoggerOptions, REDACTED, silent_logger};
    use crate::services::secret_policy::{DAY_MS, DUE_SOON_WINDOW_MS};
    use crate::testing::{FakeCrypto, TemporaryDatabase, temporary_database, temporary_directory};

    const VALUE: &str = "sk-or-v1-0a1b2c3d4e5f60718293a4b5c6d7e8f9c31";

    struct Harness {
        database: TemporaryDatabase,
        clock: Arc<FakeClock>,
        backend: Arc<FakeCrypto>,
        service: SecretService,
    }

    fn harness_with(logger: SharedLogger) -> Harness {
        let database = temporary_database();
        let clock = Arc::new(FakeClock::default());
        let backend = FakeCrypto::shared();
        let service = SecretService::new(
            database.shared(),
            Arc::new(CryptoService::new(backend.clone())),
            clock.clone(),
            logger,
        );
        Harness {
            database,
            clock,
            backend,
            service,
        }
    }

    fn harness() -> Harness {
        harness_with(silent_logger())
    }

    fn input() -> SecretCreateInput {
        SecretCreateInput {
            value: Some(Plaintext::new(VALUE)),
            fields: json!({ "baseUrl": "https://openrouter.ai/api/v1" })
                .as_object()
                .cloned()
                .expect("an object"),
            tags: vec!["llm".to_owned()],
            ..SecretCreateInput::new("openrouter", "OpenRouter — main", SecretScope::Personal)
        }
    }

    fn stored_cipher(database: &TemporaryDatabase, id: &str) -> Option<Vec<u8>> {
        database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .secrets
                    .find_cipher_by_id(id)?)
            })
            .expect("reads")
            .and_then(|row| row.cipher)
    }

    fn code(result: Result<impl std::fmt::Debug, AppError>) -> AppErrorCode {
        result.expect_err("fails").code
    }

    fn all(service: &SecretService) -> Vec<SecretSummary> {
        service.list(&SecretFilter::default()).expect("lists")
    }

    #[test]
    fn a_created_secret_round_trips_through_resolve_and_never_carries_its_value() {
        let Harness {
            database, service, ..
        } = harness();
        let created = service.create(input()).expect("creates");
        assert_eq!(created.hint.as_deref(), Some("sk-or-v1-…9c31"));
        assert_eq!(created.tags, ["llm"]);
        assert_eq!(
            Value::Object(created.fields.clone()),
            json!({ "baseUrl": "https://openrouter.ai/api/v1" })
        );
        assert_eq!(created.rotation, RotationStatus::Ok);
        assert_eq!(created.cipher_version, CIPHER_VERSION);

        for summary in all(&service) {
            assert!(!format!("{summary:?}").contains(VALUE));
            let dto = serde_json::to_value(to_secret_dto(&summary).expect("a dto")).expect("json");
            assert!(dto.get("value").is_none());
            assert!(dto.get("cipher").is_none());
            assert!(!dto.to_string().contains(VALUE));
        }

        assert_eq!(
            service.resolve(&created.id).expect("resolves").expose(),
            VALUE
        );

        let stored = stored_cipher(&database, &created.id).expect("a cipher");
        assert!(!String::from_utf8_lossy(&stored).contains(VALUE));
    }

    #[test]
    fn updating_anything_but_the_value_leaves_the_ciphertext_byte_identical() {
        let Harness {
            database,
            clock,
            backend,
            service,
        } = harness();
        let created = service.create(input()).expect("creates");
        let before = stored_cipher(&database, &created.id);
        let encrypt_calls = backend.encrypt_calls();

        clock.advance(1000);
        let renamed = service
            .update(
                &created.id,
                SecretUpdateInput {
                    name: Some("OpenRouter — lab".to_owned()),
                    note: Some(Some("shared box".to_owned())),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("updates");
        assert_eq!(renamed.name, "OpenRouter — lab");
        assert_eq!(renamed.note.as_deref(), Some("shared box"));
        assert_eq!(renamed.updated_at, created.updated_at + 1000);
        assert_eq!(renamed.hint, created.hint);
        assert_eq!(backend.encrypt_calls(), encrypt_calls);
        assert_eq!(stored_cipher(&database, &created.id), before);
        assert_eq!(
            service.resolve(&created.id).expect("resolves").expose(),
            VALUE
        );

        let replaced = service
            .update(
                &created.id,
                SecretUpdateInput {
                    value: Some(Plaintext::new("sk-or-v1-ffffffffffffffffffff1234")),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("updates");
        assert_eq!(backend.encrypt_calls(), encrypt_calls + 1);
        assert_eq!(replaced.hint.as_deref(), Some("sk-or-v1-…1234"));
        assert_eq!(
            service.resolve(&created.id).expect("resolves").expose(),
            "sk-or-v1-ffffffffffffffffffff1234"
        );
        assert_ne!(stored_cipher(&database, &created.id), before);
    }

    #[test]
    fn deleting_a_secret_in_use_is_refused_and_names_the_consumers() {
        let Harness { service, .. } = harness();
        let created = service.create(input()).expect("creates");
        service
            .add_usage(&created.id, SecretConsumerKind::Provider, "provider-1")
            .expect("adds");
        service
            .add_usage(&created.id, SecretConsumerKind::VectorStore, "store-1")
            .expect("adds");

        let error = service.remove(&created.id).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Conflict);
        assert_eq!(
            error.message,
            "Секрет используется (2) и не может быть удалён"
        );
        assert_eq!(
            error.details.map(Value::Object),
            Some(json!({
                "secretId": created.id,
                "consumers": [
                    { "kind": "provider", "id": "provider-1" },
                    { "kind": "vector_store", "id": "store-1" },
                ],
            }))
        );
        assert_eq!(
            service.get(&created.id).expect("still there").usage_count,
            2
        );

        service
            .remove_usage(&created.id, SecretConsumerKind::Provider, "provider-1")
            .expect("removes");
        service
            .remove_usage(&created.id, SecretConsumerKind::VectorStore, "store-1")
            .expect("removes");
        assert_eq!(service.usage(&created.id).expect("lists"), []);
        service.remove(&created.id).expect("removes");
        assert!(all(&service).is_empty());
    }

    #[test]
    fn missing_secrets_and_missing_ciphertext_are_reported_not_guessed_at() {
        let Harness {
            database, service, ..
        } = harness();
        assert_eq!(code(service.resolve("nope")), AppErrorCode::NotFound);
        assert_eq!(code(service.get("nope")), AppErrorCode::NotFound);
        assert_eq!(
            code(service.update(
                "nope",
                SecretUpdateInput {
                    name: Some("x".to_owned()),
                    ..SecretUpdateInput::default()
                }
            )),
            AppErrorCode::NotFound
        );
        assert_eq!(code(service.remove("nope")), AppErrorCode::NotFound);
        assert_eq!(
            code(service.add_usage("nope", SecretConsumerKind::Provider, "p")),
            AppErrorCode::NotFound
        );
        let blank = service
            .create(SecretCreateInput {
                value: Some(Plaintext::new("   ")),
                ..input()
            })
            .expect_err("refuses");
        assert_eq!(blank.code, AppErrorCode::ValidationFailed);
        assert_eq!(
            blank.details.map(Value::Object),
            Some(json!({ "field": "value" }))
        );
        assert_eq!(
            code(service.create(SecretCreateInput {
                name: "  ".to_owned(),
                ..input()
            })),
            AppErrorCode::ValidationFailed
        );
        let trimmed = service
            .create(SecretCreateInput {
                name: "  padded  ".to_owned(),
                ..input()
            })
            .expect("creates");
        assert_eq!(trimmed.name, "padded");

        let created = service.create(input()).expect("creates");
        database
            .with_connection(|connection| {
                connection.execute(
                    "UPDATE secret SET cipher = NULL WHERE id = ?1",
                    [&created.id],
                )?;
                Ok(())
            })
            .expect("clears");
        let missing = service.resolve(&created.id).expect_err("refuses");
        assert_eq!(missing.code, AppErrorCode::SecretMissing);
        assert_eq!(
            missing.details.map(Value::Object),
            Some(json!({ "secretId": created.id }))
        );
        database
            .with_connection(|connection| {
                connection.execute(
                    "UPDATE secret SET cipher = x'' WHERE id = ?1",
                    [&created.id],
                )?;
                Ok(())
            })
            .expect("empties");
        assert_eq!(
            code(service.resolve(&created.id)),
            AppErrorCode::SecretMissing
        );

        let bare = service
            .create(SecretCreateInput::new(
                "custom",
                "no value",
                SecretScope::Public,
            ))
            .expect("creates without a value");
        assert_eq!(bare.hint, None);
        assert_eq!(code(service.resolve(&bare.id)), AppErrorCode::SecretMissing);
    }

    #[test]
    fn an_unavailable_keychain_fails_loudly_instead_of_degrading_to_plaintext() {
        let Harness {
            backend, service, ..
        } = harness();
        let created = service.create(input()).expect("creates");
        backend.set_available(false);
        assert!(!service.is_encryption_available());
        assert_eq!(
            code(service.create(SecretCreateInput {
                name: "second".to_owned(),
                ..input()
            })),
            AppErrorCode::SecretDecryptFailed
        );
        assert_eq!(
            code(service.resolve(&created.id)),
            AppErrorCode::SecretDecryptFailed
        );
        assert_eq!(all(&service).len(), 1);

        backend.set_available(true);
        backend.fail_next();
        assert_eq!(
            code(service.resolve(&created.id)),
            AppErrorCode::SecretDecryptFailed
        );
        backend.fail_next();
        assert_eq!(
            code(service.update(
                &created.id,
                SecretUpdateInput {
                    value: Some(Plaintext::new("sk-or-v1-replacement-value")),
                    name: Some("renamed".to_owned()),
                    ..SecretUpdateInput::default()
                }
            )),
            AppErrorCode::SecretDecryptFailed
        );
        let unchanged = service.get(&created.id).expect("reads");
        assert_eq!(unchanged.name, created.name);
        assert_eq!(
            service.resolve(&created.id).expect("resolves").expose(),
            VALUE
        );
    }

    #[test]
    fn rotation_is_derived_from_days_and_turns_over_with_the_clock() {
        let Harness { clock, service, .. } = harness();
        let now = clock.now();
        let calm = service
            .create(SecretCreateInput {
                rotates_at: Some(now + DUE_SOON_WINDOW_MS + 1),
                ..input()
            })
            .expect("creates");
        assert_eq!(calm.rotation, RotationStatus::Ok);
        let derived = service
            .create(SecretCreateInput {
                name: "derived".to_owned(),
                rotation_days: Some(90),
                ..input()
            })
            .expect("creates");
        assert_eq!(derived.rotates_at, Some(now + 90 * DAY_MS));

        let main = SecretFilter {
            query: Some("main"),
            ..SecretFilter::default()
        };
        clock.advance(1);
        assert_eq!(
            service.list(&main).expect("lists")[0].rotation,
            RotationStatus::DueSoon
        );
        clock.set(calm.rotates_at.expect("a due date"));
        assert_eq!(
            service.list(&main).expect("lists")[0].rotation,
            RotationStatus::Overdue
        );

        let cleared = service
            .update(
                &derived.id,
                SecretUpdateInput {
                    rotation_days: Some(None),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("clears");
        assert_eq!((cleared.rotation_days, cleared.rotates_at), (None, None));
        let rescheduled = service
            .update(
                &derived.id,
                SecretUpdateInput {
                    rotation_days: Some(Some(30)),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("reschedules");
        assert_eq!(rescheduled.rotation_days, Some(30));
        assert_eq!(rescheduled.rotates_at, Some(clock.now() + 30 * DAY_MS));
        let pinned = service
            .update(
                &derived.id,
                SecretUpdateInput {
                    rotates_at: Some(Some(5)),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("pins");
        assert_eq!(
            (pinned.rotation_days, pinned.rotates_at, pinned.rotation),
            (Some(30), Some(5), RotationStatus::Overdue)
        );
    }

    #[test]
    fn a_corrupt_stored_column_is_a_db_error_naming_the_secret_and_column() {
        let Harness {
            database, service, ..
        } = harness();
        let created = service.create(input()).expect("creates");
        database
            .with_connection(|connection| {
                connection.execute(
                    "UPDATE secret SET tags = '[oops' WHERE id = ?1",
                    [&created.id],
                )?;
                Ok(())
            })
            .expect("corrupts");
        let error = service.get(&created.id).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::DbError);
        assert_eq!(
            error.details.map(Value::Object),
            Some(json!({ "secretId": created.id, "column": "tags" }))
        );
    }

    #[test]
    fn an_internal_type_has_no_public_dto() {
        let Harness { service, .. } = harness();
        let token = service
            .create(SecretCreateInput {
                value: Some(Plaintext::new("internal-account-credential")),
                ..SecretCreateInput::new(
                    "account-token",
                    "qwen-web account token",
                    SecretScope::Personal,
                )
            })
            .expect("creates");
        let error = to_secret_summary_dto(&token).expect_err("refuses");
        assert_eq!((error.code, error.details), (AppErrorCode::NotFound, None));
        assert_eq!(
            service.resolve(&token.id).expect("resolves").expose(),
            "internal-account-credential"
        );
    }

    #[test]
    fn a_full_service_run_writes_no_secret_value_into_the_log_file() {
        let directory = temporary_directory("studio-secret-log-");
        let logger = Arc::new(
            FileLogger::open(LoggerOptions::new(
                directory.path().to_path_buf(),
                LogLevel::Debug,
                Arc::new(FakeClock::default()),
            ))
            .expect("opens"),
        );
        let Harness { service, .. } = harness_with(logger.clone());
        let planted = input();
        let created = service.create(planted.clone()).expect("creates");
        service
            .update(
                &created.id,
                SecretUpdateInput {
                    name: Some("renamed".to_owned()),
                    value: Some(Plaintext::new(VALUE)),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("updates");
        let resolved = service.resolve(&created.id).expect("resolves");
        logger.log(
            LogLevel::Error,
            "secrets",
            &format!("a careless caller logged {VALUE}"),
            &[("token", json!(VALUE))],
        );
        logger.log(
            LogLevel::Debug,
            "secrets",
            &format!("debug-printed {resolved:?} and {planted:?}"),
            &[],
        );
        service.remove(&created.id).expect("removes");
        logger.close();

        let written: String = fs::read_dir(directory.path())
            .expect("lists")
            .map(|entry| fs::read_to_string(entry.expect("an entry").path()).expect("reads"))
            .collect();
        assert!(written.contains("Created a secret"));
        assert!(written.contains("Removed a secret"));
        assert!(!written.contains(VALUE));
        assert!(!written.contains("…9c31"));
        assert!(written.contains(REDACTED));
    }
}
