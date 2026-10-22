use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::DocumentRow;
use crate::data::schema::columns::EnumColumn;

pub struct DocumentRepository<'c> {
    connection: &'c Connection,
}

impl<'c> DocumentRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<DocumentRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM document ORDER BY opened_at DESC, id",
                DocumentRow::COLUMNS
            ))?
            .query_map([], DocumentRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<DocumentRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM document WHERE id = ?1",
                DocumentRow::COLUMNS
            ))?
            .query_row([id], DocumentRow::from_row)
            .optional()
    }

    pub fn find_by_hash(&self, content_hash: &str) -> rusqlite::Result<Option<DocumentRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM document WHERE content_hash = ?1",
                DocumentRow::COLUMNS
            ))?
            .query_row([content_hash], DocumentRow::from_row)
            .optional()
    }

    pub fn record_opened(&self, row: &DocumentRow) -> rusqlite::Result<DocumentRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO document ({columns}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT (content_hash) DO UPDATE SET source_path = excluded.source_path, \
                 kind = excluded.kind, mime_type = excluded.mime_type, \
                 page_count = excluded.page_count, part_count = excluded.part_count, \
                 size_bytes = excluded.size_bytes, cached_file = excluded.cached_file, \
                 opened_at = excluded.opened_at \
                 RETURNING {columns}",
                columns = DocumentRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.content_hash,
                    row.source_path,
                    EnumColumn(row.kind),
                    row.mime_type,
                    row.page_count,
                    row.part_count,
                    row.size_bytes,
                    row.cached_file,
                    row.opened_at,
                ],
                DocumentRow::from_row,
            )
    }

    pub fn remove_by_hash(&self, content_hash: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM document WHERE content_hash = ?1")?
            .execute([content_hash])?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::DocumentKind;

    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    fn row(id: &str, hash: &str, path: &str, opened_at: i64) -> DocumentRow {
        DocumentRow {
            id: id.to_owned(),
            content_hash: hash.to_owned(),
            source_path: path.to_owned(),
            kind: DocumentKind::Pdf,
            mime_type: "application/pdf".to_owned(),
            page_count: Some(3),
            part_count: 1,
            size_bytes: 120,
            cached_file: format!("{hash}.pdf"),
            opened_at,
        }
    }

    #[test]
    fn the_same_content_at_a_second_path_is_one_document() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let documents = Repositories::new(connection).documents;
                let first = documents.record_opened(&row("d1", "aa", "C:/a.pdf", 1))?;
                let second = documents.record_opened(&row("d2", "aa", "D:/b.pdf", 2))?;
                assert_eq!(second.id, first.id);
                assert_eq!(second.source_path, "D:/b.pdf");
                assert_eq!(second.opened_at, 2);
                assert_eq!(documents.list()?.len(), 1);
                Ok(())
            })
            .expect("records");
    }

    #[test]
    fn lists_the_most_recent_first_and_removes_by_hash() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let documents = Repositories::new(connection).documents;
                documents.record_opened(&row("d1", "aa", "C:/a.pdf", 1))?;
                documents.record_opened(&row("d2", "bb", "C:/b.pdf", 5))?;
                let ids: Vec<String> = documents.list()?.into_iter().map(|row| row.id).collect();
                assert_eq!(ids, ["d2", "d1"]);
                assert!(documents.remove_by_hash("aa")?);
                assert!(!documents.remove_by_hash("aa")?);
                assert!(documents.find_by_id("d1")?.is_none());
                assert_eq!(
                    documents.find_by_hash("bb")?.map(|row| row.id),
                    Some("d2".to_owned())
                );
                Ok(())
            })
            .expect("records");
    }
}
