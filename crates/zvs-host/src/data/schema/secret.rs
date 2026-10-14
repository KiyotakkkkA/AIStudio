use rusqlite::Row;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use zvs_contract::{SecretScope, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeColumn(pub SecretScope);

impl FromSql for ScopeColumn {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        SecretScope::parse(text)
            .map(Self)
            .ok_or_else(|| FromSqlError::Other(format!("unknown secret scope «{text}»").into()))
    }
}

impl ToSql for ScopeColumn {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRow {
    pub id: String,
    pub r#type: String,
    pub name: String,
    pub scope: SecretScope,
    pub cipher_version: i64,
    pub hint: Option<String>,
    pub fields: String,
    pub tags: String,
    pub note: Option<String>,
    pub rotation_days: Option<i64>,
    pub rotates_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl SecretRow {
    pub const COLUMNS: &str = "id, type, name, scope, cipher_version, hint, fields, tags, note, rotation_days, rotates_at, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            r#type: row.get("type")?,
            name: row.get("name")?,
            scope: row.get::<_, ScopeColumn>("scope")?.0,
            cipher_version: row.get("cipher_version")?,
            hint: row.get("hint")?,
            fields: row.get("fields")?,
            tags: row.get("tags")?,
            note: row.get("note")?,
            rotation_days: row.get("rotation_days")?,
            rotates_at: row.get("rotates_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretCipherRow {
    pub id: String,
    pub cipher: Option<Vec<u8>>,
    pub cipher_version: i64,
}

impl std::fmt::Debug for SecretCipherRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretCipherRow")
            .field("id", &self.id)
            .field("cipher", &self.cipher.as_ref().map(|cipher| cipher.len()))
            .field("cipher_version", &self.cipher_version)
            .finish()
    }
}

impl SecretCipherRow {
    pub const COLUMNS: &str = "id, cipher, cipher_version";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            cipher: row.get("cipher")?,
            cipher_version: row.get("cipher_version")?,
        })
    }
}
