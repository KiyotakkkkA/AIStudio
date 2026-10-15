use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::{Timestamp, VectorSourceKind};

use crate::data::schema::VectorSourceRow;
use crate::data::schema::columns::json_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorSourceDraft {
    pub id: String,
    pub store_id: String,
    pub kind: VectorSourceKind,
    pub path: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub recursive: bool,
    pub created_at: Timestamp,
}

pub struct VectorSourceRepository<'c> {
    connection: &'c Connection,
}

impl<'c> VectorSourceRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_store(&self, store_id: &str) -> rusqlite::Result<Vec<VectorSourceRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_source WHERE store_id = ?1 ORDER BY path",
                VectorSourceRow::COLUMNS
            ))?
            .query_map([store_id], VectorSourceRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<VectorSourceRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_source WHERE id = ?1",
                VectorSourceRow::COLUMNS
            ))?
            .query_row([id], VectorSourceRow::from_row)
            .optional()
    }

    pub fn add(&self, draft: &VectorSourceDraft) -> rusqlite::Result<VectorSourceRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO vector_source (id, store_id, kind, path, include, exclude, recursive, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT (store_id, path) DO UPDATE SET kind = excluded.kind, include = excluded.include, exclude = excluded.exclude, recursive = excluded.recursive \
                 RETURNING {}",
                VectorSourceRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    draft.store_id,
                    draft.kind.as_str(),
                    draft.path,
                    json_text(&draft.include)?,
                    json_text(&draft.exclude)?,
                    draft.recursive,
                    draft.created_at,
                ],
                VectorSourceRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM vector_source WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::{Repositories, VectorStoreDraft};
    use crate::testing::{seed_embedding_provider, temporary_database};

    fn draft(id: &str, path: &str) -> VectorSourceDraft {
        VectorSourceDraft {
            id: id.to_owned(),
            store_id: "s1".to_owned(),
            kind: VectorSourceKind::Folder,
            path: path.to_owned(),
            include: vec!["*.md".to_owned()],
            exclude: Vec::new(),
            recursive: true,
            created_at: 5,
        }
    }

    #[test]
    fn adding_the_same_path_again_updates_the_source_in_place() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = seed_embedding_provider(connection);
                let repositories = Repositories::new(connection);
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new("s1", "K", &provider, "m", 3, 1))?;
                let sources = repositories.vector_sources;
                let first = sources.add(&draft("src-1", "C:/docs"))?;
                assert_eq!(first.include, ["*.md"]);
                let again = sources.add(&VectorSourceDraft {
                    kind: VectorSourceKind::File,
                    include: Vec::new(),
                    exclude: vec!["x".to_owned()],
                    recursive: false,
                    ..draft("src-2", "C:/docs")
                })?;
                assert_eq!(again.id, "src-1");
                assert_eq!(again.kind, VectorSourceKind::File);
                assert_eq!(again.exclude, ["x"]);
                assert!(!again.recursive);
                sources.add(&draft("src-3", "B:/other"))?;
                assert_eq!(
                    sources
                        .list_by_store("s1")?
                        .into_iter()
                        .map(|row| row.path)
                        .collect::<Vec<_>>(),
                    ["B:/other", "C:/docs"]
                );
                sources.remove("src-1")?;
                assert_eq!(sources.find_by_id("src-1")?, None);
                Ok(())
            })
            .expect("the repository works");
    }
}
