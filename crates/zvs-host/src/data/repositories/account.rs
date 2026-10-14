use rusqlite::{Connection, OptionalExtension};

use crate::data::schema::AccountRow;

pub struct AccountRepository<'c> {
    connection: &'c Connection,
}

impl<'c> AccountRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
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
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::testing::temporary_database;

    #[test]
    fn an_account_reads_back_with_its_expiry_in_seconds_and_a_missing_one_is_none() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                connection.execute(
                    "INSERT INTO account (id, adapter, partition, external_id, email_masked, token_expires_at, status, linked_at, updated_at) VALUES (?1, 'qwen-web', 'persist:browser-work', 'private-id', 'a***@example.com', ?2, 'needs-relink', 1, 2)",
                    params!["acc-1", 1_800_172_800_i64],
                )?;
                let accounts = AccountRepository::new(connection);
                let row = accounts.find_by_id("acc-1")?.expect("stored");
                assert_eq!(row.email_masked.as_deref(), Some("a***@example.com"));
                assert_eq!(row.token_expires_at, Some(1_800_172_800.0));
                assert!(!row.is_linked());
                assert_eq!(row.linked_at, 1);
                assert_eq!(accounts.find_by_id("missing")?, None);
                Ok(())
            })
            .expect("reads");
    }
}
