use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::Timestamp;

use crate::data::schema::SettingRow;

pub struct SettingRepository<'c> {
    connection: &'c Connection,
}

impl<'c> SettingRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn get(&self, key: &str) -> rusqlite::Result<Option<SettingRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM setting WHERE key = ?1",
                SettingRow::COLUMNS
            ))?
            .query_row([key], SettingRow::from_row)
            .optional()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<SettingRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM setting ORDER BY key",
                SettingRow::COLUMNS
            ))?
            .query_map([], SettingRow::from_row)?
            .collect()
    }

    pub fn set(
        &self,
        key: &str,
        value: &str,
        updated_at: Timestamp,
    ) -> rusqlite::Result<SettingRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO setting (key, value, updated_at) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at \
                 RETURNING {}",
                SettingRow::COLUMNS
            ))?
            .query_row(params![key, value, updated_at], SettingRow::from_row)
    }

    pub fn remove(&self, key: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM setting WHERE key = ?1")?
            .execute([key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    #[test]
    fn setting_repository_round_trips_values_and_lists_them_in_key_order() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let settings = Repositories::new(connection).settings;
                assert_eq!(settings.get("window.main")?, None);

                let stored = settings.set("window.main", r#"{"width":1440}"#, 1_700_000_000_000)?;
                assert_eq!(
                    stored,
                    SettingRow {
                        key: "window.main".to_owned(),
                        value: r#"{"width":1440}"#.to_owned(),
                        updated_at: 1_700_000_000_000,
                    }
                );
                assert_eq!(settings.get("window.main")?, Some(stored));

                let updated = settings.set("window.main", r#"{"width":800}"#, 1_700_000_001_000)?;
                assert_eq!(updated.value, r#"{"width":800}"#);
                assert_eq!(updated.updated_at, 1_700_000_001_000);
                assert_eq!(settings.list()?.len(), 1);

                settings.set("a.key", "\"first\"", 1)?;
                assert_eq!(
                    settings
                        .list()?
                        .into_iter()
                        .map(|row| row.key)
                        .collect::<Vec<_>>(),
                    ["a.key", "window.main"]
                );

                settings.remove("a.key")?;
                assert_eq!(settings.get("a.key")?, None);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn repositories_in_one_transaction_share_it() {
        let database = temporary_database();
        database
            .transaction(|transaction| {
                let repositories = Repositories::new(transaction);
                repositories.settings.set("a.key", "1", 1)?;
                assert_eq!(
                    Repositories::new(transaction)
                        .settings
                        .get("a.key")?
                        .map(|row| row.value),
                    Some("1".to_owned())
                );
                Ok(())
            })
            .expect("commits");
    }
}
