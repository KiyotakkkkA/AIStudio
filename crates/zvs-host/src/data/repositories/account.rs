use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use zvs_contract::{AccountFamily, AccountStatus, ProviderStatus, Timestamp};

use crate::data::repositories::{SecretRepository, SecretUsageDraft};
use crate::data::schema::{AccountRow, SecretConsumerKind};
use crate::platform::ids::new_id;

pub const NO_ACCOUNT_LINKED: &str = "no account linked";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDraft {
    pub id: Option<String>,
    pub adapter: AccountFamily,
    pub partition: String,
    pub external_id: String,
    pub email_masked: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub token_secret_id: Option<String>,
    pub token_expires_at: Option<i64>,
    pub status: AccountStatus,
    pub status_detail: Option<String>,
    pub last_checked_at: Option<Timestamp>,
    pub linked_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AccountDraft {
    pub fn new(
        adapter: AccountFamily,
        partition: impl Into<String>,
        external_id: impl Into<String>,
        linked_at: Timestamp,
    ) -> Self {
        Self {
            id: None,
            adapter,
            partition: partition.into(),
            external_id: external_id.into(),
            email_masked: None,
            display_name: None,
            avatar_url: None,
            token_secret_id: None,
            token_expires_at: None,
            status: AccountStatus::Linked,
            status_detail: None,
            last_checked_at: None,
            linked_at,
            updated_at: linked_at,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountPatch {
    pub partition: Option<String>,
    pub external_id: Option<String>,
    pub email_masked: Option<Option<String>>,
    pub display_name: Option<Option<String>>,
    pub avatar_url: Option<Option<String>>,
    pub token_secret_id: Option<Option<String>>,
    pub token_expires_at: Option<Option<i64>>,
    pub status: Option<AccountStatus>,
    pub status_detail: Option<Option<String>>,
    pub last_checked_at: Option<Option<Timestamp>>,
    pub updated_at: Option<Timestamp>,
}

impl AccountPatch {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn status(status: AccountStatus, detail: Option<String>, now: Timestamp) -> Self {
        Self {
            status: Some(status),
            status_detail: Some(detail),
            last_checked_at: Some(Some(now)),
            updated_at: Some(now),
            ..Self::default()
        }
    }

    fn assignments(&self, now: Timestamp) -> Vec<(&'static str, SqlValue)> {
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
        push("partition", self.partition.as_deref().map(text));
        push("external_id", self.external_id.as_deref().map(text));
        push(
            "email_masked",
            self.email_masked.as_ref().map(nullable_text),
        );
        push(
            "display_name",
            self.display_name.as_ref().map(nullable_text),
        );
        push("avatar_url", self.avatar_url.as_ref().map(nullable_text));
        push(
            "token_secret_id",
            self.token_secret_id.as_ref().map(nullable_text),
        );
        push(
            "token_expires_at",
            self.token_expires_at.as_ref().map(nullable_integer),
        );
        push("status", self.status.map(|status| text(status.as_str())));
        push(
            "status_detail",
            self.status_detail.as_ref().map(nullable_text),
        );
        push(
            "last_checked_at",
            self.last_checked_at.as_ref().map(nullable_integer),
        );
        push(
            "updated_at",
            Some(SqlValue::Integer(self.updated_at.unwrap_or(now))),
        );
        assignments
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountRemoval {
    pub removed: bool,
    pub freed_secret_id: Option<String>,
    pub detached_provider_ids: Vec<String>,
}

pub struct AccountRepository<'c> {
    connection: &'c Connection,
}

impl<'c> AccountRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, draft: &AccountDraft) -> rusqlite::Result<AccountRow> {
        let id = draft
            .id
            .clone()
            .unwrap_or_else(|| new_id().hyphenated().to_string());
        let row = self
            .connection
            .prepare_cached(&format!(
                "INSERT INTO account (id, adapter, partition, external_id, email_masked, display_name, avatar_url, token_secret_id, token_expires_at, status, status_detail, last_checked_at, linked_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) RETURNING {}",
                AccountRow::COLUMNS
            ))?
            .query_row(
                params![
                    id,
                    draft.adapter.as_str(),
                    draft.partition,
                    draft.external_id,
                    draft.email_masked,
                    draft.display_name,
                    draft.avatar_url,
                    draft.token_secret_id,
                    draft.token_expires_at,
                    draft.status.as_str(),
                    draft.status_detail,
                    draft.last_checked_at,
                    draft.linked_at,
                    draft.updated_at,
                ],
                AccountRow::from_row,
            )?;
        if let Some(secret_id) = &row.token_secret_id {
            SecretRepository::new(self.connection).add_usage(&SecretUsageDraft {
                secret_id,
                consumer_kind: SecretConsumerKind::Account,
                consumer_id: &row.id,
                created_at: row.linked_at,
            })?;
        }
        Ok(row)
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<AccountRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM account WHERE id = ?1",
                AccountRow::COLUMNS
            ))?
            .query_row([id], AccountRow::from_row)
            .optional()
    }

    pub fn find_by_adapter_and_external_id(
        &self,
        adapter: AccountFamily,
        external_id: &str,
    ) -> rusqlite::Result<Option<AccountRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM account WHERE adapter = ?1 AND external_id = ?2",
                AccountRow::COLUMNS
            ))?
            .query_row(params![adapter.as_str(), external_id], AccountRow::from_row)
            .optional()
    }

    pub fn list_by_adapter(&self, adapter: AccountFamily) -> rusqlite::Result<Vec<AccountRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM account WHERE adapter = ?1 ORDER BY linked_at ASC, id ASC",
                AccountRow::COLUMNS
            ))?
            .query_map([adapter.as_str()], AccountRow::from_row)?
            .collect()
    }

    pub fn update(
        &self,
        id: &str,
        patch: &AccountPatch,
        now: Timestamp,
    ) -> rusqlite::Result<Option<AccountRow>> {
        let Some(previous) = self.find_by_id(id)? else {
            return Ok(None);
        };
        if patch.is_empty() {
            return Ok(Some(previous));
        }
        let assignments = patch.assignments(now);
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
                "UPDATE account SET {set} WHERE id = ? RETURNING {}",
                AccountRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), AccountRow::from_row)
            .optional()?
        else {
            return Ok(None);
        };
        if previous.token_secret_id != row.token_secret_id {
            let secrets = SecretRepository::new(self.connection);
            if let Some(old) = &previous.token_secret_id {
                secrets.remove_usage(old, SecretConsumerKind::Account, id)?;
            }
            if let Some(new) = &row.token_secret_id {
                secrets.add_usage(&SecretUsageDraft {
                    secret_id: new,
                    consumer_kind: SecretConsumerKind::Account,
                    consumer_id: id,
                    created_at: row.updated_at,
                })?;
            }
        }
        Ok(Some(row))
    }

    pub fn update_status(
        &self,
        id: &str,
        status: AccountStatus,
        detail: Option<String>,
        now: Timestamp,
    ) -> rusqlite::Result<Option<AccountRow>> {
        self.update(id, &AccountPatch::status(status, detail, now), now)
    }

    pub fn remove(&self, id: &str, now: Timestamp) -> rusqlite::Result<AccountRemoval> {
        let Some(row) = self.find_by_id(id)? else {
            return Ok(AccountRemoval::default());
        };
        let detached = self
            .connection
            .prepare_cached(
                "UPDATE provider SET account_id = NULL, status = ?1, status_detail = ?2, updated_at = ?3 WHERE account_id = ?4 RETURNING id",
            )?
            .query_map(
                params![ProviderStatus::Failed.as_str(), NO_ACCOUNT_LINKED, now, id],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if let Some(secret_id) = &row.token_secret_id {
            SecretRepository::new(self.connection).remove_usage(
                secret_id,
                SecretConsumerKind::Account,
                id,
            )?;
        }
        self.connection
            .prepare_cached("DELETE FROM account WHERE id = ?1")?
            .execute([id])?;
        Ok(AccountRemoval {
            removed: true,
            freed_secret_id: row.token_secret_id,
            detached_provider_ids: detached,
        })
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::{AdapterFamily, AuthMode, ProviderCapability, ProviderKind, SecretScope};

    use super::*;
    use crate::data::repositories::{ProviderDraft, ProviderPatch, Repositories, SecretDraft};
    use crate::data::schema::ProviderSettings;
    use crate::data::{DataError, Database};
    use crate::testing::temporary_database;

    fn draft() -> AccountDraft {
        AccountDraft::new(
            AccountFamily::QwenWeb,
            "persist:browser-work",
            "user-1",
            100,
        )
    }

    fn provider(account_id: Option<&str>) -> ProviderDraft {
        ProviderDraft {
            adapter: AdapterFamily::QwenWeb,
            auth_mode: AuthMode::Account,
            account_id: account_id.map(str::to_owned),
            ..ProviderDraft::new(
                ProviderKind::OpenaiCompatible,
                "Qwen",
                "https://chat.qwen.ai/api/v2",
                vec![ProviderCapability::Text],
                100,
            )
        }
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

    fn run<T>(
        database: &Database,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> T {
        database
            .transaction(|transaction| work(&Repositories::new(transaction)))
            .expect("runs")
    }

    #[test]
    fn the_account_table_has_the_link_columns_and_one_row_per_vendor_identity() {
        let database = temporary_database();
        let mut columns = database
            .with_connection(|connection| {
                Ok(connection
                    .prepare("PRAGMA table_info(account)")?
                    .query_map([], |row| row.get::<_, String>("name"))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .expect("columns");
        columns.sort();
        run(&database, |repositories| {
            assert_eq!(
                columns,
                [
                    "adapter",
                    "avatar_url",
                    "display_name",
                    "email_masked",
                    "external_id",
                    "id",
                    "last_checked_at",
                    "linked_at",
                    "partition",
                    "status",
                    "status_detail",
                    "token_expires_at",
                    "token_secret_id",
                    "updated_at",
                ]
            );
            let linked = repositories.accounts.create(&draft())?;
            assert_eq!(linked.status, "linked");
            assert!(repositories.accounts.create(&draft()).is_err());
            assert_eq!(
                repositories
                    .accounts
                    .list_by_adapter(AccountFamily::QwenWeb)?,
                std::slice::from_ref(&linked)
            );
            assert_eq!(
                repositories
                    .accounts
                    .find_by_adapter_and_external_id(AccountFamily::QwenWeb, "user-2")?,
                None
            );
            assert_eq!(
                repositories
                    .accounts
                    .find_by_adapter_and_external_id(AccountFamily::QwenWeb, "user-1")?,
                Some(linked)
            );
            Ok(())
        });
    }

    #[test]
    fn the_auth_mode_guard_keeps_api_and_account_providers_apart() {
        let database = temporary_database();
        run(&database, |repositories| {
            let account = repositories.accounts.create(&draft())?;
            let key = secret(repositories, "Key")?;
            assert!(repositories.providers.create(&provider(None)).is_err());
            assert!(
                repositories
                    .providers
                    .create(&ProviderDraft {
                        secret_id: Some(key.clone()),
                        ..provider(Some(&account.id))
                    })
                    .is_err()
            );
            assert!(
                repositories
                    .providers
                    .create(&ProviderDraft {
                        name: "Local".to_owned(),
                        auth_mode: AuthMode::Api,
                        adapter: AdapterFamily::OpenaiCompatible,
                        ..provider(Some(&account.id))
                    })
                    .is_err()
            );
            let linked = repositories
                .providers
                .create(&provider(Some(&account.id)))?;
            assert_eq!(linked.account_id.as_deref(), Some(account.id.as_str()));
            assert_eq!(linked.secret_id, None);
            assert!(
                repositories
                    .providers
                    .update(
                        &linked.id,
                        &ProviderPatch {
                            secret_id: Some(Some(key)),
                            ..ProviderPatch::default()
                        },
                        200,
                    )
                    .is_err()
            );
            assert_eq!(
                repositories
                    .providers
                    .find_by_id(&linked.id)?
                    .expect("kept")
                    .secret_id,
                None
            );
            Ok(())
        });
    }

    #[test]
    fn removing_an_account_detaches_providers_instead_of_deleting_them() {
        let database = temporary_database();
        run(&database, |repositories| {
            let account = repositories.accounts.create(&draft())?;
            let mut settings = ProviderSettings::default();
            settings
                .0
                .insert("timeoutSeconds".to_owned(), serde_json::json!(45));
            let linked = repositories.providers.create(&ProviderDraft {
                settings,
                ..provider(Some(&account.id))
            })?;
            let removal = repositories.accounts.remove(&account.id, 500)?;
            assert_eq!(
                removal.detached_provider_ids,
                std::slice::from_ref(&linked.id)
            );
            assert_eq!(removal.freed_secret_id, None);
            assert!(removal.removed);
            assert_eq!(repositories.accounts.find_by_id(&account.id)?, None);
            let survivor = repositories
                .providers
                .find_by_id(&linked.id)?
                .expect("kept");
            assert_eq!(survivor.account_id, None);
            assert_eq!(survivor.status, ProviderStatus::Failed);
            assert_eq!(survivor.status_detail.as_deref(), Some(NO_ACCOUNT_LINKED));
            assert_eq!(survivor.settings.timeout_seconds(), Some(45.0));
            assert_eq!(survivor.auth_mode, "account");
            assert_eq!(survivor.updated_at, 500);
            assert_eq!(
                repositories.accounts.remove(&account.id, 600)?,
                AccountRemoval::default()
            );
            Ok(())
        });
    }

    #[test]
    fn the_token_secret_usage_follows_the_account() {
        let database = temporary_database();
        run(&database, |repositories| {
            let first = secret(repositories, "First")?;
            let second = secret(repositories, "Second")?;
            let account = repositories.accounts.create(&AccountDraft {
                token_secret_id: Some(first.clone()),
                ..draft()
            })?;
            let usage = |id: &str| -> rusqlite::Result<Vec<String>> {
                Ok(repositories
                    .secrets
                    .list_usage(id)?
                    .into_iter()
                    .map(|entry| format!("{}:{}", entry.consumer_kind.as_str(), entry.consumer_id))
                    .collect())
            };
            assert_eq!(usage(&first)?, [format!("account:{}", account.id)]);
            let moved = repositories
                .accounts
                .update(
                    &account.id,
                    &AccountPatch {
                        token_secret_id: Some(Some(second.clone())),
                        token_expires_at: Some(Some(4_102_444_800)),
                        ..AccountPatch::default()
                    },
                    300,
                )?
                .expect("updated");
            assert_eq!(moved.updated_at, 300);
            assert_eq!(moved.token_expires_at, Some(4_102_444_800));
            assert!(usage(&first)?.is_empty());
            assert_eq!(usage(&second)?, [format!("account:{}", account.id)]);
            let unchanged = repositories
                .accounts
                .update(&account.id, &AccountPatch::default(), 900)?
                .expect("kept");
            assert_eq!(unchanged.updated_at, 300);
            let relink = repositories
                .accounts
                .update_status(
                    &account.id,
                    AccountStatus::NeedsRelink,
                    Some("Куки очищены".to_owned()),
                    400,
                )?
                .expect("updated");
            assert_eq!(relink.status, "needs-relink");
            assert_eq!(relink.status_detail.as_deref(), Some("Куки очищены"));
            assert_eq!(relink.last_checked_at, Some(400));
            let removal = repositories.accounts.remove(&account.id, 500)?;
            assert_eq!(removal.freed_secret_id, Some(second.clone()));
            assert!(usage(&second)?.is_empty());
            assert_eq!(
                repositories
                    .accounts
                    .update(&account.id, &AccountPatch::default(), 1)?,
                None
            );
            Ok(())
        });
    }
}
