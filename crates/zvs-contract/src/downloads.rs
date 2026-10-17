use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{DownloadId, RunId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(DownloadItemKind {
    Model => "model",
    Embedding => "embedding",
    Runtime => "runtime",
    Mcp => "mcp",
    Skill => "skill",
});

wire_enum!(DownloadStatus {
    Queued => "queued",
    Running => "running",
    Paused => "paused",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
});

impl DownloadStatus {
    pub const TERMINAL: &[Self] = &[Self::Succeeded, Self::Failed, Self::Cancelled];

    pub fn is_terminal(self) -> bool {
        Self::TERMINAL.contains(&self)
    }
}

wire_enum!(ChecksumAlgorithm {
    Sha256 => "sha256",
    Blake3 => "blake3",
});

wire_enum!(CatalogueSource {
    Curated => "curated",
    Ollama => "ollama",
    Github => "github",
});

wire_enum!(CatalogueItemState {
    Installed => "installed",
    Update => "update",
    Available => "available",
    Downloading => "downloading",
    Queued => "queued",
});

wire_enum!(DiskCategory {
    Models => "models",
    Embeddings => "embeddings",
    Runtimes => "runtimes",
    Mcp => "mcp",
    Skills => "skills",
    Vectors => "vectors",
    Other => "other",
});

pub const ITEM_REF_MIN: usize = 3;
pub const ITEM_REF_MAX: usize = 256;
pub const URL_MAX: usize = 2048;
pub const PRIORITY_MAX: u64 = 9;

pub fn check_item_ref(field: &str, value: &str) -> Result<(), AppError> {
    let trimmed = js_trim(value);
    if trimmed.encode_utf16().count() < ITEM_REF_MIN {
        return Err(invalid(field, "minLength"));
    }
    check::max_len(field, trimmed, ITEM_REF_MAX)
}

fn check_priority(field: &str, value: u64) -> Result<(), AppError> {
    check::int_range(field, value, 0, PRIORITY_MAX)
}

fn check_distinct_limit<T>(field: &str, values: &[T], max: usize) -> Result<(), AppError> {
    if values.len() > max {
        Err(invalid(field, "maxItems"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksumDto {
    pub algorithm: ChecksumAlgorithm,
    pub value: String,
}

impl ChecksumDto {
    pub fn value(&self) -> &str {
        js_trim(&self.value)
    }
}

impl Validate for ChecksumDto {
    fn validate(&self) -> Result<(), AppError> {
        let value = self.value();
        let hex = value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if hex && (32..=128).contains(&value.len()) {
            Ok(())
        } else {
            Err(invalid("value", "pattern"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueItemDto {
    #[serde(rename = "ref")]
    pub item_ref: String,
    pub kind: DownloadItemKind,
    pub source: CatalogueSource,
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub version: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub installed_version: Option<String>,
    pub size_bytes: u64,
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub checksum: Option<ChecksumDto>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub dimension: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub query_prefix: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub document_prefix: Option<String>,
    pub state: CatalogueItemState,
    pub downloadable: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub blocked_reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub download_id: Option<DownloadId>,
}

fn check_bounded(field: &str, value: &str, max: usize) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, max)
}

fn check_optional_len(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::max_len(field, value, max))
}

impl Validate for CatalogueItemDto {
    fn validate(&self) -> Result<(), AppError> {
        check_item_ref("ref", &self.item_ref)?;
        check_bounded("name", &self.name, 200)?;
        check_bounded("displayName", &self.display_name, 200)?;
        check::max_len("description", &self.description, 1000)?;
        check_optional_len("version", self.version.as_ref(), 64)?;
        check_optional_len("installedVersion", self.installed_version.as_ref(), 64)?;
        check::safe_integer("sizeBytes", self.size_bytes)?;
        check::url("url", &self.url, URL_MAX)?;
        nested("checksum", self.checksum.validate())?;
        check_distinct_limit("tags", &self.tags, 16)?;
        for (index, tag) in self.tags.iter().enumerate() {
            check::max_len(&format!("tags.{index}"), tag, 48)?;
        }
        if let Some(dimension) = self.dimension {
            check::safe_integer("dimension", dimension)?;
            check::int_range("dimension", dimension, 1, u64::MAX)?;
        }
        check_optional_len("queryPrefix", self.query_prefix.as_ref(), 512)?;
        check_optional_len("documentPrefix", self.document_prefix.as_ref(), 512)?;
        check_optional_len("blockedReason", self.blocked_reason.as_ref(), 400)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDto {
    pub id: DownloadId,
    pub item_kind: DownloadItemKind,
    pub item_ref: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub bytes_done: u64,
    pub status: DownloadStatus,
    pub priority: u64,
    pub target_path: String,
    pub url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub checksum: Option<ChecksumDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub version: Option<String>,
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
    pub run_id: Option<RunId>,
    #[serde(default)]
    pub rate_bytes_per_second: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub eta_ms: Option<u64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
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
}

impl Validate for DownloadDto {
    fn validate(&self) -> Result<(), AppError> {
        check_item_ref("itemRef", &self.item_ref)?;
        check_bounded("displayName", &self.display_name, 200)?;
        check::safe_integer("sizeBytes", self.size_bytes)?;
        check::safe_integer("bytesDone", self.bytes_done)?;
        check_priority("priority", self.priority)?;
        check_bounded("targetPath", &self.target_path, 4096)?;
        check::url("url", &self.url, URL_MAX)?;
        nested("checksum", self.checksum.validate())?;
        check_optional_len("version", self.version.as_ref(), 64)?;
        check_optional_len("error", self.error.as_ref(), 2000)?;
        check::number_range(
            "rateBytesPerSecond",
            self.rate_bytes_per_second,
            0.0,
            f64::MAX,
        )?;
        if let Some(eta) = self.eta_ms {
            check::safe_integer("etaMs", eta)?;
        }
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)?;
        if let Some(at) = self.started_at {
            check::timestamp("startedAt", at)?;
        }
        if let Some(at) = self.finished_at {
            check::timestamp("finishedAt", at)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskCategoryUsage {
    pub category: DiskCategory,
    pub bytes: u64,
}

impl Validate for DiskCategoryUsage {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("bytes", self.bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsageDto {
    pub root: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub categories: Vec<DiskCategoryUsage>,
    pub measured_at: Timestamp,
}

impl Validate for DiskUsageDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("root", &self.root)?;
        check::safe_integer("totalBytes", self.total_bytes)?;
        check::safe_integer("freeBytes", self.free_bytes)?;
        check::safe_integer("usedBytes", self.used_bytes)?;
        nested("categories", self.categories.validate())?;
        check::timestamp("measuredAt", self.measured_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRef {
    pub id: DownloadId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrioritiseDownloadInput {
    pub id: DownloadId,
    pub priority: u64,
}

impl Validate for PrioritiseDownloadInput {
    fn validate(&self) -> Result<(), AppError> {
        check_priority("priority", self.priority)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDownloadInput {
    #[serde(rename = "ref")]
    pub item_ref: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub priority: Option<u64>,
}

impl StartDownloadInput {
    pub fn new(item_ref: impl Into<String>) -> Self {
        Self {
            item_ref: item_ref.into(),
            priority: None,
        }
    }

    pub fn item_ref(&self) -> &str {
        js_trim(&self.item_ref)
    }
}

impl Validate for StartDownloadInput {
    fn validate(&self) -> Result<(), AppError> {
        check_item_ref("ref", &self.item_ref)?;
        self.priority
            .map_or(Ok(()), |priority| check_priority("priority", priority))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadListFilter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub statuses: Option<Vec<DownloadStatus>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub kinds: Option<Vec<DownloadItemKind>>,
}

impl Validate for DownloadListFilter {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(statuses) = &self.statuses {
            check_distinct_limit("statuses", statuses, DownloadStatus::ALL.len())?;
        }
        if let Some(kinds) = &self.kinds {
            check_distinct_limit("kinds", kinds, DownloadItemKind::ALL.len())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueFilter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub kinds: Option<Vec<DownloadItemKind>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub states: Option<Vec<CatalogueItemState>>,
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
    pub refresh: Option<bool>,
}

impl CatalogueFilter {
    pub fn refresh(&self) -> bool {
        self.refresh.unwrap_or(false)
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref().map(js_trim)
    }
}

impl Validate for CatalogueFilter {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(kinds) = &self.kinds {
            check_distinct_limit("kinds", kinds, DownloadItemKind::ALL.len())?;
        }
        if let Some(states) = &self.states {
            check_distinct_limit("states", states, CatalogueItemState::ALL.len())?;
        }
        self.query()
            .map_or(Ok(()), |query| check::max_len("query", query, 200))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsDiskInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub refresh: Option<bool>,
}

impl DownloadsDiskInput {
    pub fn refresh(&self) -> bool {
        self.refresh.unwrap_or(false)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFetchInput {
    pub download_id: DownloadId,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadReportDto {
    pub id: DownloadId,
    pub bytes: u64,
    pub resumed_from: u64,
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub checksum: Option<String>,
}

impl Validate for DownloadReportDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("bytes", self.bytes)?;
        check::safe_integer("resumedFrom", self.resumed_from)?;
        check::non_empty("path", &self.path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsRemoveOutput {
    pub id: DownloadId,
    pub removed: bool,
}

impl Validate for DownloadsRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

pub type DownloadsListOutput = Vec<DownloadDto>;
pub type DownloadsCatalogueOutput = Vec<CatalogueItemDto>;

no_refinements!(DownloadRef, DownloadsDiskInput, DownloadFetchInput);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const ID: &str = "0199c3a8-6b2f-7c1e-9a4d-1f2e3d4c5b6a";

    #[test]
    fn an_item_ref_is_trimmed_before_its_length_is_checked() {
        let input =
            parse::<StartDownloadInput>(json!({ "ref": "  curated:model:a  " })).expect("parses");
        assert_eq!(input.item_ref(), "curated:model:a");
        assert!(parse::<StartDownloadInput>(json!({ "ref": "  ab  " })).is_err());
        assert!(parse::<StartDownloadInput>(json!({ "ref": "abc", "priority": 10 })).is_err());
    }

    #[test]
    fn a_checksum_is_hex_between_32_and_128_digits() {
        let ok = |value: &str| {
            parse::<ChecksumDto>(json!({ "algorithm": "sha256", "value": value })).is_ok()
        };
        assert!(ok(&"a".repeat(64)));
        assert!(ok(&format!(" {} ", "F".repeat(32))));
        assert!(!ok(&"a".repeat(31)));
        assert!(!ok(&"g".repeat(64)));
        assert!(!ok(&"a".repeat(129)));
    }

    #[test]
    fn a_download_always_writes_its_rate() {
        let dto = parse::<DownloadDto>(json!({
            "id": ID,
            "itemKind": "model",
            "itemRef": "curated:model:a",
            "displayName": "A",
            "sizeBytes": 10,
            "bytesDone": 0,
            "status": "queued",
            "priority": 5,
            "targetPath": "C:/a.bin",
            "url": "https://example.test/a",
            "createdAt": 1,
            "updatedAt": 1,
        }))
        .expect("parses");
        assert_eq!(
            serde_json::to_value(&dto).expect("serialises")["rateBytesPerSecond"],
            json!(0.0)
        );
        assert!(DownloadStatus::Cancelled.is_terminal());
        assert!(!DownloadStatus::Paused.is_terminal());
    }

    #[test]
    fn the_refresh_flags_default_to_false() {
        assert!(
            !parse::<CatalogueFilter>(json!({}))
                .expect("parses")
                .refresh()
        );
        assert!(
            !parse::<DownloadsDiskInput>(json!({}))
                .expect("parses")
                .refresh()
        );
        assert!(parse::<CatalogueFilter>(json!({ "query": "x".repeat(201) })).is_err());
    }
}
