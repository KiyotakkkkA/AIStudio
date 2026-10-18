use std::collections::HashMap;

use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use zvs_contract::{
    McpCapabilityKind, McpEnv, McpHeaders, McpServerStatus, McpStartup, McpTransport, Timestamp,
};

use crate::data::schema::columns::{EnumColumn, json_text};
use crate::data::schema::tool::nullable_json_text;
use crate::data::schema::{McpCapabilityRow, McpServerRow};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilityCounts {
    pub tool: u64,
    pub resource: u64,
    pub prompt: u64,
}

impl CapabilityCounts {
    fn add(&mut self, kind: McpCapabilityKind) {
        match kind {
            McpCapabilityKind::Tool => self.tool += 1,
            McpCapabilityKind::Resource => self.resource += 1,
            McpCapabilityKind::Prompt => self.prompt += 1,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpServerPatch {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub transport: Option<McpTransport>,
    pub command: Option<Option<String>>,
    pub args: Option<Vec<String>>,
    pub env: Option<McpEnv>,
    pub url: Option<Option<String>>,
    pub headers: Option<McpHeaders>,
    pub enabled: Option<bool>,
    pub startup: Option<McpStartup>,
    pub status: Option<McpServerStatus>,
    pub protocol_version: Option<Option<String>>,
    pub server_name: Option<Option<String>>,
    pub server_version: Option<Option<String>>,
    pub last_error: Option<Option<String>>,
    pub last_connected_at: Option<Option<Timestamp>>,
    pub updated_at: Option<Timestamp>,
}

fn text(value: &str) -> SqlValue {
    SqlValue::Text(value.to_owned())
}

fn optional_text(value: Option<&str>) -> SqlValue {
    value.map_or(SqlValue::Null, text)
}

pub struct McpServerRepository<'c> {
    connection: &'c Connection,
}

impl<'c> McpServerRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    fn servers(&self, clause: &str, values: Vec<SqlValue>) -> rusqlite::Result<Vec<McpServerRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM mcp_server {clause}",
                McpServerRow::COLUMNS
            ))?
            .query_map(params_from_iter(values), McpServerRow::from_row)?
            .collect()
    }

    fn capabilities_where(
        &self,
        clause: &str,
        values: Vec<SqlValue>,
    ) -> rusqlite::Result<Vec<McpCapabilityRow>> {
        self.connection
            .prepare(&format!(
                "SELECT {} FROM mcp_capability {clause}",
                McpCapabilityRow::COLUMNS
            ))?
            .query_map(params_from_iter(values), McpCapabilityRow::from_row)?
            .collect()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<McpServerRow>> {
        self.servers("ORDER BY name", Vec::new())
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<McpServerRow>> {
        Ok(self
            .servers("WHERE id = ?1", vec![text(id)])?
            .into_iter()
            .next())
    }

    pub fn find_by_namespace(&self, namespace: &str) -> rusqlite::Result<Option<McpServerRow>> {
        Ok(self
            .servers("WHERE namespace = ?1", vec![text(namespace)])?
            .into_iter()
            .next())
    }

    pub fn list_auto_start(&self) -> rusqlite::Result<Vec<McpServerRow>> {
        self.servers(
            "WHERE enabled = 1 AND startup = 'launch' ORDER BY name",
            Vec::new(),
        )
    }

    pub fn insert(&self, row: &McpServerRow) -> rusqlite::Result<McpServerRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO mcp_server ({columns}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, \
                 ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19) RETURNING {columns}",
                columns = McpServerRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.name,
                    row.namespace,
                    EnumColumn(row.transport),
                    row.command,
                    json_text(&row.args)?,
                    json_text(&row.env)?,
                    row.url,
                    json_text(&row.headers)?,
                    row.enabled,
                    EnumColumn(row.startup),
                    EnumColumn(row.status),
                    row.protocol_version,
                    row.server_name,
                    row.server_version,
                    row.last_error,
                    row.last_connected_at,
                    row.created_at,
                    row.updated_at,
                ],
                McpServerRow::from_row,
            )
    }

    pub fn update(
        &self,
        id: &str,
        patch: &McpServerPatch,
    ) -> rusqlite::Result<Option<McpServerRow>> {
        let mut assignments: Vec<&str> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if let Some(name) = &patch.name {
            assignments.push("name = ?");
            values.push(text(name));
        }
        if let Some(namespace) = &patch.namespace {
            assignments.push("namespace = ?");
            values.push(text(namespace));
        }
        if let Some(transport) = patch.transport {
            assignments.push("transport = ?");
            values.push(text(transport.as_str()));
        }
        if let Some(args) = &patch.args {
            assignments.push("args = ?");
            values.push(SqlValue::Text(json_text(args)?));
        }
        if let Some(env) = &patch.env {
            assignments.push("env = ?");
            values.push(SqlValue::Text(json_text(env)?));
        }
        if let Some(headers) = &patch.headers {
            assignments.push("headers = ?");
            values.push(SqlValue::Text(json_text(headers)?));
        }
        if let Some(enabled) = patch.enabled {
            assignments.push("enabled = ?");
            values.push(SqlValue::Integer(i64::from(enabled)));
        }
        if let Some(startup) = patch.startup {
            assignments.push("startup = ?");
            values.push(text(startup.as_str()));
        }
        if let Some(status) = patch.status {
            assignments.push("status = ?");
            values.push(text(status.as_str()));
        }
        for (column, value) in [
            ("command = ?", &patch.command),
            ("url = ?", &patch.url),
            ("protocol_version = ?", &patch.protocol_version),
            ("server_name = ?", &patch.server_name),
            ("server_version = ?", &patch.server_version),
            ("last_error = ?", &patch.last_error),
        ] {
            if let Some(value) = value {
                assignments.push(column);
                values.push(optional_text(value.as_deref()));
            }
        }
        if let Some(connected) = patch.last_connected_at {
            assignments.push("last_connected_at = ?");
            values.push(connected.map_or(SqlValue::Null, SqlValue::Integer));
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
                "UPDATE mcp_server SET {} WHERE id = ? RETURNING {}",
                assignments.join(", "),
                McpServerRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), McpServerRow::from_row)
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM mcp_server WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    pub fn capabilities(&self, server_id: &str) -> rusqlite::Result<Vec<McpCapabilityRow>> {
        self.capabilities_where(
            "WHERE server_id = ?1 ORDER BY kind, name",
            vec![text(server_id)],
        )
    }

    pub fn capabilities_of_kind(
        &self,
        server_id: &str,
        kind: McpCapabilityKind,
    ) -> rusqlite::Result<Vec<McpCapabilityRow>> {
        self.capabilities_where(
            "WHERE server_id = ?1 AND kind = ?2 ORDER BY name",
            vec![text(server_id), text(kind.as_str())],
        )
    }

    pub fn find_capability_by_tool_key(
        &self,
        tool_key: &str,
    ) -> rusqlite::Result<Option<McpCapabilityRow>> {
        Ok(self
            .capabilities_where("WHERE tool_key = ?1", vec![text(tool_key)])?
            .into_iter()
            .next())
    }

    pub fn list_tool_capabilities(&self) -> rusqlite::Result<Vec<McpCapabilityRow>> {
        self.capabilities_where("WHERE kind = 'tool' ORDER BY name", Vec::new())
    }

    pub fn upsert_capability(&self, row: &McpCapabilityRow) -> rusqlite::Result<McpCapabilityRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO mcp_capability ({columns}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, \
                 ?9, ?10, ?11, ?12, ?13) ON CONFLICT (server_id, kind, name) DO UPDATE SET \
                 title = excluded.title, description = excluded.description, \
                 tool_key = excluded.tool_key, uri = excluded.uri, mime_type = excluded.mime_type, \
                 schema = excluded.schema, read_only = excluded.read_only, \
                 destructive = excluded.destructive, discovered_at = excluded.discovered_at \
                 RETURNING {columns}",
                columns = McpCapabilityRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.server_id,
                    EnumColumn(row.kind),
                    row.name,
                    row.title,
                    row.description,
                    row.tool_key,
                    row.uri,
                    row.mime_type,
                    nullable_json_text(&row.schema)?,
                    row.read_only,
                    row.destructive,
                    row.discovered_at,
                ],
                McpCapabilityRow::from_row,
            )
    }

    pub fn prune_capabilities(&self, server_id: &str, keep_ids: &[String]) -> rusqlite::Result<()> {
        let mut values = vec![text(server_id)];
        let mut sql = "DELETE FROM mcp_capability WHERE server_id = ?".to_owned();
        if !keep_ids.is_empty() {
            sql.push_str(&format!(
                " AND id NOT IN ({})",
                vec!["?"; keep_ids.len()].join(", ")
            ));
            values.extend(keep_ids.iter().cloned().map(SqlValue::Text));
        }
        self.connection
            .prepare(&sql)?
            .execute(params_from_iter(values))?;
        Ok(())
    }

    pub fn capability_counts(&self, server_id: &str) -> rusqlite::Result<CapabilityCounts> {
        let mut counts = CapabilityCounts::default();
        for row in self.capabilities(server_id)? {
            counts.add(row.kind);
        }
        Ok(counts)
    }

    pub fn counts_for_servers(
        &self,
        server_ids: &[String],
    ) -> rusqlite::Result<HashMap<String, CapabilityCounts>> {
        let mut counts: HashMap<String, CapabilityCounts> = HashMap::new();
        if server_ids.is_empty() {
            return Ok(counts);
        }
        let rows = self.capabilities_where(
            &format!(
                "WHERE server_id IN ({})",
                vec!["?"; server_ids.len()].join(", ")
            ),
            server_ids.iter().cloned().map(SqlValue::Text).collect(),
        )?;
        for row in rows {
            counts.entry(row.server_id).or_default().add(row.kind);
        }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zvs_contract::McpValue;

    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    fn server(id: &str, name: &str, startup: McpStartup) -> McpServerRow {
        McpServerRow {
            id: id.to_owned(),
            name: name.to_owned(),
            namespace: name.to_owned(),
            transport: McpTransport::Stdio,
            command: Some("node".to_owned()),
            args: vec!["server.mjs".to_owned()],
            env: McpEnv::from([("MODE".to_owned(), McpValue::literal("x"))]),
            url: None,
            headers: McpHeaders::new(),
            enabled: true,
            startup,
            status: McpServerStatus::Stopped,
            protocol_version: None,
            server_name: None,
            server_version: None,
            last_error: None,
            last_connected_at: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn capability(
        id: &str,
        server_id: &str,
        kind: McpCapabilityKind,
        name: &str,
    ) -> McpCapabilityRow {
        McpCapabilityRow {
            id: id.to_owned(),
            server_id: server_id.to_owned(),
            kind,
            name: name.to_owned(),
            title: None,
            description: String::new(),
            tool_key: (kind == McpCapabilityKind::Tool).then(|| format!("ns.{name}")),
            uri: None,
            mime_type: None,
            schema: Value::Null,
            read_only: false,
            destructive: false,
            discovered_at: 1,
        }
    }

    #[test]
    fn servers_round_trip_their_json_columns_and_list_the_ones_to_launch() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let servers = Repositories::new(connection).mcp_servers;
                let created = servers.insert(&server("a", "alpha", McpStartup::Launch))?;
                assert_eq!(created.env["MODE"], McpValue::literal("x"));
                servers.insert(&server("b", "beta", McpStartup::Manual))?;
                let patched = servers
                    .update(
                        "a",
                        &McpServerPatch {
                            command: Some(None),
                            status: Some(McpServerStatus::Connected),
                            last_connected_at: Some(Some(7)),
                            ..McpServerPatch::default()
                        },
                    )?
                    .expect("a row");
                assert_eq!(patched.command, None);
                assert_eq!(patched.status, McpServerStatus::Connected);
                assert_eq!(patched.last_connected_at, Some(7));
                assert_eq!(
                    servers
                        .list_auto_start()?
                        .into_iter()
                        .map(|row| row.id)
                        .collect::<Vec<_>>(),
                    ["a"]
                );
                assert_eq!(
                    servers.find_by_namespace("beta")?.map(|row| row.id),
                    Some("b".to_owned())
                );
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn capabilities_upsert_by_name_prune_and_cascade_with_their_server() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let servers = Repositories::new(connection).mcp_servers;
                servers.insert(&server("a", "alpha", McpStartup::Manual))?;
                let first = servers.upsert_capability(&capability(
                    "c1",
                    "a",
                    McpCapabilityKind::Tool,
                    "search",
                ))?;
                let again = servers.upsert_capability(&McpCapabilityRow {
                    schema: json!({ "type": "object" }),
                    ..capability("c9", "a", McpCapabilityKind::Tool, "search")
                })?;
                assert_eq!(again.id, first.id);
                assert_eq!(again.schema, json!({ "type": "object" }));
                servers.upsert_capability(&capability(
                    "c2",
                    "a",
                    McpCapabilityKind::Resource,
                    "docs",
                ))?;
                servers.upsert_capability(&capability(
                    "c3",
                    "a",
                    McpCapabilityKind::Prompt,
                    "summarise",
                ))?;
                assert_eq!(
                    servers.capability_counts("a")?,
                    CapabilityCounts {
                        tool: 1,
                        resource: 1,
                        prompt: 1
                    }
                );
                assert_eq!(
                    servers
                        .find_capability_by_tool_key("ns.search")?
                        .map(|row| row.name),
                    Some("search".to_owned())
                );
                servers.prune_capabilities("a", &["c1".to_owned()])?;
                assert_eq!(servers.capabilities("a")?.len(), 1);
                assert_eq!(servers.counts_for_servers(&["a".to_owned()])?["a"].tool, 1);
                servers.remove("a")?;
                assert!(servers.list_tool_capabilities()?.is_empty());
                Ok(())
            })
            .expect("the repository works");
    }
}
