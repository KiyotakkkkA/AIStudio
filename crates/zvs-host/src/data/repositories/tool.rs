use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::Value;
use zvs_contract::{Timestamp, ToolReferenceKind, ToolSource, ToolTier};

use crate::data::schema::columns::EnumColumn;
use crate::data::schema::tool::nullable_json_text;
use crate::data::schema::{ToolReferenceRow, ToolRow};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolPatch {
    pub source: Option<ToolSource>,
    pub source_ref: Option<Option<String>>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub tier: Option<ToolTier>,
    pub enabled: Option<bool>,
    pub available: Option<bool>,
    pub updated_at: Option<Timestamp>,
}

pub struct ToolRepository<'c> {
    connection: &'c Connection,
}

impl<'c> ToolRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    fn select(&self, clause: &str, values: Vec<SqlValue>) -> rusqlite::Result<Vec<ToolRow>> {
        self.connection
            .prepare_cached(&format!("SELECT {} FROM tool {clause}", ToolRow::COLUMNS))?
            .query_map(params_from_iter(values), ToolRow::from_row)?
            .collect()
    }

    fn one(&self, clause: &str, value: &str) -> rusqlite::Result<Option<ToolRow>> {
        self.connection
            .prepare_cached(&format!("SELECT {} FROM tool {clause}", ToolRow::COLUMNS))?
            .query_row([value], ToolRow::from_row)
            .optional()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<ToolRow>> {
        self.select("ORDER BY key", Vec::new())
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<ToolRow>> {
        self.one("WHERE id = ?1", id)
    }

    pub fn find_by_key(&self, key: &str) -> rusqlite::Result<Option<ToolRow>> {
        self.one("WHERE key = ?1", key)
    }

    pub fn list_by_source(
        &self,
        source: ToolSource,
        source_ref: Option<&str>,
    ) -> rusqlite::Result<Vec<ToolRow>> {
        match source_ref {
            Some(source_ref) => self.select(
                "WHERE source = ?1 AND source_ref = ?2 ORDER BY key",
                vec![
                    SqlValue::Text(source.as_str().to_owned()),
                    SqlValue::Text(source_ref.to_owned()),
                ],
            ),
            None => self.select(
                "WHERE source = ?1 AND source_ref IS NULL ORDER BY key",
                vec![SqlValue::Text(source.as_str().to_owned())],
            ),
        }
    }

    pub fn insert(&self, row: &ToolRow) -> rusqlite::Result<ToolRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO tool ({columns}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16) \
                 RETURNING {columns}",
                columns = ToolRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.key,
                    EnumColumn(row.source),
                    row.source_ref,
                    row.display_name,
                    row.description,
                    nullable_json_text(&row.input_schema)?,
                    row.read_only,
                    row.destructive,
                    EnumColumn(row.tier),
                    row.enabled,
                    row.available,
                    row.last_used_at,
                    i64::try_from(row.call_count).unwrap_or(i64::MAX),
                    row.registered_at,
                    row.updated_at,
                ],
                ToolRow::from_row,
            )
    }

    pub fn update(&self, id: &str, patch: &ToolPatch) -> rusqlite::Result<Option<ToolRow>> {
        let mut assignments: Vec<&str> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        let text = |value: &str| SqlValue::Text(value.to_owned());
        if let Some(source) = patch.source {
            assignments.push("source = ?");
            values.push(text(source.as_str()));
        }
        if let Some(source_ref) = &patch.source_ref {
            assignments.push("source_ref = ?");
            values.push(source_ref.as_deref().map_or(SqlValue::Null, text));
        }
        if let Some(display_name) = &patch.display_name {
            assignments.push("display_name = ?");
            values.push(text(display_name));
        }
        if let Some(description) = &patch.description {
            assignments.push("description = ?");
            values.push(text(description));
        }
        if let Some(schema) = &patch.input_schema {
            assignments.push("input_schema = ?");
            values.push(nullable_json_text(schema)?.map_or(SqlValue::Null, SqlValue::Text));
        }
        for (column, flag) in [
            ("read_only = ?", patch.read_only),
            ("destructive = ?", patch.destructive),
            ("enabled = ?", patch.enabled),
            ("available = ?", patch.available),
        ] {
            if let Some(flag) = flag {
                assignments.push(column);
                values.push(SqlValue::Integer(i64::from(flag)));
            }
        }
        if let Some(tier) = patch.tier {
            assignments.push("tier = ?");
            values.push(text(tier.as_str()));
        }
        if let Some(updated_at) = patch.updated_at {
            assignments.push("updated_at = ?");
            values.push(SqlValue::Integer(updated_at));
        }
        if assignments.is_empty() {
            return self.find_by_id(id);
        }
        values.push(text(id));
        self.connection
            .prepare(&format!(
                "UPDATE tool SET {} WHERE id = ? RETURNING {}",
                assignments.join(", "),
                ToolRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), ToolRow::from_row)
            .optional()
    }

    pub fn record_use(&self, id: &str, used_at: Timestamp) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "UPDATE tool SET last_used_at = ?2, call_count = call_count + 1 WHERE id = ?1",
            )?
            .execute(params![id, used_at])?;
        Ok(())
    }

    pub fn references(&self, tool_id: &str) -> rusqlite::Result<Vec<ToolReferenceRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM tool_reference WHERE tool_id = ?1 ORDER BY kind, name",
                ToolReferenceRow::COLUMNS
            ))?
            .query_map([tool_id], ToolReferenceRow::from_row)?
            .collect()
    }

    pub fn link_reference(&self, row: &ToolReferenceRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "INSERT INTO tool_reference (id, tool_id, kind, ref_id, name) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT (tool_id, kind, ref_id) DO UPDATE SET name = excluded.name",
            )?
            .execute(params![
                row.id,
                row.tool_id,
                EnumColumn(row.kind),
                row.ref_id,
                row.name
            ])?;
        Ok(())
    }

    pub fn clear_references(
        &self,
        kind: ToolReferenceKind,
        ref_id: &str,
        keep_tool_ids: &[String],
    ) -> rusqlite::Result<()> {
        let mut values = vec![
            SqlValue::Text(kind.as_str().to_owned()),
            SqlValue::Text(ref_id.to_owned()),
        ];
        let mut sql = "DELETE FROM tool_reference WHERE kind = ? AND ref_id = ?".to_owned();
        if !keep_tool_ids.is_empty() {
            sql.push_str(&format!(
                " AND tool_id NOT IN ({})",
                vec!["?"; keep_tool_ids.len()].join(", ")
            ));
            values.extend(keep_tool_ids.iter().cloned().map(SqlValue::Text));
        }
        self.connection
            .prepare(&sql)?
            .execute(params_from_iter(values))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    fn row(id: &str, key: &str, source_ref: Option<&str>) -> ToolRow {
        ToolRow {
            id: id.to_owned(),
            key: key.to_owned(),
            source: ToolSource::Mcp,
            source_ref: source_ref.map(str::to_owned),
            display_name: key.to_owned(),
            description: String::new(),
            input_schema: Value::Null,
            read_only: false,
            destructive: false,
            tier: ToolTier::Ask,
            enabled: true,
            available: true,
            last_used_at: None,
            call_count: 0,
            registered_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn rows_round_trip_patch_and_count_uses() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let tools = Repositories::new(connection).tools;
                let created = tools.insert(&row("t1", "mail.send", Some("s1")))?;
                assert_eq!(created.input_schema, Value::Null);
                tools.insert(&row("t2", "fs.read", None))?;
                let patched = tools
                    .update(
                        "t1",
                        &ToolPatch {
                            input_schema: Some(json!({ "type": "object" })),
                            tier: Some(ToolTier::Off),
                            source_ref: Some(None),
                            ..ToolPatch::default()
                        },
                    )?
                    .expect("a row");
                assert_eq!(patched.input_schema, json!({ "type": "object" }));
                assert_eq!(patched.tier, ToolTier::Off);
                assert_eq!(patched.source_ref, None);
                tools.record_use("t1", 5)?;
                tools.record_use("t1", 9)?;
                let used = tools.find_by_key("mail.send")?.expect("a row");
                assert_eq!((used.call_count, used.last_used_at), (2, Some(9)));
                assert_eq!(
                    tools
                        .list()?
                        .into_iter()
                        .map(|row| row.key)
                        .collect::<Vec<_>>(),
                    ["fs.read", "mail.send"]
                );
                assert_eq!(tools.list_by_source(ToolSource::Mcp, None)?.len(), 2);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn references_upsert_by_subject_and_clear_all_but_the_kept_tools() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let tools = Repositories::new(connection).tools;
                tools.insert(&row("t1", "mail.send", None))?;
                tools.insert(&row("t2", "fs.read", None))?;
                for (id, tool) in [("r1", "t1"), ("r2", "t2")] {
                    tools.link_reference(&ToolReferenceRow {
                        id: id.to_owned(),
                        tool_id: tool.to_owned(),
                        kind: ToolReferenceKind::Scenario,
                        ref_id: "s1".to_owned(),
                        name: "Триаж".to_owned(),
                    })?;
                }
                tools.link_reference(&ToolReferenceRow {
                    id: "r3".to_owned(),
                    tool_id: "t1".to_owned(),
                    kind: ToolReferenceKind::Scenario,
                    ref_id: "s1".to_owned(),
                    name: "Новое имя".to_owned(),
                })?;
                assert_eq!(tools.references("t1")?[0].name, "Новое имя");
                tools.clear_references(ToolReferenceKind::Scenario, "s1", &["t2".to_owned()])?;
                assert!(tools.references("t1")?.is_empty());
                assert_eq!(tools.references("t2")?.len(), 1);
                tools.clear_references(ToolReferenceKind::Scenario, "s1", &[])?;
                assert!(tools.references("t2")?.is_empty());
                Ok(())
            })
            .expect("the repository works");
    }
}
