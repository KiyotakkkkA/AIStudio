use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{RunId, ScenarioId, ScenarioVersionId};
use crate::primitives::{Json, MAX_SAFE_INTEGER, Timestamp};
use crate::runs::GRAPH_TIMEOUT_MAX_MS;
use crate::validate::{Validate, check, invalid, js_trim, nested};

wire_enum!(ScenarioSlot {
    Model => "model",
    Memory => "memory",
    Tool => "tool",
    Embedding => "embedding",
});

wire_enum!(ScenarioTriggerKind {
    Chat => "chat",
    Mail => "mail",
    Schedule => "schedule",
    Webhook => "webhook",
});

wire_enum!(ScenarioOnError {
    Stop => "stop",
    Continue => "continue",
});

wire_enum!(ScenarioOverlap {
    Skip => "skip",
    Queue => "queue",
});

wire_enum!(ScenarioNodeRole {
    Trigger => "trigger",
    Action => "action",
    SubNode => "subNode",
});

wire_enum!(ScenarioIssueRule {
    DuplicateNode => "duplicateNode",
    UnknownNodeType => "unknownNodeType",
    UnknownNode => "unknownNode",
    DuplicateEdge => "duplicateEdge",
    InvalidEdge => "invalidEdge",
    Cycle => "cycle",
    UnsupportedSlot => "unsupportedSlot",
    SlotMismatch => "slotMismatch",
    MultipleParents => "multipleParents",
    MissingAttachment => "missingAttachment",
    TooManyAttachments => "tooManyAttachments",
    MissingParameter => "missingParameter",
    NoTrigger => "noTrigger",
    MultipleTriggers => "multipleTriggers",
    Unreachable => "unreachable",
    RestrictedTool => "restrictedTool",
    InvalidPort => "invalidPort",
    MissingDialog => "missingDialog",
    LimitedDialog => "limitedDialog",
    DuplicateKind => "duplicateKind",
});

impl ScenarioIssueRule {
    pub const WARNINGS: &[Self] = &[Self::LimitedDialog];

    pub fn is_warning(self) -> bool {
        Self::WARNINGS.contains(&self)
    }
}

pub const SCENARIO_NAME_MAX: usize = 120;
pub const SCENARIO_TAG_MAX: usize = 32;
pub const SCENARIO_TAGS_MAX: usize = 16;
pub const SCENARIO_NOTE_MAX: usize = 500;
pub const SCENARIO_NODE_ID_MAX: usize = 64;
pub const SCENARIO_NODE_TYPE_MAX: usize = 128;
pub const SCENARIO_NODE_NAME_MAX: usize = 120;
pub const SCENARIO_PORT_MAX: usize = 64;
pub const SCENARIO_PARAM_KEY_MAX: usize = 128;
pub const SCENARIO_NODES_MAX: usize = 200;
pub const SCENARIO_EDGES_MAX: usize = 1000;
pub const SCENARIO_ATTACHMENTS_MAX: usize = 1000;
pub const SCENARIO_ISSUES_MAX: usize = 1000;
pub const SCENARIO_MAIN_PORT: &str = "main";
pub const SCENARIO_TRUE_PORT: &str = "true";
pub const SCENARIO_FALSE_PORT: &str = "false";
pub const SCENARIO_NOTES_MAX: usize = 2000;
pub const SCENARIO_ATTEMPTS_MAX: u64 = 10;
pub const SCENARIO_BACKOFF_MAX_MS: u64 = 60_000;

pub type ScenarioParams = Map<String, Value>;

fn check_node_id(field: &str, value: &str) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, SCENARIO_NODE_ID_MAX)
}

fn check_node_ids(field: &str, values: &[String]) -> Result<(), AppError> {
    if values.len() > SCENARIO_NODES_MAX {
        return Err(invalid(field, "maxLength"));
    }
    values
        .iter()
        .enumerate()
        .try_for_each(|(index, value)| check_node_id(&format!("{field}.{index}"), value))
}

fn check_params(field: &str, params: &ScenarioParams) -> Result<(), AppError> {
    params.keys().try_for_each(|key| {
        check::non_empty(field, key)?;
        check::max_len(field, key, SCENARIO_PARAM_KEY_MAX)
    })
}

fn check_tags(tags: &[String]) -> Result<(), AppError> {
    if tags.len() > SCENARIO_TAGS_MAX {
        return Err(invalid("tags", "maxLength"));
    }
    tags.iter().enumerate().try_for_each(|(index, tag)| {
        check::trimmed_name(&format!("tags.{index}"), tag, SCENARIO_TAG_MAX)
    })
}

fn check_note(note: &str) -> Result<(), AppError> {
    check::max_len("note", note, SCENARIO_NOTE_MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioPositionDto {
    pub x: f64,
    pub y: f64,
}

impl Validate for ScenarioPositionDto {
    fn validate(&self) -> Result<(), AppError> {
        check::finite("x", self.x)?;
        check::finite("y", self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioNodeSettingsDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub max_attempts: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub backoff_ms: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub timeout_ms: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub on_error: Option<ScenarioOnError>,
}

impl ScenarioNodeSettingsDto {
    pub fn on_error(&self) -> ScenarioOnError {
        self.on_error.unwrap_or(ScenarioOnError::Stop)
    }
}

impl Validate for ScenarioNodeSettingsDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(attempts) = self.max_attempts {
            check::int_range("maxAttempts", attempts, 1, SCENARIO_ATTEMPTS_MAX)?;
        }
        if let Some(backoff) = self.backoff_ms {
            check::int_range("backoffMs", backoff, 0, SCENARIO_BACKOFF_MAX_MS)?;
        }
        if let Some(timeout) = self.timeout_ms {
            check::int_range("timeoutMs", timeout, 1, GRAPH_TIMEOUT_MAX_MS)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioNodeDto {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub name: Option<String>,
    pub position: ScenarioPositionDto,
    pub params: ScenarioParams,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub disabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub settings: Option<ScenarioNodeSettingsDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub notes: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub pinned_input: Option<Json>,
}

impl ScenarioNodeDto {
    pub fn is_disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }

    pub fn settings(&self) -> ScenarioNodeSettingsDto {
        self.settings.unwrap_or_default()
    }
}

impl Validate for ScenarioNodeDto {
    fn validate(&self) -> Result<(), AppError> {
        check_node_id("id", &self.id)?;
        check::non_empty("type", &self.node_type)?;
        check::max_len("type", &self.node_type, SCENARIO_NODE_TYPE_MAX)?;
        if let Some(name) = &self.name {
            check::max_len("name", name, SCENARIO_NODE_NAME_MAX)?;
        }
        nested("position", self.position.validate())?;
        nested("settings", self.settings.validate())?;
        if let Some(notes) = &self.notes {
            check::max_len("notes", notes, SCENARIO_NOTES_MAX)?;
        }
        check_params("params", &self.params)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioEdgeDto {
    pub from: String,
    pub to: String,
    pub from_port: String,
    pub to_port: String,
}

impl Validate for ScenarioEdgeDto {
    fn validate(&self) -> Result<(), AppError> {
        check_node_id("from", &self.from)?;
        check_node_id("to", &self.to)?;
        check::non_empty("fromPort", &self.from_port)?;
        check::max_len("fromPort", &self.from_port, SCENARIO_PORT_MAX)?;
        check::non_empty("toPort", &self.to_port)?;
        check::max_len("toPort", &self.to_port, SCENARIO_PORT_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioAttachmentDto {
    pub parent_node_id: String,
    pub child_node_id: String,
    pub slot: ScenarioSlot,
}

impl Validate for ScenarioAttachmentDto {
    fn validate(&self) -> Result<(), AppError> {
        check_node_id("parentNodeId", &self.parent_node_id)?;
        check_node_id("childNodeId", &self.child_node_id)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioGraphDto {
    pub nodes: Vec<ScenarioNodeDto>,
    pub edges: Vec<ScenarioEdgeDto>,
    pub attachments: Vec<ScenarioAttachmentDto>,
}

impl Validate for ScenarioGraphDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.nodes.len() > SCENARIO_NODES_MAX {
            return Err(invalid("nodes", "maxLength"));
        }
        if self.edges.len() > SCENARIO_EDGES_MAX {
            return Err(invalid("edges", "maxLength"));
        }
        if self.attachments.len() > SCENARIO_ATTACHMENTS_MAX {
            return Err(invalid("attachments", "maxLength"));
        }
        nested("nodes", self.nodes.validate())?;
        nested("edges", self.edges.validate())?;
        nested("attachments", self.attachments.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioIssueDto {
    pub rule: ScenarioIssueRule,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub node_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub slot: Option<ScenarioSlot>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub param: Option<String>,
}

impl ScenarioIssueDto {
    pub const fn graph(rule: ScenarioIssueRule) -> Self {
        Self {
            rule,
            node_id: None,
            slot: None,
            param: None,
        }
    }

    pub fn node(rule: ScenarioIssueRule, node_id: &str) -> Self {
        Self {
            node_id: Some(node_id.to_owned()),
            ..Self::graph(rule)
        }
    }

    pub fn slot(rule: ScenarioIssueRule, node_id: &str, slot: ScenarioSlot) -> Self {
        Self {
            slot: Some(slot),
            ..Self::node(rule, node_id)
        }
    }

    pub fn port(node_id: &str, port: &str) -> Self {
        Self {
            param: Some(port.to_owned()),
            ..Self::node(ScenarioIssueRule::InvalidPort, node_id)
        }
    }

    pub fn tool(node_id: &str, key: &str) -> Self {
        Self {
            param: Some(key.to_owned()),
            ..Self::node(ScenarioIssueRule::RestrictedTool, node_id)
        }
    }

    pub fn is_warning(&self) -> bool {
        self.rule.is_warning()
    }

    pub fn param(node_id: &str, param: &str) -> Self {
        Self {
            param: Some(param.to_owned()),
            ..Self::node(ScenarioIssueRule::MissingParameter, node_id)
        }
    }
}

impl Validate for ScenarioIssueDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(node_id) = &self.node_id {
            check_node_id("nodeId", node_id)?;
        }
        if let Some(param) = &self.param {
            check::non_empty("param", param)?;
            check::max_len("param", param, SCENARIO_PARAM_KEY_MAX)?;
        }
        Ok(())
    }
}

pub fn blocking_issues(issues: &[ScenarioIssueDto]) -> Vec<&ScenarioIssueDto> {
    issues.iter().filter(|issue| !issue.is_warning()).collect()
}

fn check_issues(issues: &[ScenarioIssueDto]) -> Result<(), AppError> {
    if issues.len() > SCENARIO_ISSUES_MAX {
        return Err(invalid("issues", "maxLength"));
    }
    issues
        .iter()
        .enumerate()
        .try_for_each(|(index, issue)| nested(&format!("issues.{index}"), issue.validate()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTriggerDto {
    pub kind: ScenarioTriggerKind,
    pub node_id: String,
    pub params: ScenarioParams,
}

impl Validate for ScenarioTriggerDto {
    fn validate(&self) -> Result<(), AppError> {
        check_node_id("nodeId", &self.node_id)?;
        check_params("params", &self.params)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDiffDto {
    pub nodes_added: Vec<String>,
    pub nodes_removed: Vec<String>,
    pub nodes_changed: Vec<String>,
    pub edges_added: u64,
    pub edges_removed: u64,
    pub attachments_added: u64,
    pub attachments_removed: u64,
}

impl ScenarioDiffDto {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Validate for ScenarioDiffDto {
    fn validate(&self) -> Result<(), AppError> {
        check_node_ids("nodesAdded", &self.nodes_added)?;
        check_node_ids("nodesRemoved", &self.nodes_removed)?;
        check_node_ids("nodesChanged", &self.nodes_changed)?;
        check::safe_integer("edgesAdded", self.edges_added)?;
        check::safe_integer("edgesRemoved", self.edges_removed)?;
        check::safe_integer("attachmentsAdded", self.attachments_added)?;
        check::safe_integer("attachmentsRemoved", self.attachments_removed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDto {
    pub id: ScenarioId,
    pub name: String,
    pub tags: Vec<String>,
    pub active: bool,
    pub overlap: ScenarioOverlap,
    pub current_version_id: ScenarioVersionId,
    pub version: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub trigger: Option<ScenarioTriggerDto>,
    pub issue_count: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for ScenarioDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, SCENARIO_NAME_MAX)?;
        check_tags(&self.tags)?;
        check::int_range("version", self.version, 1, MAX_SAFE_INTEGER.unsigned_abs())?;
        nested("trigger", self.trigger.validate())?;
        check::safe_integer("issueCount", self.issue_count)?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDetailDto {
    #[serde(flatten)]
    pub scenario: ScenarioDto,
    pub graph: ScenarioGraphDto,
    pub issues: Vec<ScenarioIssueDto>,
}

impl Validate for ScenarioDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        self.scenario.validate()?;
        nested("graph", self.graph.validate())?;
        check_issues(&self.issues)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioVersionDto {
    pub id: ScenarioVersionId,
    pub scenario_id: ScenarioId,
    pub version: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub note: Option<String>,
    pub summary: ScenarioDiffDto,
    pub created_at: Timestamp,
}

impl Validate for ScenarioVersionDto {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("version", self.version, 1, MAX_SAFE_INTEGER.unsigned_abs())?;
        if let Some(note) = &self.note {
            check_note(note)?;
        }
        nested("summary", self.summary.validate())?;
        check::timestamp("createdAt", self.created_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRef {
    pub id: ScenarioId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScenarioInput {
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub graph: Option<ScenarioGraphDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub note: Option<String>,
}

impl Validate for CreateScenarioInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, SCENARIO_NAME_MAX)?;
        if let Some(tags) = &self.tags {
            check_tags(tags)?;
        }
        nested("graph", self.graph.validate())?;
        if let Some(note) = &self.note {
            check_note(note)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateScenarioInput {
    pub id: ScenarioId,
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
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub graph: Option<ScenarioGraphDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub note: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub overlap: Option<ScenarioOverlap>,
}

impl Validate for UpdateScenarioInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check::trimmed_name("name", name, SCENARIO_NAME_MAX)?;
        }
        if let Some(tags) = &self.tags {
            check_tags(tags)?;
        }
        nested("graph", self.graph.validate())?;
        if let Some(note) = &self.note {
            check_note(note)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioValidateInput {
    pub graph: ScenarioGraphDto,
}

impl Validate for ScenarioValidateInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("graph", self.graph.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioValidationDto {
    pub valid: bool,
    pub issues: Vec<ScenarioIssueDto>,
}

impl Validate for ScenarioValidationDto {
    fn validate(&self) -> Result<(), AppError> {
        check_issues(&self.issues)?;
        if self.valid == blocking_issues(&self.issues).is_empty() {
            Ok(())
        } else {
            Err(invalid("valid", "consistency"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetScenarioActiveInput {
    pub id: ScenarioId,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRemoveOutput {
    pub id: ScenarioId,
    pub removed: bool,
}

impl Validate for ScenarioRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioSlotSpecDto {
    pub slot: ScenarioSlot,
    pub required: bool,
    pub multiple: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioNodeTypeDto {
    #[serde(rename = "type")]
    pub node_type: String,
    pub role: ScenarioNodeRole,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub trigger: Option<ScenarioTriggerKind>,
    pub fills: Vec<ScenarioSlot>,
    pub slots: Vec<ScenarioSlotSpecDto>,
    pub ports: Vec<String>,
    pub required_params: Vec<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub tool_key: Option<String>,
    pub params_schema: Json,
    pub idempotent: bool,
}

impl Validate for ScenarioNodeTypeDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("type", &self.node_type)?;
        check::max_len("type", &self.node_type, SCENARIO_NODE_TYPE_MAX)?;
        self.ports
            .iter()
            .enumerate()
            .try_for_each(|(index, port)| {
                let field = format!("ports.{index}");
                check::non_empty(&field, port)?;
                check::max_len(&field, port, SCENARIO_PORT_MAX)
            })?;
        self.required_params
            .iter()
            .enumerate()
            .try_for_each(|(index, param)| {
                let field = format!("requiredParams.{index}");
                check::non_empty(&field, param)?;
                check::max_len(&field, param, SCENARIO_PARAM_KEY_MAX)
            })?;
        if let Some(key) = &self.tool_key {
            check::non_empty("toolKey", key)?;
            check::max_len("toolKey", key, SCENARIO_NODE_TYPE_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDraftDto {
    pub scenario_id: ScenarioId,
    pub graph: ScenarioGraphDto,
    pub saved_at: Timestamp,
}

impl Validate for ScenarioDraftDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("graph", self.graph.validate())?;
        check::timestamp("savedAt", self.saved_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveScenarioDraftInput {
    pub id: ScenarioId,
    pub graph: ScenarioGraphDto,
}

impl Validate for SaveScenarioDraftInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("graph", self.graph.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRunInput {
    pub id: ScenarioId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub graph: Option<ScenarioGraphDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub input: Option<Json>,
}

impl Validate for ScenarioRunInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("graph", self.graph.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRunNodeInput {
    pub id: ScenarioId,
    pub graph: ScenarioGraphDto,
    pub node_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub input: Option<Json>,
}

impl Validate for ScenarioRunNodeInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("graph", self.graph.validate())?;
        check_node_id("nodeId", &self.node_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioRetryStepInput {
    pub run_id: RunId,
    pub node_id: String,
}

impl Validate for ScenarioRetryStepInput {
    fn validate(&self) -> Result<(), AppError> {
        check_node_id("nodeId", &self.node_id)
    }
}

crate::validate::no_refinements!(ScenarioRef, SetScenarioActiveInput, ScenarioSlotSpecDto);

pub fn normalise_tags(tags: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for tag in tags {
        let tag = js_trim(tag).to_owned();
        if !seen.contains(&tag) {
            seen.push(tag);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn a_graph_keeps_edges_and_attachments_apart() {
        let graph: ScenarioGraphDto = parse(json!({
            "nodes": [],
            "edges": [{ "from": "a", "to": "b", "fromPort": "main", "toPort": "main" }],
            "attachments": [{ "parentNodeId": "b", "childNodeId": "m", "slot": "model" }],
        }))
        .expect("parses");
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.attachments[0].slot, ScenarioSlot::Model);
        assert!(
            parse::<ScenarioGraphDto>(json!({
                "nodes": [],
                "edges": [],
                "attachments": [{ "parentNodeId": "b", "childNodeId": "m", "slot": "brain" }],
            }))
            .is_err()
        );
    }

    #[test]
    fn a_validation_result_is_valid_exactly_when_it_has_no_issues() {
        assert!(parse::<ScenarioValidationDto>(json!({ "valid": true, "issues": [] })).is_ok());
        assert!(
            parse::<ScenarioValidationDto>(json!({
                "valid": true,
                "issues": [{ "rule": "noTrigger" }],
            }))
            .is_err()
        );
    }

    #[test]
    fn tags_are_trimmed_and_deduplicated_in_order() {
        let tags = [
            "  support ".to_owned(),
            "mail".to_owned(),
            "support".to_owned(),
        ];
        assert_eq!(normalise_tags(&tags), ["support", "mail"]);
    }
}
