use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::data::schema::GraphRelationRow;

pub struct GraphRelationRepository<'c> {
    connection: &'c Connection,
}

impl<'c> GraphRelationRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<GraphRelationRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_relation_type ORDER BY name",
                GraphRelationRow::COLUMNS
            ))?
            .query_map([], GraphRelationRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<GraphRelationRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_relation_type WHERE id = ?1",
                GraphRelationRow::COLUMNS
            ))?
            .query_row([id], GraphRelationRow::from_row)
            .optional()
    }

    pub fn find_by_name(&self, name: &str) -> rusqlite::Result<Option<GraphRelationRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_relation_type WHERE name = ?1",
                GraphRelationRow::COLUMNS
            ))?
            .query_row([name], GraphRelationRow::from_row)
            .optional()
    }

    pub fn list_by_ids(&self, ids: &[String]) -> rusqlite::Result<Vec<GraphRelationRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        self.connection
            .prepare(&format!(
                "SELECT {} FROM graph_relation_type WHERE id IN ({placeholders})",
                GraphRelationRow::COLUMNS
            ))?
            .query_map(
                params_from_iter(ids.iter().map(|id| SqlValue::Text(id.clone()))),
                GraphRelationRow::from_row,
            )?
            .collect()
    }

    pub fn define(&self, row: &GraphRelationRow) -> rusqlite::Result<GraphRelationRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO graph_relation_type (id, name, description, directed) VALUES (?1, ?2, ?3, ?4) RETURNING {}",
                GraphRelationRow::COLUMNS
            ))?
            .query_row(
                params![row.id, row.name, row.description, row.directed],
                GraphRelationRow::from_row,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    #[test]
    fn the_migrated_catalogue_carries_the_four_starter_relations() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let relations = Repositories::new(connection).graph_relations;
                let rows = relations.list()?;
                assert_eq!(
                    rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
                    ["cites", "co_occurs", "derived_from", "user_linked"]
                );
                let co_occurs = relations.find_by_name("co_occurs")?.expect("seeded");
                assert!(!co_occurs.directed);
                assert_eq!(co_occurs.id, "01999a00-0003-7000-8000-000000000003");
                assert_eq!(
                    relations.find_by_id(&co_occurs.id)?,
                    Some(co_occurs.clone())
                );
                assert_eq!(
                    relations
                        .list_by_ids(&[co_occurs.id.clone(), "missing".to_owned()])?
                        .len(),
                    1
                );
                assert_eq!(relations.list_by_ids(&[])?, Vec::new());
                let defined = relations.define(&GraphRelationRow {
                    id: "r5".to_owned(),
                    name: "mentions".to_owned(),
                    description: String::new(),
                    directed: true,
                })?;
                assert_eq!(defined.name, "mentions");
                Ok(())
            })
            .expect("the repository works");
    }
}
