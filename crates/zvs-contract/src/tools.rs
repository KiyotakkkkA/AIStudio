use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{RunId, ToolId};
use crate::primitives::{Json, Timestamp};
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(ToolTier {
    Auto => "auto",
    Ask => "ask",
    Off => "off",
});

wire_enum!(ToolSource {
    Builtin => "builtin",
    Mcp => "mcp",
    Skill => "skill",
    Integration => "integration",
    Browser => "browser",
});

wire_enum!(ToolReferenceKind {
    Scenario => "scenario",
    Skill => "skill",
});

wire_enum!(ToolDefaultTier {
    Inferred => "inferred",
    Auto => "auto",
    Ask => "ask",
    Off => "off",
});

pub const TOOL_KEY_MAX: usize = 128;

fn segment_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

pub fn is_tool_key(value: &str) -> bool {
    let mut segments = value.split('.');
    let Some(head) = segments.next() else {
        return false;
    };
    let mut head_chars = head.chars();
    if !head_chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        || !head_chars.all(|character| character.is_ascii_alphanumeric())
    {
        return false;
    }
    let mut tail = 0;
    for segment in segments {
        if segment.is_empty() || !segment.chars().all(segment_char) {
            return false;
        }
        tail += 1;
    }
    tail > 0
}

pub fn check_tool_key(field: &str, value: &str) -> Result<(), AppError> {
    let trimmed = js_trim(value);
    check::non_empty(field, trimmed)?;
    check::max_len(field, trimmed, TOOL_KEY_MAX)?;
    if is_tool_key(trimmed) {
        Ok(())
    } else {
        Err(invalid(field, "pattern"))
    }
}

fn check_optional_len(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::max_len(field, value, max))
}

fn check_optional_timestamp(field: &str, value: Option<Timestamp>) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::timestamp(field, value))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReferenceDto {
    pub kind: ToolReferenceKind,
    pub ref_id: String,
    pub name: String,
}

impl Validate for ToolReferenceDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("refId", &self.ref_id)?;
        check::max_len("refId", &self.ref_id, 128)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, 200)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDto {
    pub id: ToolId,
    pub key: String,
    pub source: ToolSource,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub source_ref: Option<String>,
    pub display_name: String,
    pub description: String,
    pub input_schema: Json,
    pub read_only: bool,
    pub destructive: bool,
    pub tier: ToolTier,
    pub enabled: bool,
    pub available: bool,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_used_at: Option<Timestamp>,
    pub call_count: u64,
    pub registered_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ToolDto {
    pub const fn needs_confirmation(&self) -> bool {
        tool_needs_confirmation(self.tier, self.destructive)
    }
}

impl Validate for ToolDto {
    fn validate(&self) -> Result<(), AppError> {
        check_tool_key("key", &self.key)?;
        check_optional_len("sourceRef", self.source_ref.as_ref(), 256)?;
        check::non_empty("displayName", &self.display_name)?;
        check::max_len("displayName", &self.display_name, 200)?;
        check::max_len("description", &self.description, 4000)?;
        check_optional_timestamp("lastUsedAt", self.last_used_at)?;
        check::safe_integer("callCount", self.call_count)?;
        check::timestamp("registeredAt", self.registered_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetailDto {
    #[serde(flatten)]
    pub tool: ToolDto,
    pub references: Vec<ToolReferenceDto>,
}

impl Validate for ToolDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        self.tool.validate()?;
        nested("references", self.references.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsageDto {
    pub id: ToolId,
    pub key: String,
    pub call_count: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_used_at: Option<Timestamp>,
    pub references: Vec<ToolReferenceDto>,
}

impl Validate for ToolUsageDto {
    fn validate(&self) -> Result<(), AppError> {
        check_tool_key("key", &self.key)?;
        check::safe_integer("callCount", self.call_count)?;
        check_optional_timestamp("lastUsedAt", self.last_used_at)?;
        nested("references", self.references.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRef {
    pub id: ToolId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolListFilter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub source: Option<ToolSource>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub tier: Option<ToolTier>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub query: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub include_unavailable: Option<bool>,
}

impl ToolListFilter {
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref().map(js_trim)
    }

    pub fn include_unavailable(&self) -> bool {
        self.include_unavailable.unwrap_or(true)
    }
}

impl Validate for ToolListFilter {
    fn validate(&self) -> Result<(), AppError> {
        self.query()
            .map_or(Ok(()), |query| check::max_len("query", query, 128))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetToolTierInput {
    pub id: ToolId,
    pub tier: ToolTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetToolEnabledInput {
    pub id: ToolId,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefaultsDto {
    #[serde(default = "inferred")]
    pub builtin: ToolDefaultTier,
    #[serde(default = "inferred")]
    pub mcp: ToolDefaultTier,
    #[serde(default = "inferred")]
    pub skill: ToolDefaultTier,
    #[serde(default = "inferred")]
    pub integration: ToolDefaultTier,
    #[serde(default = "inferred")]
    pub browser: ToolDefaultTier,
}

const fn inferred() -> ToolDefaultTier {
    ToolDefaultTier::Inferred
}

impl Default for ToolDefaultsDto {
    fn default() -> Self {
        Self {
            builtin: ToolDefaultTier::Inferred,
            mcp: ToolDefaultTier::Inferred,
            skill: ToolDefaultTier::Inferred,
            integration: ToolDefaultTier::Inferred,
            browser: ToolDefaultTier::Inferred,
        }
    }
}

impl ToolDefaultsDto {
    pub const fn for_source(&self, source: ToolSource) -> ToolDefaultTier {
        match source {
            ToolSource::Builtin => self.builtin,
            ToolSource::Mcp => self.mcp,
            ToolSource::Skill => self.skill,
            ToolSource::Integration => self.integration,
            ToolSource::Browser => self.browser,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvokeInput {
    pub id: ToolId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub input: Option<Json>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub confirmed: Option<bool>,
}

impl ToolInvokeInput {
    pub fn new(id: ToolId) -> Self {
        Self {
            id,
            input: None,
            confirmed: None,
        }
    }

    pub fn input(&self) -> Json {
        self.input
            .clone()
            .unwrap_or_else(|| Json::Object(serde_json::Map::new()))
    }

    pub fn confirmed(&self) -> bool {
        self.confirmed.unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvokeResultDto {
    pub run_id: RunId,
    pub ok: bool,
    pub duration_ms: u64,
    pub output: Json,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub error: Option<String>,
}

impl Validate for ToolInvokeResultDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("durationMs", self.duration_ms)
    }
}

pub const fn tool_needs_confirmation(tier: ToolTier, destructive: bool) -> bool {
    destructive || matches!(tier, ToolTier::Ask)
}

no_refinements!(
    ToolRef,
    SetToolTierInput,
    SetToolEnabledInput,
    ToolDefaultsDto
);

impl Validate for ToolInvokeInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn tiers_use_the_wire_names() {
        assert_eq!(
            serde_json::to_value(ToolTier::ALL).expect("serialises"),
            json!(["auto", "ask", "off"])
        );
        assert_eq!(ToolTier::parse("ask"), Some(ToolTier::Ask));
        assert_eq!(ToolTier::parse("ASK"), None);
    }

    #[test]
    fn a_tool_key_is_a_namespace_and_at_least_one_verb() {
        for key in [
            "mail.send",
            "qdrant.list-collections",
            "a.b.c",
            "vector.search_v2",
        ] {
            assert!(is_tool_key(key), "{key}");
        }
        for key in [
            "mail",
            "Mail.send",
            "mail.",
            ".send",
            "mail..send",
            "ma-il.send",
            "1a.b",
        ] {
            assert!(!is_tool_key(key), "{key}");
        }
        assert!(check_tool_key("key", "  mail.send ").is_ok());
        assert!(check_tool_key("key", &format!("a.{}", "b".repeat(127))).is_err());
    }

    #[test]
    fn the_filter_and_the_invoke_input_apply_their_defaults() {
        let filter = parse::<ToolListFilter>(json!({})).expect("parses");
        assert!(filter.include_unavailable());
        assert!(parse::<ToolListFilter>(json!({ "query": null })).is_err());
        let invoke =
            parse::<ToolInvokeInput>(json!({ "id": "0199aa11-1111-7111-8111-111111111111" }))
                .expect("parses");
        assert_eq!(invoke.input(), json!({}));
        assert!(!invoke.confirmed());
        let defaults = parse::<ToolDefaultsDto>(json!({ "mcp": "off" })).expect("parses");
        assert_eq!(defaults.for_source(ToolSource::Mcp), ToolDefaultTier::Off);
        assert_eq!(defaults.builtin, ToolDefaultTier::Inferred);
    }
}
