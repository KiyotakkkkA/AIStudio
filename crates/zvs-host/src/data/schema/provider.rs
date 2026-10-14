use rusqlite::Row;
use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zvs_contract::{
    AdapterFamily, AuthMode, ProviderCapability, ProviderKind, ProviderStatus, Timestamp,
};

use super::columns::{EnumColumn, json_column, json_text};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderSettings(pub Map<String, Value>);

impl ProviderSettings {
    pub const TIMEOUT_SECONDS: &str = "timeoutSeconds";
    pub const SELECTED_MODEL_IDS: &str = "selectedModelIds";

    pub fn timeout_seconds(&self) -> Option<f64> {
        self.0.get(Self::TIMEOUT_SECONDS).and_then(Value::as_f64)
    }

    pub fn timeout_text(&self) -> String {
        match self.0.get(Self::TIMEOUT_SECONDS) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(text)) => text.clone(),
            Some(other) => other.to_string(),
        }
    }

    pub fn selected_model_ids(&self) -> Vec<String> {
        self.0
            .get(Self::SELECTED_MODEL_IDS)
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.0.insert(key.to_owned(), value);
        self
    }
}

impl FromSql for ProviderSettings {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        serde_json::from_str(value.as_str()?)
            .map_err(|error| rusqlite::types::FromSqlError::Other(Box::new(error)))
    }
}

impl ToSql for ProviderSettings {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(json_text(&self.0)?))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRow {
    pub id: String,
    pub kind: ProviderKind,
    pub adapter: String,
    pub auth_mode: String,
    pub name: String,
    pub base_url: String,
    pub secret_id: Option<String>,
    pub account_id: Option<String>,
    pub capabilities: Vec<ProviderCapability>,
    pub cap_text: bool,
    pub cap_embedding: bool,
    pub cap_image: bool,
    pub enabled: bool,
    pub settings: ProviderSettings,
    pub default_model_id: Option<String>,
    pub status: ProviderStatus,
    pub status_detail: Option<String>,
    pub last_probe_at: Option<Timestamp>,
    pub last_latency_ms: Option<i64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ProviderRow {
    pub const COLUMNS: &str = "id, kind, adapter, auth_mode, name, base_url, secret_id, account_id, capabilities, cap_text, cap_embedding, cap_image, enabled, settings, default_model_id, status, status_detail, last_probe_at, last_latency_ms, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            kind: row.get::<_, EnumColumn<ProviderKind>>("kind")?.0,
            adapter: row.get("adapter")?,
            auth_mode: row.get("auth_mode")?,
            name: row.get("name")?,
            base_url: row.get("base_url")?,
            secret_id: row.get("secret_id")?,
            account_id: row.get("account_id")?,
            capabilities: json_column(row, "capabilities")?,
            cap_text: row.get("cap_text")?,
            cap_embedding: row.get("cap_embedding")?,
            cap_image: row.get("cap_image")?,
            enabled: row.get("enabled")?,
            settings: row.get("settings")?,
            default_model_id: row.get("default_model_id")?,
            status: row.get::<_, EnumColumn<ProviderStatus>>("status")?.0,
            status_detail: row.get("status_detail")?,
            last_probe_at: row.get("last_probe_at")?,
            last_latency_ms: row.get("last_latency_ms")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn adapter_family(&self) -> Option<AdapterFamily> {
        AdapterFamily::parse(&self.adapter)
    }

    pub fn auth(&self) -> Option<AuthMode> {
        AuthMode::parse(&self.auth_mode)
    }

    pub fn is_local(&self) -> bool {
        self.adapter == AdapterFamily::Local.as_str()
    }
}
