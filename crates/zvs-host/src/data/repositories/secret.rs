use std::collections::HashMap;

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::{Value, json};
use zvs_contract::{AppError, AppErrorCode, SecretScope, Timestamp};

use crate::data::DataError;
use crate::data::schema::{
    ScopeColumn, SecretCipherRow, SecretConsumerKind, SecretRow, SecretUsageRow,
};
use crate::platform::ids::new_id;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SecretFilter<'a> {
    pub scope: Option<SecretScope>,
    pub r#type: Option<&'a str>,
    pub query: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretDraft {
    pub id: Option<String>,
    pub r#type: String,
    pub name: String,
    pub scope: SecretScope,
    pub cipher: Option<Vec<u8>>,
    pub cipher_version: i64,
    pub hint: Option<String>,
    pub fields: String,
    pub tags: String,
    pub note: Option<String>,
    pub rotation_days: Option<i64>,
    pub rotates_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretPatch {
    pub r#type: Option<String>,
    pub name: Option<String>,
    pub scope: Option<SecretScope>,
    pub cipher: Option<Option<Vec<u8>>>,
    pub cipher_version: Option<i64>,
    pub hint: Option<Option<String>>,
    pub fields: Option<String>,
    pub tags: Option<String>,
    pub note: Option<Option<String>>,
    pub rotation_days: Option<Option<i64>>,
    pub rotates_at: Option<Option<Timestamp>>,
    pub updated_at: Option<Timestamp>,
}

impl SecretPatch {
    fn assignments(&self) -> Vec<(&'static str, SqlValue)> {
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
        push("type", self.r#type.as_deref().map(text));
        push("name", self.name.as_deref().map(text));
        push("scope", self.scope.map(|scope| text(scope.as_str())));
        push(
            "cipher",
            self.cipher
                .as_ref()
                .map(|cipher| cipher.clone().map_or(SqlValue::Null, SqlValue::Blob)),
        );
        push("cipher_version", self.cipher_version.map(SqlValue::Integer));
        push("hint", self.hint.as_ref().map(nullable_text));
        push("fields", self.fields.as_deref().map(text));
        push("tags", self.tags.as_deref().map(text));
        push("note", self.note.as_ref().map(nullable_text));
        push(
            "rotation_days",
            self.rotation_days.as_ref().map(nullable_integer),
        );
        push("rotates_at", self.rotates_at.as_ref().map(nullable_integer));
        push("updated_at", self.updated_at.map(SqlValue::Integer));
        assignments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretUsageDraft<'a> {
    pub secret_id: &'a str,
    pub consumer_kind: SecretConsumerKind,
    pub consumer_id: &'a str,
    pub created_at: Timestamp,
}

pub fn secret_in_use(id: &str, usage: &[SecretUsageRow]) -> AppError {
    AppError::new(
        AppErrorCode::Conflict,
        format!(
            "Секрет используется ({}) и не может быть удалён",
            usage.len()
        ),
    )
    .with_detail("secretId", id)
    .with_detail(
        "consumers",
        usage
            .iter()
            .map(|entry| json!({ "kind": entry.consumer_kind.as_str(), "id": entry.consumer_id }))
            .collect::<Vec<Value>>(),
    )
}

fn escape_like(value: &str) -> String {
    value.chars().fold(
        String::with_capacity(value.len()),
        |mut escaped, character| {
            if matches!(character, '!' | '%' | '_') {
                escaped.push('!');
            }
            escaped.push(character);
            escaped
        },
    )
}

pub struct SecretRepository<'c> {
    connection: &'c Connection,
}

impl<'c> SecretRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list_summaries(&self, filter: &SecretFilter<'_>) -> rusqlite::Result<Vec<SecretRow>> {
        let mut conditions = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if let Some(scope) = filter.scope {
            conditions.push("scope = ?");
            values.push(SqlValue::Text(scope.as_str().to_owned()));
        }
        if let Some(kind) = filter.r#type {
            conditions.push("type = ?");
            values.push(SqlValue::Text(kind.to_owned()));
        }
        if let Some(query) = filter
            .query
            .map(str::trim)
            .filter(|query| !query.is_empty())
        {
            conditions.push("name LIKE ? ESCAPE '!'");
            values.push(SqlValue::Text(format!("%{}%", escape_like(query))));
        }
        let clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM secret{clause} ORDER BY updated_at DESC, name ASC",
                SecretRow::COLUMNS
            ))?
            .query_map(params_from_iter(values), SecretRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<SecretRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM secret WHERE id = ?1",
                SecretRow::COLUMNS
            ))?
            .query_row([id], SecretRow::from_row)
            .optional()
    }

    pub fn find_cipher_by_id(&self, id: &str) -> rusqlite::Result<Option<SecretCipherRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM secret WHERE id = ?1",
                SecretCipherRow::COLUMNS
            ))?
            .query_row([id], SecretCipherRow::from_row)
            .optional()
    }

    pub fn list_rotating_before(&self, at: Timestamp) -> rusqlite::Result<Vec<SecretRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM secret WHERE rotates_at IS NOT NULL AND rotates_at <= ?1 ORDER BY rotates_at ASC",
                SecretRow::COLUMNS
            ))?
            .query_map([at], SecretRow::from_row)?
            .collect()
    }

    pub fn create(&self, draft: &SecretDraft) -> rusqlite::Result<SecretRow> {
        let id = draft
            .id
            .clone()
            .unwrap_or_else(|| new_id().hyphenated().to_string());
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO secret (id, type, name, scope, cipher, cipher_version, hint, fields, tags, note, rotation_days, rotates_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) RETURNING {}",
                SecretRow::COLUMNS
            ))?
            .query_row(
                params![
                    id,
                    draft.r#type,
                    draft.name,
                    ScopeColumn(draft.scope),
                    draft.cipher,
                    draft.cipher_version,
                    draft.hint,
                    draft.fields,
                    draft.tags,
                    draft.note,
                    draft.rotation_days,
                    draft.rotates_at,
                    draft.created_at,
                    draft.updated_at,
                ],
                SecretRow::from_row,
            )
    }

    pub fn update(&self, id: &str, patch: &SecretPatch) -> rusqlite::Result<Option<SecretRow>> {
        let assignments = patch.assignments();
        if assignments.is_empty() {
            return self.find_by_id(id);
        }
        let set = assignments
            .iter()
            .map(|(column, _)| format!("{column} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = assignments
            .into_iter()
            .map(|(_, value)| value)
            .chain([SqlValue::Text(id.to_owned())]);
        self.connection
            .prepare_cached(&format!(
                "UPDATE secret SET {set} WHERE id = ? RETURNING {}",
                SecretRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), SecretRow::from_row)
            .optional()
    }

    pub fn remove(&self, id: &str) -> Result<(), DataError> {
        let usage = self.list_usage(id)?;
        if !usage.is_empty() {
            return Err(secret_in_use(id, &usage).into());
        }
        self.connection
            .prepare_cached("DELETE FROM secret WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    pub fn count_usage_by_secret(&self) -> rusqlite::Result<HashMap<String, u64>> {
        self.connection
            .prepare_cached("SELECT secret_id, count(*) FROM secret_usage GROUP BY secret_id")?
            .query_map([], |row| {
                Ok((row.get(0)?, row.get::<_, i64>(1)?.unsigned_abs()))
            })?
            .collect()
    }

    pub fn count_usage(&self, id: &str) -> rusqlite::Result<u64> {
        self.connection
            .prepare_cached("SELECT count(*) FROM secret_usage WHERE secret_id = ?1")?
            .query_row([id], |row| row.get::<_, i64>(0))
            .map(i64::unsigned_abs)
    }

    pub fn list_usage(&self, id: &str) -> rusqlite::Result<Vec<SecretUsageRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM secret_usage WHERE secret_id = ?1 ORDER BY consumer_kind ASC, consumer_id ASC",
                SecretUsageRow::COLUMNS
            ))?
            .query_map([id], SecretUsageRow::from_row)?
            .collect()
    }

    pub fn add_usage(&self, entry: &SecretUsageDraft<'_>) -> rusqlite::Result<SecretUsageRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO secret_usage (secret_id, consumer_kind, consumer_id, created_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (secret_id, consumer_kind, consumer_id) DO UPDATE SET created_at = excluded.created_at \
                 RETURNING {}",
                SecretUsageRow::COLUMNS
            ))?
            .query_row(
                params![
                    entry.secret_id,
                    entry.consumer_kind,
                    entry.consumer_id,
                    entry.created_at
                ],
                SecretUsageRow::from_row,
            )
    }

    pub fn remove_usage(
        &self,
        secret_id: &str,
        consumer_kind: SecretConsumerKind,
        consumer_id: &str,
    ) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "DELETE FROM secret_usage WHERE secret_id = ?1 AND consumer_kind = ?2 AND consumer_id = ?3",
            )?
            .execute(params![secret_id, consumer_kind, consumer_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::{CLOCK_ORIGIN, temporary_database};

    const NOW: Timestamp = CLOCK_ORIGIN;

    fn draft() -> SecretDraft {
        SecretDraft {
            id: None,
            r#type: "ollama".to_owned(),
            name: "Ollama — personal".to_owned(),
            scope: SecretScope::Personal,
            cipher: Some(b"ciphertext".to_vec()),
            cipher_version: 1,
            hint: Some("…4f2a".to_owned()),
            fields: r#"{"baseUrl":"https://ollama.com/api"}"#.to_owned(),
            tags: r#"["llm","cloud"]"#.to_owned(),
            note: None,
            rotation_days: None,
            rotates_at: None,
            created_at: NOW,
            updated_at: NOW,
        }
    }

    fn named(r#type: &str, name: &str) -> SecretDraft {
        SecretDraft {
            r#type: r#type.to_owned(),
            name: name.to_owned(),
            ..draft()
        }
    }

    fn usage<'a>(
        secret_id: &'a str,
        consumer_kind: SecretConsumerKind,
        consumer_id: &'a str,
        created_at: Timestamp,
    ) -> SecretUsageDraft<'a> {
        SecretUsageDraft {
            secret_id,
            consumer_kind,
            consumer_id,
            created_at,
        }
    }

    fn all(secrets: &SecretRepository<'_>) -> rusqlite::Result<Vec<SecretRow>> {
        secrets.list_summaries(&SecretFilter::default())
    }

    #[test]
    fn a_secret_round_trips_through_create_read_update_and_delete() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                let created = secrets.create(&draft())?;
                assert_eq!(created.id.len(), 36);
                assert_eq!(created.name, "Ollama — personal");
                assert_eq!(created.cipher_version, 1);
                assert_eq!(created.hint.as_deref(), Some("…4f2a"));
                assert_eq!(
                    serde_json::from_str::<Vec<String>>(&created.tags).expect("tags"),
                    ["llm", "cloud"]
                );
                assert_eq!(secrets.find_by_id(&created.id)?, Some(created.clone()));

                let updated = secrets
                    .update(
                        &created.id,
                        &SecretPatch {
                            name: Some("Ollama — lab".to_owned()),
                            updated_at: Some(NOW + 10),
                            ..SecretPatch::default()
                        },
                    )?
                    .expect("updates");
                assert_eq!(updated.name, "Ollama — lab");
                assert_eq!(updated.updated_at, NOW + 10);
                assert_eq!(all(&secrets)?.len(), 1);
                assert_eq!(
                    secrets.update(&created.id, &SecretPatch::default())?,
                    Some(updated)
                );
                assert_eq!(
                    secrets.update(
                        "missing",
                        &SecretPatch {
                            name: Some("x".to_owned()),
                            ..SecretPatch::default()
                        }
                    )?,
                    None
                );

                secrets.remove(&created.id)?;
                assert_eq!(secrets.find_by_id(&created.id)?, None);
                assert!(all(&secrets)?.is_empty());
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn the_database_refuses_two_secrets_with_the_same_name_and_type() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                secrets.create(&draft())?;
                let error = secrets.create(&draft()).expect_err("refuses");
                assert!(
                    error.to_string().contains("UNIQUE constraint failed"),
                    "{error}"
                );
                let other = secrets.create(&SecretDraft {
                    r#type: "custom".to_owned(),
                    ..draft()
                })?;
                assert_eq!(other.r#type, "custom");
                assert_eq!(all(&secrets)?.len(), 2);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn only_find_cipher_by_id_returns_ciphertext() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                let created = secrets.create(&draft())?;
                assert!(!SecretRow::COLUMNS.contains("cipher,"));
                assert!(!SecretRow::COLUMNS.ends_with("cipher"));
                let cipher = secrets.find_cipher_by_id(&created.id)?.expect("a row");
                assert_eq!(cipher.cipher_version, 1);
                assert_eq!(cipher.cipher.as_deref(), Some(&b"ciphertext"[..]));
                assert!(!format!("{cipher:?}").contains("ciphertext"));
                assert_eq!(secrets.find_cipher_by_id("missing")?, None);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn deleting_a_secret_something_still_uses_is_refused_and_names_the_consumers() {
        let database = temporary_database();
        let created = database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                let created = secrets.create(&draft())?;
                secrets.add_usage(&usage(
                    &created.id,
                    SecretConsumerKind::Provider,
                    "provider-1",
                    NOW,
                ))?;
                Ok(created)
            })
            .expect("creates");

        let error = database
            .with_connection(|connection| Repositories::new(connection).secrets.remove(&created.id))
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Conflict);
        assert_eq!(
            error.details.map(Value::Object),
            Some(json!({
                "secretId": created.id,
                "consumers": [{ "kind": "provider", "id": "provider-1" }],
            }))
        );

        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                assert!(secrets.find_by_id(&created.id)?.is_some());
                let error = connection
                    .execute("DELETE FROM secret WHERE id = ?1", [&created.id])
                    .expect_err("the foreign key restricts");
                assert!(
                    error.to_string().contains("FOREIGN KEY constraint failed"),
                    "{error}"
                );
                secrets.remove_usage(&created.id, SecretConsumerKind::Provider, "provider-1")?;
                secrets.remove(&created.id)?;
                assert_eq!(secrets.find_by_id(&created.id)?, None);
                Ok(())
            })
            .expect("removes");
    }

    #[test]
    fn usage_is_counted_per_secret_across_every_consumer_kind() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                let first = secrets.create(&draft())?;
                let second = secrets.create(&named("openrouter", "OpenRouter — main"))?;
                for (kind, consumer) in [
                    (SecretConsumerKind::Provider, "provider-1"),
                    (SecretConsumerKind::Provider, "provider-2"),
                    (SecretConsumerKind::VectorStore, "store-1"),
                    (SecretConsumerKind::McpServer, "mcp-1"),
                    (SecretConsumerKind::Integration, "integration-1"),
                ] {
                    secrets.add_usage(&usage(&first.id, kind, consumer, NOW))?;
                }
                secrets.add_usage(&usage(
                    &first.id,
                    SecretConsumerKind::Provider,
                    "provider-1",
                    NOW + 5,
                ))?;

                assert_eq!(secrets.count_usage(&first.id)?, 5);
                assert_eq!(secrets.count_usage(&second.id)?, 0);
                let listed = secrets.list_usage(&first.id)?;
                assert_eq!(
                    listed
                        .iter()
                        .map(|entry| format!(
                            "{}:{}",
                            entry.consumer_kind.as_str(),
                            entry.consumer_id
                        ))
                        .collect::<Vec<_>>(),
                    [
                        "integration:integration-1",
                        "mcp_server:mcp-1",
                        "provider:provider-1",
                        "provider:provider-2",
                        "vector_store:store-1",
                    ]
                );
                assert_eq!(
                    listed
                        .iter()
                        .find(|entry| entry.consumer_id == "provider-1")
                        .map(|entry| entry.created_at),
                    Some(NOW + 5)
                );
                assert_eq!(
                    secrets.count_usage_by_secret()?,
                    HashMap::from([(first.id.clone(), 5)])
                );

                secrets.remove_usage(&first.id, SecretConsumerKind::Provider, "provider-2")?;
                assert_eq!(secrets.count_usage(&first.id)?, 4);

                secrets.remove(&second.id)?;
                assert_eq!(all(&secrets)?.len(), 1);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn summaries_are_filtered_by_scope_type_and_name_newest_first() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                secrets.create(&SecretDraft {
                    updated_at: NOW + 1,
                    ..draft()
                })?;
                secrets.create(&SecretDraft {
                    scope: SecretScope::Shared,
                    updated_at: NOW + 3,
                    ..named("openrouter", "OpenRouter — main")
                })?;
                secrets.create(&SecretDraft {
                    updated_at: NOW + 2,
                    ..named("mistral", "Mistral — embeddings")
                })?;
                let names = |filter: SecretFilter<'_>, pick: fn(SecretRow) -> String| {
                    secrets
                        .list_summaries(&filter)
                        .map(|rows| rows.into_iter().map(pick).collect::<Vec<_>>())
                };
                assert_eq!(
                    names(SecretFilter::default(), |row| row.r#type)?,
                    ["openrouter", "mistral", "ollama"]
                );
                assert_eq!(
                    names(
                        SecretFilter {
                            scope: Some(SecretScope::Shared),
                            ..SecretFilter::default()
                        },
                        |row| row.name
                    )?,
                    ["OpenRouter — main"]
                );
                assert_eq!(
                    names(
                        SecretFilter {
                            r#type: Some("mistral"),
                            ..SecretFilter::default()
                        },
                        |row| row.name
                    )?,
                    ["Mistral — embeddings"]
                );
                assert_eq!(
                    names(
                        SecretFilter {
                            query: Some("ollama"),
                            ..SecretFilter::default()
                        },
                        |row| row.r#type
                    )?,
                    ["ollama"]
                );
                for query in ["%", "_", "!"] {
                    assert!(
                        names(
                            SecretFilter {
                                query: Some(query),
                                ..SecretFilter::default()
                            },
                            |row| row.name
                        )?
                        .is_empty(),
                        "{query}"
                    );
                }
                assert_eq!(
                    names(
                        SecretFilter {
                            query: Some("   "),
                            ..SecretFilter::default()
                        },
                        |row| row.name
                    )?
                    .len(),
                    3
                );
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn rotating_secrets_are_listed_by_due_date_and_undated_ones_are_left_out() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                secrets.create(&named("ollama", "never rotates"))?;
                secrets.create(&SecretDraft {
                    rotation_days: Some(90),
                    rotates_at: Some(NOW + 2000),
                    ..named("mistral", "due later")
                })?;
                secrets.create(&SecretDraft {
                    rotation_days: Some(30),
                    rotates_at: Some(NOW + 1000),
                    ..named("custom", "due sooner")
                })?;
                let names = |at| {
                    secrets
                        .list_rotating_before(at)
                        .map(|rows| rows.into_iter().map(|row| row.name).collect::<Vec<_>>())
                };
                assert_eq!(names(NOW + 3000)?, ["due sooner", "due later"]);
                assert_eq!(names(NOW + 1000)?, ["due sooner"]);
                assert!(names(NOW)?.is_empty());
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn a_rolled_back_unit_of_work_leaves_no_secret_and_no_usage_behind() {
        let database = temporary_database();
        let failed = database.transaction(|transaction| {
            let secrets = Repositories::new(transaction).secrets;
            let created = secrets.create(&draft())?;
            secrets.add_usage(&usage(
                &created.id,
                SecretConsumerKind::Provider,
                "provider-1",
                NOW,
            ))?;
            Err::<(), DataError>(
                AppError::new(AppErrorCode::Unknown, "the unit of work failed halfway").into(),
            )
        });
        assert!(failed.is_err());
        let left = database
            .with_connection(|connection| {
                let secrets = Repositories::new(connection).secrets;
                Ok((all(&secrets)?, secrets.count_usage_by_secret()?))
            })
            .expect("reads");
        assert!(left.0.is_empty());
        assert!(left.1.is_empty());
    }

    #[test]
    fn like_wildcards_are_escaped() {
        assert_eq!(escape_like("50%_off!"), "50!%!_off!!");
    }
}
