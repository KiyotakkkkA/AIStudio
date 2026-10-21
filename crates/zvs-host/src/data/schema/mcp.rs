use rusqlite::Row;
use serde_json::Value;
use zvs_contract::{
    McpCapabilityKind, McpEnv, McpHeaders, McpServerStatus, McpStartup, McpTransport, Timestamp,
};

use super::columns::{EnumColumn, json_column};
use super::tool::nullable_json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerRow {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: McpEnv,
    pub url: Option<String>,
    pub headers: McpHeaders,
    pub enabled: bool,
    pub startup: McpStartup,
    pub status: McpServerStatus,
    pub protocol_version: Option<String>,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub instructions: Option<String>,
    pub last_error: Option<String>,
    pub last_connected_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl McpServerRow {
    pub const COLUMNS: &str = "id, name, namespace, transport, command, args, env, url, headers, \
        enabled, startup, status, protocol_version, server_name, server_version, instructions, \
        last_error, \
        last_connected_at, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            namespace: row.get("namespace")?,
            transport: row.get::<_, EnumColumn<McpTransport>>("transport")?.0,
            command: row.get("command")?,
            args: json_column(row, "args")?,
            env: json_column(row, "env")?,
            url: row.get("url")?,
            headers: json_column(row, "headers")?,
            enabled: row.get("enabled")?,
            startup: row.get::<_, EnumColumn<McpStartup>>("startup")?.0,
            status: row.get::<_, EnumColumn<McpServerStatus>>("status")?.0,
            protocol_version: row.get("protocol_version")?,
            server_name: row.get("server_name")?,
            server_version: row.get("server_version")?,
            instructions: row.get("instructions")?,
            last_error: row.get("last_error")?,
            last_connected_at: row.get("last_connected_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn secret_ids(&self) -> Vec<String> {
        self.env
            .values()
            .chain(self.headers.values())
            .filter_map(zvs_contract::McpValue::secret_id)
            .map(|id| id.to_string())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpCapabilityRow {
    pub id: String,
    pub server_id: String,
    pub kind: McpCapabilityKind,
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub tool_key: Option<String>,
    pub uri: Option<String>,
    pub mime_type: Option<String>,
    pub schema: Value,
    pub read_only: bool,
    pub destructive: bool,
    pub discovered_at: Timestamp,
}

impl McpCapabilityRow {
    pub const COLUMNS: &str = "id, server_id, kind, name, title, description, tool_key, uri, \
        mime_type, schema, read_only, destructive, discovered_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            server_id: row.get("server_id")?,
            kind: row.get::<_, EnumColumn<McpCapabilityKind>>("kind")?.0,
            name: row.get("name")?,
            title: row.get("title")?,
            description: row.get("description")?,
            tool_key: row.get("tool_key")?,
            uri: row.get("uri")?,
            mime_type: row.get("mime_type")?,
            schema: nullable_json(row, "schema")?,
            read_only: row.get("read_only")?,
            destructive: row.get("destructive")?,
            discovered_at: row.get("discovered_at")?,
        })
    }
}
