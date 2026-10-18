use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{McpServerId, SecretId};
use crate::primitives::{Json, Timestamp};
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(McpTransport {
    Stdio => "stdio",
    Http => "http",
});

wire_enum!(McpStartup {
    Manual => "manual",
    Launch => "launch",
});

wire_enum!(McpServerStatus {
    Disabled => "disabled",
    Stopped => "stopped",
    Starting => "starting",
    Connected => "connected",
    Failed => "failed",
});

wire_enum!(McpCapabilityKind {
    Tool => "tool",
    Resource => "resource",
    Prompt => "prompt",
});

wire_enum!(McpLogLevel {
    Debug => "debug",
    Info => "info",
    Warn => "warn",
    Error => "error",
});

pub const MCP_NAME_MAX: usize = 120;
pub const MCP_NAMESPACE_MAX: usize = 48;
pub const MCP_COMMAND_MAX: usize = 512;
pub const MCP_ARG_MAX: usize = 2048;
pub const MCP_URL_MAX: usize = 2048;
pub const MCP_VALUE_MAX: usize = 4096;
pub const MCP_ERROR_MAX: usize = 2000;
pub const MCP_LOG_LINE_MAX: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum McpValue {
    Literal {
        value: String,
    },
    Secret {
        #[serde(rename = "secretId")]
        secret_id: SecretId,
    },
}

impl McpValue {
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal {
            value: value.into(),
        }
    }

    pub const fn secret(secret_id: SecretId) -> Self {
        Self::Secret { secret_id }
    }

    pub const fn secret_id(&self) -> Option<SecretId> {
        match self {
            Self::Literal { .. } => None,
            Self::Secret { secret_id } => Some(*secret_id),
        }
    }
}

impl Validate for McpValue {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Literal { value } => check::max_len("value", value, MCP_VALUE_MAX),
            Self::Secret { .. } => Ok(()),
        }
    }
}

pub type McpEnv = BTreeMap<String, McpValue>;
pub type McpHeaders = BTreeMap<String, McpValue>;

pub fn is_env_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub fn is_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || "!#$%&'*+.^_`|~-".contains(character)
        })
}

pub fn is_mcp_namespace(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|character| character.is_ascii_alphanumeric())
}

pub fn check_namespace(field: &str, value: &str) -> Result<(), AppError> {
    let trimmed = js_trim(value);
    check::non_empty(field, trimmed)?;
    check::max_len(field, trimmed, MCP_NAMESPACE_MAX)?;
    if is_mcp_namespace(trimmed) {
        Ok(())
    } else {
        Err(invalid(field, "pattern"))
    }
}

fn check_values(
    field: &str,
    values: &BTreeMap<String, McpValue>,
    name_ok: fn(&str) -> bool,
) -> Result<(), AppError> {
    for (name, value) in values {
        let trimmed = js_trim(name);
        let path = format!("{field}.{name}");
        check::non_empty(&path, trimmed)?;
        check::max_len(&path, trimmed, 128)?;
        if !name_ok(trimmed) {
            return Err(invalid(&path, "pattern"));
        }
        nested(&path, value.validate())?;
    }
    Ok(())
}

pub fn check_env(field: &str, values: &McpEnv) -> Result<(), AppError> {
    check_values(field, values, is_env_name)
}

pub fn check_headers(field: &str, values: &McpHeaders) -> Result<(), AppError> {
    check_values(field, values, is_header_name)
}

pub fn trimmed_values(values: &BTreeMap<String, McpValue>) -> BTreeMap<String, McpValue> {
    values
        .iter()
        .map(|(name, value)| (js_trim(name).to_owned(), value.clone()))
        .collect()
}

fn check_args(field: &str, args: &[String]) -> Result<(), AppError> {
    args.iter()
        .enumerate()
        .try_for_each(|(index, arg)| check::max_len(&format!("{field}.{index}"), arg, MCP_ARG_MAX))
}

fn check_optional_len(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::max_len(field, value, max))
}

fn check_name(field: &str, value: &str) -> Result<(), AppError> {
    check::trimmed_name(field, value, MCP_NAME_MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDto {
    pub id: McpServerId,
    pub name: String,
    pub namespace: String,
    pub transport: McpTransport,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: McpEnv,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub url: Option<String>,
    pub headers: McpHeaders,
    pub enabled: bool,
    pub startup: McpStartup,
    pub status: McpServerStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub protocol_version: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub server_name: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub server_version: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_error: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_connected_at: Option<Timestamp>,
    pub tool_count: u64,
    pub resource_count: u64,
    pub prompt_count: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for McpServerDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, MCP_NAME_MAX)?;
        check_namespace("namespace", &self.namespace)?;
        check_optional_len("command", self.command.as_ref(), MCP_COMMAND_MAX)?;
        check_args("args", &self.args)?;
        check_env("env", &self.env)?;
        check_optional_len("url", self.url.as_ref(), MCP_URL_MAX)?;
        check_headers("headers", &self.headers)?;
        check_optional_len("protocolVersion", self.protocol_version.as_ref(), 64)?;
        check_optional_len("serverName", self.server_name.as_ref(), 200)?;
        check_optional_len("serverVersion", self.server_version.as_ref(), 64)?;
        check_optional_len("lastError", self.last_error.as_ref(), MCP_ERROR_MAX)?;
        if let Some(at) = self.last_connected_at {
            check::timestamp("lastConnectedAt", at)?;
        }
        check::safe_integer("toolCount", self.tool_count)?;
        check::safe_integer("resourceCount", self.resource_count)?;
        check::safe_integer("promptCount", self.prompt_count)?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilityDto {
    pub id: String,
    pub server_id: McpServerId,
    pub kind: McpCapabilityKind,
    pub name: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub title: Option<String>,
    pub description: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub tool_key: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub uri: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub mime_type: Option<String>,
    pub schema: Json,
    pub read_only: bool,
    pub destructive: bool,
    pub discovered_at: Timestamp,
}

impl Validate for McpCapabilityDto {
    fn validate(&self) -> Result<(), AppError> {
        if !crate::ids::is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, 200)?;
        check_optional_len("title", self.title.as_ref(), 200)?;
        check::max_len("description", &self.description, 4000)?;
        check_optional_len("toolKey", self.tool_key.as_ref(), 128)?;
        check_optional_len("uri", self.uri.as_ref(), 2048)?;
        check_optional_len("mimeType", self.mime_type.as_ref(), 200)?;
        check::timestamp("discoveredAt", self.discovered_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerDetailDto {
    #[serde(flatten)]
    pub server: McpServerDto,
    pub capabilities: Vec<McpCapabilityDto>,
}

impl Validate for McpServerDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        self.server.validate()?;
        nested("capabilities", self.capabilities.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTestResultDto {
    pub ok: bool,
    pub latency_ms: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub protocol_version: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub server_name: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub server_version: Option<String>,
    pub tool_count: u64,
    pub resource_count: u64,
    pub prompt_count: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub error: Option<String>,
}

impl Validate for McpTestResultDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("latencyMs", self.latency_ms)?;
        check_optional_len("protocolVersion", self.protocol_version.as_ref(), 64)?;
        check_optional_len("serverName", self.server_name.as_ref(), 200)?;
        check_optional_len("serverVersion", self.server_version.as_ref(), 64)?;
        check::safe_integer("toolCount", self.tool_count)?;
        check::safe_integer("resourceCount", self.resource_count)?;
        check::safe_integer("promptCount", self.prompt_count)?;
        check_optional_len("error", self.error.as_ref(), MCP_ERROR_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpLogLineDto {
    pub at: Timestamp,
    pub level: McpLogLevel,
    pub line: String,
}

impl Validate for McpLogLineDto {
    fn validate(&self) -> Result<(), AppError> {
        check::timestamp("at", self.at)?;
        check::max_len("line", &self.line, MCP_LOG_LINE_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHealthDto {
    pub server_id: McpServerId,
    pub status: McpServerStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub uptime_ms: Option<u64>,
    pub restarts: u64,
    pub restarting: bool,
    pub calls: u64,
    pub errors: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub median_latency_ms: Option<u64>,
    pub recent_log: Vec<McpLogLineDto>,
}

impl Validate for McpHealthDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(uptime) = self.uptime_ms {
            check::safe_integer("uptimeMs", uptime)?;
        }
        check::safe_integer("restarts", self.restarts)?;
        check::safe_integer("calls", self.calls)?;
        check::safe_integer("errors", self.errors)?;
        if let Some(latency) = self.median_latency_ms {
            check::safe_integer("medianLatencyMs", latency)?;
        }
        nested("recentLog", self.recent_log.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRef {
    pub id: McpServerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMcpServerInput {
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub namespace: Option<String>,
    pub transport: McpTransport,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub command: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub args: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub env: Option<McpEnv>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub url: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub headers: Option<McpHeaders>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub startup: Option<McpStartup>,
}

fn trimmed_option(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(Option::as_deref)
        .map(|text| js_trim(text).to_owned())
}

fn trimmed_url(value: Option<&Option<String>>) -> Option<String> {
    value
        .and_then(Option::as_deref)
        .map(crate::validate::normalize_url)
}

fn check_command(value: Option<&Option<String>>) -> Result<(), AppError> {
    trimmed_option(value).map_or(Ok(()), |command| {
        check::max_len("command", &command, MCP_COMMAND_MAX)
    })
}

fn check_url(value: Option<&Option<String>>) -> Result<(), AppError> {
    value
        .and_then(Option::as_deref)
        .map_or(Ok(()), |url| check::url("url", url, MCP_URL_MAX))
}

impl CreateMcpServerInput {
    pub fn new(name: impl Into<String>, transport: McpTransport) -> Self {
        Self {
            name: name.into(),
            namespace: None,
            transport,
            command: None,
            args: None,
            env: None,
            url: None,
            headers: None,
            enabled: None,
            startup: None,
        }
    }

    pub fn name(&self) -> &str {
        js_trim(&self.name)
    }

    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref().map(js_trim)
    }

    pub fn command(&self) -> Option<String> {
        trimmed_option(self.command.as_ref())
    }

    pub fn args(&self) -> Vec<String> {
        self.args.clone().unwrap_or_default()
    }

    pub fn env(&self) -> McpEnv {
        self.env.as_ref().map(trimmed_values).unwrap_or_default()
    }

    pub fn url(&self) -> Option<String> {
        trimmed_url(self.url.as_ref())
    }

    pub fn headers(&self) -> McpHeaders {
        self.headers
            .as_ref()
            .map(trimmed_values)
            .unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn startup(&self) -> McpStartup {
        self.startup.unwrap_or(McpStartup::Manual)
    }
}

impl Validate for CreateMcpServerInput {
    fn validate(&self) -> Result<(), AppError> {
        check_name("name", &self.name)?;
        if let Some(namespace) = &self.namespace {
            check_namespace("namespace", namespace)?;
        }
        check_command(self.command.as_ref())?;
        if let Some(args) = &self.args {
            check_args("args", args)?;
        }
        if let Some(env) = &self.env {
            check_env("env", env)?;
        }
        check_url(self.url.as_ref())?;
        if let Some(headers) = &self.headers {
            check_headers("headers", headers)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMcpServerInput {
    pub id: McpServerId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub namespace: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub transport: Option<McpTransport>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub command: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub args: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub env: Option<McpEnv>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub url: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub headers: Option<McpHeaders>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub startup: Option<McpStartup>,
}

impl UpdateMcpServerInput {
    pub const fn new(id: McpServerId) -> Self {
        Self {
            id,
            name: None,
            namespace: None,
            transport: None,
            command: None,
            args: None,
            env: None,
            url: None,
            headers: None,
            enabled: None,
            startup: None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref().map(js_trim)
    }

    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref().map(js_trim)
    }

    pub fn command(&self) -> Option<Option<String>> {
        self.command
            .as_ref()
            .map(|command| command.as_deref().map(|text| js_trim(text).to_owned()))
    }

    pub fn url(&self) -> Option<Option<String>> {
        self.url
            .as_ref()
            .map(|url| url.as_deref().map(crate::validate::normalize_url))
    }

    pub fn env(&self) -> Option<McpEnv> {
        self.env.as_ref().map(trimmed_values)
    }

    pub fn headers(&self) -> Option<McpHeaders> {
        self.headers.as_ref().map(trimmed_values)
    }
}

impl Validate for UpdateMcpServerInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check_name("name", name)?;
        }
        if let Some(namespace) = &self.namespace {
            check_namespace("namespace", namespace)?;
        }
        check_command(self.command.as_ref())?;
        if let Some(args) = &self.args {
            check_args("args", args)?;
        }
        if let Some(env) = &self.env {
            check_env("env", env)?;
        }
        check_url(self.url.as_ref())?;
        if let Some(headers) = &self.headers {
            check_headers("headers", headers)?;
        }
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpContentDto {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub text: Option<String>,
    pub data: Json,
}

impl Validate for McpContentDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("type", &self.content_type, 64)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpCallResultDto {
    pub content: Vec<McpContentDto>,
    pub text: String,
    pub structured: Json,
    pub is_error: bool,
}

impl Validate for McpCallResultDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("content", self.content.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRemoveOutput {
    pub id: McpServerId,
    pub removed: bool,
}

impl Validate for McpRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

no_refinements!(McpServerRef);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const SECRET: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn a_value_is_either_a_literal_or_a_secret_reference() {
        let env: McpEnv = serde_json::from_value(json!({
            "API_KEY": { "kind": "secret", "secretId": SECRET },
            "MODE": { "kind": "literal", "value": "fast" },
        }))
        .expect("parses");
        assert_eq!(env["MODE"], McpValue::literal("fast"));
        assert_eq!(
            env["API_KEY"].secret_id().map(|id| id.to_string()),
            Some(SECRET.to_owned())
        );
        assert!(
            serde_json::from_value::<McpValue>(json!({ "kind": "secret", "value": "x" })).is_err()
        );
    }

    #[test]
    fn names_follow_the_env_header_and_namespace_shapes() {
        assert!(is_env_name("_QDRANT_1"));
        assert!(!is_env_name("1QDRANT"));
        assert!(!is_env_name("QD-RANT"));
        assert!(is_header_name("X-Api-Key"));
        assert!(!is_header_name("X Api"));
        assert!(is_mcp_namespace("qdrantLocal"));
        assert!(!is_mcp_namespace("Qdrant"));
        assert!(!is_mcp_namespace("qdrant-local"));
    }

    #[test]
    fn a_draft_applies_its_defaults_and_trims_what_zod_trims() {
        let draft = parse::<CreateMcpServerInput>(json!({
            "name": "  qdrant ",
            "transport": "stdio",
            "command": " node ",
            "env": { " MODE ": { "kind": "literal", "value": "x" } },
        }))
        .expect("parses");
        assert_eq!(draft.name(), "qdrant");
        assert_eq!(draft.command().as_deref(), Some("node"));
        assert!(draft.env().contains_key("MODE"));
        assert!(draft.enabled());
        assert_eq!(draft.startup(), McpStartup::Manual);
        assert_eq!(draft.url(), None);
        assert!(
            parse::<CreateMcpServerInput>(
                json!({ "name": "x", "transport": "http", "url": "nope" })
            )
            .is_err()
        );
        assert!(
            parse::<CreateMcpServerInput>(
                json!({ "name": "x", "transport": "stdio", "command": null })
            )
            .is_ok()
        );
    }
}
