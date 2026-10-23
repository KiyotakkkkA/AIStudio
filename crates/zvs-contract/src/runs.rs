use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::events::{HostEvent, LogLineDto, RunOutcomeDto};
use crate::ids::{RunId, StepId, StreamId, is_wire_uuid};
use crate::primitives::{Json, MAX_SAFE_INTEGER, Timestamp};
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

pub const GRAPH_MAX_NODES: usize = 10_000;
pub const GRAPH_ALLOWED_TOOLS_MAX: usize = 256;
pub const DEFAULT_CONCURRENCY: u64 = 4;
pub const DEFAULT_RUN_LIST_LIMIT: u64 = 50;

wire_enum!(RunStatus {
    Queued => "queued",
    Running => "running",
    Blocked => "blocked",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
    Interrupted => "interrupted",
});

impl RunStatus {
    pub const UNFINISHED: &[Self] = &[Self::Queued, Self::Running, Self::Blocked];
    pub const FINISHED: &[Self] = &[
        Self::Succeeded,
        Self::Failed,
        Self::Cancelled,
        Self::Interrupted,
    ];
    pub const ATTENTION: &[Self] = &[Self::Failed, Self::Interrupted];

    pub fn is_unfinished(self) -> bool {
        Self::UNFINISHED.contains(&self)
    }
}

wire_enum!(RunKind {
    Chat => "chat",
    Scenario => "scenario",
    Agentic => "agentic",
    Job => "job",
    Indexing => "indexing",
    Download => "download",
    Browser => "browser",
    Skill => "skill",
});

wire_enum!(StepStatus {
    Running => "running",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
    Abandoned => "abandoned",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "source",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum GraphBinding {
    Run {},
    Node {
        node_id: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::wire::present"
        )]
        path: Option<String>,
    },
}

impl GraphBinding {
    pub fn node(node_id: impl Into<String>) -> Self {
        Self::Node {
            node_id: node_id.into(),
            path: None,
        }
    }

    pub fn node_path(node_id: impl Into<String>, path: impl Into<String>) -> Self {
        Self::Node {
            node_id: node_id.into(),
            path: Some(path.into()),
        }
    }
}

pub const GRAPH_TIMEOUT_MAX_MS: u64 = 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    pub max_attempts: u64,
    pub backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff_ms: 0,
        }
    }
}

impl Validate for RetryPolicy {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("maxAttempts", self.max_attempts, 1, 100)?;
        check::int_range("backoffMs", self.backoff_ms, 0, 60_000)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub dependencies: Option<Vec<String>>,
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
    pub bindings: Option<BTreeMap<String, GraphBinding>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub retry: Option<RetryPolicy>,
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
    pub continue_on_error: Option<bool>,
}

impl GraphNode {
    pub fn new(id: impl Into<String>, node_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            node_type: node_type.into(),
            dependencies: None,
            input: None,
            bindings: None,
            retry: None,
            timeout_ms: None,
            continue_on_error: None,
        }
    }

    pub fn continues_on_error(&self) -> bool {
        self.continue_on_error.unwrap_or(false)
    }

    pub fn dependencies(&self) -> &[String] {
        self.dependencies.as_deref().unwrap_or_default()
    }

    pub fn input(&self) -> &Json {
        static NULL: Json = Json::Null;
        self.input.as_ref().unwrap_or(&NULL)
    }

    pub fn bindings(&self) -> impl Iterator<Item = (&String, &GraphBinding)> {
        self.bindings.iter().flatten()
    }

    pub fn has_bindings(&self) -> bool {
        self.bindings
            .as_ref()
            .is_some_and(|bindings| !bindings.is_empty())
    }

    pub fn retry(&self) -> RetryPolicy {
        self.retry.unwrap_or_default()
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.dependencies.get_or_insert_with(Vec::new);
        self.input.get_or_insert(Json::Null);
        self.bindings.get_or_insert_with(BTreeMap::new);
        self.retry.get_or_insert_with(RetryPolicy::default);
        self
    }
}

impl Validate for GraphNode {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, 128)?;
        check::non_empty("type", &self.node_type)?;
        check::max_len("type", &self.node_type, 128)?;
        if let Some(timeout) = self.timeout_ms {
            check::int_range("timeoutMs", timeout, 1, GRAPH_TIMEOUT_MAX_MS)?;
        }
        nested("retry", self.retry.validate())
    }
}

pub fn is_permission_scope(value: &str) -> bool {
    ["skill:", "scenario:", "site:"].iter().any(|prefix| {
        value.strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty()
                && !rest
                    .chars()
                    .any(|character| matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        })
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunGraph {
    pub nodes: Vec<GraphNode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub permission_scopes: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub allowed_tools: Option<Vec<String>>,
}

impl RunGraph {
    pub const fn new(nodes: Vec<GraphNode>) -> Self {
        Self {
            nodes,
            permission_scopes: None,
            allowed_tools: None,
        }
    }

    pub fn permission_scopes(&self) -> &[String] {
        self.permission_scopes.as_deref().unwrap_or_default()
    }

    pub fn allows_tool(&self, key: &str) -> bool {
        self.allowed_tools
            .as_ref()
            .is_none_or(|allowed| allowed.iter().any(|tool| tool == key))
    }

    #[must_use]
    pub fn normalized(self) -> Self {
        Self {
            nodes: self.nodes.into_iter().map(GraphNode::normalized).collect(),
            permission_scopes: self.permission_scopes,
            allowed_tools: self.allowed_tools,
        }
    }
}

impl Validate for RunGraph {
    fn validate(&self) -> Result<(), AppError> {
        if self.nodes.len() > GRAPH_MAX_NODES {
            return Err(invalid("nodes", "maxLength"));
        }
        nested("nodes", self.nodes.validate())?;
        for (index, scope) in self.permission_scopes().iter().enumerate() {
            if !is_permission_scope(scope) {
                return Err(invalid(&format!("permissionScopes.{index}"), "pattern"));
            }
        }
        if let Some(allowed) = &self.allowed_tools {
            if allowed.len() > GRAPH_ALLOWED_TOOLS_MAX {
                return Err(invalid("allowedTools", "maxLength"));
            }
            for (index, key) in allowed.iter().enumerate() {
                check::max_len(&format!("allowedTools.{index}"), key, 128)?;
                if !crate::tools::is_tool_key(key) {
                    return Err(invalid(&format!("allowedTools.{index}"), "pattern"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRunInput {
    pub kind: RunKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub subject_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub title: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub retry_of_id: Option<RunId>,
    pub graph: RunGraph,
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
    pub concurrency: Option<u64>,
}

impl StartRunInput {
    pub const fn new(kind: RunKind, graph: RunGraph) -> Self {
        Self {
            kind,
            subject_id: None,
            title: None,
            retry_of_id: None,
            graph,
            input: None,
            concurrency: None,
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref().map(js_trim)
    }

    pub fn input(&self) -> &Json {
        static NULL: Json = Json::Null;
        self.input.as_ref().unwrap_or(&NULL)
    }

    pub fn concurrency(&self) -> u64 {
        self.concurrency.unwrap_or(DEFAULT_CONCURRENCY)
    }
}

impl Validate for StartRunInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(subject) = &self.subject_id {
            check::non_empty("subjectId", subject)?;
        }
        if let Some(title) = &self.title {
            check::trimmed_name("title", title, 200)?;
        }
        nested("graph", self.graph.validate())?;
        check::int_range("concurrency", self.concurrency(), 1, 64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunIdInput {
    pub id: RunId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHandleDto {
    pub id: RunId,
    pub stream_id: StreamId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsApproveInput {
    pub id: RunId,
    pub approval_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub always: Option<bool>,
}

impl RunsApproveInput {
    pub fn always(&self) -> bool {
        self.always.unwrap_or(false)
    }
}

impl Validate for RunsApproveInput {
    fn validate(&self) -> Result<(), AppError> {
        check_uuid("approvalId", &self.approval_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsDenyInput {
    pub id: RunId,
    pub approval_id: String,
}

impl Validate for RunsDenyInput {
    fn validate(&self) -> Result<(), AppError> {
        check_uuid("approvalId", &self.approval_id)
    }
}

fn check_uuid(field: &str, value: &str) -> Result<(), AppError> {
    if is_wire_uuid(value) {
        Ok(())
    } else {
        Err(invalid(field, "uuid"))
    }
}

fn check_int(field: &str, value: i64) -> Result<(), AppError> {
    if (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) {
        Ok(())
    } else {
        Err(invalid(field, "safeInteger"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDto {
    pub id: RunId,
    pub stream_id: StreamId,
    pub kind: RunKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub subject_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub title: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub retry_of_id: Option<RunId>,
    pub status: RunStatus,
    pub graph: RunGraph,
    pub input: Json,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub outcome: Option<RunOutcomeDto>,
    pub concurrency: u64,
    pub created_at: i64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub started_at: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub finished_at: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub pruned_at: Option<i64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
}

impl Validate for RunDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("graph", self.graph.validate())?;
        check::safe_integer("concurrency", self.concurrency)?;
        check::int_range("concurrency", self.concurrency, 1, u64::MAX)?;
        check_int("createdAt", self.created_at)?;
        for (field, value) in [
            ("startedAt", self.started_at),
            ("finishedAt", self.finished_at),
            ("prunedAt", self.pruned_at),
        ] {
            if let Some(value) = value {
                check_int(field, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepDto {
    pub id: StepId,
    pub run_id: RunId,
    pub node_id: String,
    #[serde(rename = "type")]
    pub step_type: String,
    pub status: StepStatus,
    pub input: Json,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub output: Option<Json>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
    pub started_at: i64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub finished_at: Option<i64>,
    pub attempt: u64,
}

impl Validate for StepDto {
    fn validate(&self) -> Result<(), AppError> {
        check_int("startedAt", self.started_at)?;
        if let Some(finished) = self.finished_at {
            check_int("finishedAt", finished)?;
        }
        check::safe_integer("attempt", self.attempt)?;
        check::int_range("attempt", self.attempt, 1, u64::MAX)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunListFilter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub statuses: Option<Vec<RunStatus>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub kinds: Option<Vec<RunKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub subject_id: Option<String>,
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
    pub from: Option<Timestamp>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub to: Option<Timestamp>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub live: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub limit: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub cursor: Option<String>,
}

impl RunListFilter {
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref().map(js_trim)
    }

    pub fn live(&self) -> bool {
        self.live.unwrap_or(false)
    }

    pub fn limit(&self) -> u64 {
        self.limit.unwrap_or(DEFAULT_RUN_LIST_LIMIT)
    }
}

impl Validate for RunListFilter {
    fn validate(&self) -> Result<(), AppError> {
        if self
            .statuses
            .as_ref()
            .is_some_and(|statuses| statuses.len() > RunStatus::ALL.len())
        {
            return Err(invalid("statuses", "maxLength"));
        }
        if self
            .kinds
            .as_ref()
            .is_some_and(|kinds| kinds.len() > RunKind::ALL.len())
        {
            return Err(invalid("kinds", "maxLength"));
        }
        if let Some(subject_id) = &self.subject_id {
            check::non_empty("subjectId", subject_id)?;
            check::max_len("subjectId", subject_id, 128)?;
        }
        if let Some(query) = self.query() {
            check::max_len("query", query, 200)?;
        }
        if let Some(from) = self.from {
            check::timestamp("from", from)?;
        }
        if let Some(to) = self.to {
            check::timestamp("to", to)?;
        }
        check::int_range("limit", self.limit(), 1, 200)?;
        if let Some(cursor) = &self.cursor {
            check::non_empty("cursor", cursor)?;
            check::max_len("cursor", cursor, 128)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCountsDto {
    pub total: u64,
    pub by_status: BTreeMap<RunStatus, u64>,
    pub by_kind: BTreeMap<RunKind, u64>,
}

impl Validate for RunCountsDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("total", self.total)?;
        if self.by_status.len() != RunStatus::ALL.len() {
            return Err(invalid("byStatus", "exhaustive"));
        }
        if self.by_kind.len() != RunKind::ALL.len() {
            return Err(invalid("byKind", "exhaustive"));
        }
        for (status, total) in &self.by_status {
            check::safe_integer(&format!("byStatus.{status}"), *total)?;
        }
        for (kind, total) in &self.by_kind {
            check::safe_integer(&format!("byKind.{kind}"), *total)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProgressDto {
    pub done: u64,
    pub total: u64,
}

impl Validate for RunProgressDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("done", self.done)?;
        check::safe_integer("total", self.total)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunApprovalDto {
    pub id: String,
    pub subject: String,
    pub scope: String,
    pub expires_at: Timestamp,
}

impl Validate for RunApprovalDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::non_empty("subject", &self.subject)?;
        check::non_empty("scope", &self.scope)?;
        check::timestamp("expiresAt", self.expires_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummaryDto {
    pub id: RunId,
    pub stream_id: StreamId,
    pub kind: RunKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub subject_id: Option<String>,
    pub title: String,
    pub status: RunStatus,
    pub progress: RunProgressDto,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub active_node_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub approval: Option<RunApprovalDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub outcome: Option<RunOutcomeDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub retry_of_id: Option<RunId>,
    pub created_at: Timestamp,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub started_at: Option<Timestamp>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub finished_at: Option<Timestamp>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub pruned_at: Option<Timestamp>,
}

impl Validate for RunSummaryDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("progress", self.progress.validate())?;
        nested("approval", self.approval.validate())?;
        check::timestamp("createdAt", self.created_at)?;
        for (field, value) in [
            ("startedAt", self.started_at),
            ("finishedAt", self.finished_at),
            ("prunedAt", self.pruned_at),
        ] {
            if let Some(value) = value {
                check::timestamp(field, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPageDto {
    pub items: Vec<RunSummaryDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub next_cursor: Option<String>,
    pub counts: RunCountsDto,
}

impl Validate for RunPageDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("items", self.items.validate())?;
        if let Some(cursor) = &self.next_cursor {
            check::non_empty("nextCursor", cursor)?;
        }
        nested("counts", self.counts.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDetailDto {
    pub run: RunDto,
    pub summary: RunSummaryDto,
    pub steps: Vec<StepDto>,
    pub logs: Vec<LogLineDto>,
    #[serde(default)]
    pub events: Vec<HostEvent>,
}

impl Validate for RunDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("run", self.run.validate())?;
        nested("summary", self.summary.validate())?;
        nested("steps", self.steps.validate())?;
        nested("events", self.events.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsClearFinishedOutput {
    pub removed: u64,
}

impl Validate for RunsClearFinishedOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("removed", self.removed)
    }
}

pub type RunsStepsOutput = Vec<StepDto>;

pub fn log_line(entries: &[(&str, Value)]) -> LogLineDto {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect::<Map<String, Value>>()
}

no_refinements!(GraphBinding, RunIdInput, RunHandleDto);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_minimal_node_round_trips_and_normalises_to_the_zod_defaults() {
        let node: GraphNode =
            serde_json::from_value(json!({ "id": "a", "type": "work" })).expect("parses");
        assert_eq!(
            serde_json::to_value(&node).expect("serialises"),
            json!({ "id": "a", "type": "work" })
        );
        assert_eq!(
            serde_json::to_value(node.normalized()).expect("serialises"),
            json!({
                "id": "a",
                "type": "work",
                "dependencies": [],
                "input": null,
                "bindings": {},
                "retry": { "maxAttempts": 1, "backoffMs": 0 }
            })
        );
    }

    #[test]
    fn bindings_are_tagged_on_their_source() {
        let node: GraphNode = serde_json::from_value(json!({
            "id": "d",
            "type": "d",
            "input": {},
            "bindings": {
                "original": { "source": "run" },
                "left": { "source": "node", "nodeId": "b" }
            }
        }))
        .expect("parses");
        let bindings: Vec<_> = node.bindings().collect();
        assert_eq!(
            bindings,
            [
                (&"left".to_owned(), &GraphBinding::node("b")),
                (&"original".to_owned(), &GraphBinding::Run {}),
            ]
        );
        assert!(serde_json::from_value::<GraphBinding>(json!({ "source": "node" })).is_err());
    }

    #[test]
    fn permission_scopes_follow_the_zod_pattern() {
        for scope in ["skill:a", "scenario:42", "site:https://example.com"] {
            assert!(is_permission_scope(scope), "{scope}");
        }
        for scope in ["global", "skill:", "tool:a", "site:a\nb", " skill:a"] {
            assert!(!is_permission_scope(scope), "{scope}");
        }
    }

    #[test]
    fn start_input_defaults_apply_through_accessors() {
        let input: StartRunInput = serde_json::from_value(json!({
            "kind": "job",
            "title": "  Задание  ",
            "graph": { "nodes": [] }
        }))
        .expect("parses");
        assert_eq!(input.concurrency(), 4);
        assert_eq!(input.input(), &Json::Null);
        assert_eq!(input.title(), Some("Задание"));
        assert!(input.validate().is_ok());
        let crowded = StartRunInput {
            concurrency: Some(65),
            ..input
        };
        assert!(crowded.validate().is_err());
    }

    #[test]
    fn counts_must_name_every_status_and_kind() {
        let counts = RunCountsDto {
            total: 0,
            by_status: RunStatus::ALL.iter().map(|status| (*status, 0)).collect(),
            by_kind: RunKind::ALL.iter().map(|kind| (*kind, 0)).collect(),
        };
        assert!(counts.validate().is_ok());
        let mut partial = counts;
        partial.by_kind.remove(&RunKind::Browser);
        assert!(partial.validate().is_err());
    }
}
