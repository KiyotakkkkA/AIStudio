use rusqlite::Row;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use zvs_contract::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretConsumerKind {
    Provider,
    McpServer,
    Integration,
    VectorStore,
    Account,
}

impl SecretConsumerKind {
    pub const ALL: &[Self] = &[
        Self::Provider,
        Self::McpServer,
        Self::Integration,
        Self::VectorStore,
        Self::Account,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::McpServer => "mcp_server",
            Self::Integration => "integration",
            Self::VectorStore => "vector_store",
            Self::Account => "account",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }
}

impl FromSql for SecretConsumerKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        Self::parse(text)
            .ok_or_else(|| FromSqlError::Other(format!("unknown consumer kind «{text}»").into()))
    }
}

impl ToSql for SecretConsumerKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretUsageRow {
    pub secret_id: String,
    pub consumer_kind: SecretConsumerKind,
    pub consumer_id: String,
    pub created_at: Timestamp,
}

impl SecretUsageRow {
    pub const COLUMNS: &str = "secret_id, consumer_kind, consumer_id, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            secret_id: row.get("secret_id")?,
            consumer_kind: row.get("consumer_kind")?,
            consumer_id: row.get("consumer_id")?,
            created_at: row.get("created_at")?,
        })
    }
}
