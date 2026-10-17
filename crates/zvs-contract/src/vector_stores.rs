use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{DocumentId, ProviderId, VectorEvalCaseId, VectorStoreId};
use crate::primitives::{Json, Timestamp};
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(VectorBackend {
    Lancedb => "lancedb",
    Qdrant => "qdrant",
    Pgvector => "pgvector",
});

wire_enum!(VectorMetric {
    Cosine => "cosine",
    L2 => "l2",
    Dot => "dot",
});

wire_enum!(VectorHealth {
    Healthy => "healthy",
    Stale => "stale",
    Pending => "pending",
    Broken => "broken",
});

wire_enum!(VectorSearchMode {
    Hybrid => "hybrid",
    Vector => "vector",
});

wire_enum!(VectorOcrLanguage {
    Auto => "auto",
    Rus => "rus",
    Eng => "eng",
    RusEng => "rus+eng",
});

wire_enum!(VectorSourceKind {
    File => "file",
    Folder => "folder",
});

wire_enum!(VectorHitProvenance {
    Vector => "vector",
    Keyword => "keyword",
    Both => "both",
});

wire_enum!(VectorRerankOutcome {
    Off => "off",
    Applied => "applied",
    Fallback => "fallback",
});

wire_enum!(VectorEvalMode {
    Vector => "vector",
    Rerank => "rerank",
    Hybrid => "hybrid",
    HybridRerank => "hybrid-rerank",
});

pub const VECTOR_SCHEMA_VERSION: u64 = 2;
pub const DEFAULT_CHAT_MIN_SCORE: f64 = 0.3;
pub const DEFAULT_RERANK_CANDIDATES: u64 = 50;
pub const DEFAULT_OCR_MIN_CHARS: u64 = 200;
pub const DEFAULT_DIMENSION: u64 = 1024;
pub const DEFAULT_CHUNK_SIZE: u64 = 256;
pub const DEFAULT_CHUNK_OVERLAP: u64 = 32;
pub const DEFAULT_SEARCH_K: u64 = 10;
pub const DEFAULT_EVAL_K: u64 = 10;
pub const FLAT_INDEX: &str = "FLAT";
pub const STORE_NAME_MAX: usize = 128;
pub const DESCRIPTION_MAX: usize = 4096;
pub const MODEL_ID_MAX: usize = 256;
pub const MODEL_REF_MAX: usize = 256;
pub const PREFIX_MAX: usize = 512;
pub const QUERY_MAX: usize = 100_000;
pub const SOURCE_PATH_MAX: usize = 4096;
pub const PATTERN_MAX: usize = 256;
pub const PATTERNS_MAX: usize = 64;
pub const CHUNK_INDICES_MAX: usize = 200;
pub const QUESTION_MAX: usize = 2000;
pub const EXPECTED_DOCUMENTS_MAX: usize = 20;

pub fn below_relevance_floor(
    score: f64,
    provenance: Option<VectorHitProvenance>,
    floor: f64,
) -> bool {
    score < floor && provenance != Some(VectorHitProvenance::Keyword)
}

const fn default_search_mode() -> VectorSearchMode {
    VectorSearchMode::Hybrid
}

const fn default_chat_min_score() -> f64 {
    DEFAULT_CHAT_MIN_SCORE
}

const fn default_schema_version() -> u64 {
    1
}

fn check_model_ref(field: &str, value: &str) -> Result<(), AppError> {
    check::max_len(field, js_trim(value), MODEL_REF_MAX)
}

fn check_ratio(field: &str, value: f64) -> Result<(), AppError> {
    check::number_range(field, value, 0.0, 1.0)
}

fn check_non_negative(field: &str, value: f64) -> Result<(), AppError> {
    check::number_range(field, value, 0.0, f64::MAX)
}

fn check_dimension(value: u64) -> Result<(), AppError> {
    check::int_range("dimension", value, 1, 65_536)
}

fn check_chunk_size(value: u64) -> Result<(), AppError> {
    check::int_range("chunkSize", value, 1, 1_000_000)
}

fn check_prefix(field: &str, value: &str) -> Result<(), AppError> {
    check::max_len(field, value, PREFIX_MAX)
}

fn check_patterns(field: &str, patterns: &[String], max: Option<usize>) -> Result<(), AppError> {
    if max.is_some_and(|max| patterns.len() > max) {
        return Err(invalid(field, "maxItems"));
    }
    patterns
        .iter()
        .enumerate()
        .try_for_each(|(index, pattern)| {
            check::trimmed_name(&format!("{field}.{index}"), pattern, PATTERN_MAX)
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VectorRerankConfig {
    pub enabled: bool,
    pub model_ref: String,
    pub candidates: u64,
}

impl Default for VectorRerankConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_ref: String::new(),
            candidates: DEFAULT_RERANK_CANDIDATES,
        }
    }
}

impl Validate for VectorRerankConfig {
    fn validate(&self) -> Result<(), AppError> {
        check_model_ref("modelRef", &self.model_ref)?;
        check::int_range("candidates", self.candidates, 1, 500)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VectorOcrConfig {
    pub enabled: bool,
    pub model_ref: String,
    pub language: VectorOcrLanguage,
    pub min_chars_per_page: u64,
}

impl Default for VectorOcrConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_ref: String::new(),
            language: VectorOcrLanguage::Auto,
            min_chars_per_page: DEFAULT_OCR_MIN_CHARS,
        }
    }
}

impl Validate for VectorOcrConfig {
    fn validate(&self) -> Result<(), AppError> {
        check_model_ref("modelRef", &self.model_ref)?;
        check::int_range("minCharsPerPage", self.min_chars_per_page, 0, 100_000)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoreDto {
    pub id: VectorStoreId,
    pub name: String,
    pub description: String,
    pub backend: VectorBackend,
    pub embedding_provider_id: ProviderId,
    pub embedding_model_id: String,
    pub dimension: u64,
    pub metric: VectorMetric,
    pub chunk_size: u64,
    pub chunk_overlap: u64,
    #[serde(default)]
    pub rerank: VectorRerankConfig,
    #[serde(default)]
    pub ocr: VectorOcrConfig,
    #[serde(default)]
    pub query_prefix: String,
    #[serde(default)]
    pub document_prefix: String,
    #[serde(default = "default_search_mode")]
    pub search_mode: VectorSearchMode,
    #[serde(default = "default_chat_min_score")]
    pub chat_min_score: f64,
    #[serde(default = "default_schema_version")]
    pub schema_version: u64,
    pub index_type: String,
    pub status: VectorHealth,
    #[serde(default)]
    pub graph_stale: bool,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_indexed_at: Option<Timestamp>,
    pub documents: u64,
    pub vectors: u64,
    pub bytes: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for VectorStoreDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, STORE_NAME_MAX)?;
        check::max_len("description", &self.description, DESCRIPTION_MAX)?;
        check::trimmed_name("embeddingModelId", &self.embedding_model_id, MODEL_ID_MAX)?;
        check_dimension(self.dimension)?;
        check_chunk_size(self.chunk_size)?;
        check::safe_integer("chunkOverlap", self.chunk_overlap)?;
        nested("rerank", self.rerank.validate())?;
        nested("ocr", self.ocr.validate())?;
        check_prefix("queryPrefix", &self.query_prefix)?;
        check_prefix("documentPrefix", &self.document_prefix)?;
        check_ratio("chatMinScore", self.chat_min_score)?;
        check::safe_integer("schemaVersion", self.schema_version)?;
        check::int_range("schemaVersion", self.schema_version, 1, u64::MAX)?;
        check::non_empty("indexType", &self.index_type)?;
        if let Some(at) = self.last_indexed_at {
            check::timestamp("lastIndexedAt", at)?;
        }
        check::safe_integer("documents", self.documents)?;
        check::safe_integer("vectors", self.vectors)?;
        check::safe_integer("bytes", self.bytes)?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

pub type VectorStoresListOutput = Vec<VectorStoreDto>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoreRef {
    pub id: VectorStoreId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVectorStoreInput {
    pub name: String,
    pub embedding_provider_id: ProviderId,
    pub embedding_model_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub backend: Option<VectorBackend>,
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
    pub metric: Option<VectorMetric>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub chunk_size: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub chunk_overlap: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub rerank: Option<VectorRerankConfig>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub ocr: Option<VectorOcrConfig>,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub search_mode: Option<VectorSearchMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub chat_min_score: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub index_type: Option<String>,
}

impl CreateVectorStoreInput {
    pub fn new(name: impl Into<String>, provider: ProviderId, model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            embedding_provider_id: provider,
            embedding_model_id: model.into(),
            description: None,
            backend: None,
            dimension: None,
            metric: None,
            chunk_size: None,
            chunk_overlap: None,
            rerank: None,
            ocr: None,
            query_prefix: None,
            document_prefix: None,
            search_mode: None,
            chat_min_score: None,
            index_type: None,
        }
    }

    pub fn description(&self) -> &str {
        self.description.as_deref().unwrap_or("")
    }

    pub fn backend(&self) -> VectorBackend {
        self.backend.unwrap_or(VectorBackend::Lancedb)
    }

    pub fn dimension(&self) -> u64 {
        self.dimension.unwrap_or(DEFAULT_DIMENSION)
    }

    pub fn metric(&self) -> VectorMetric {
        self.metric.unwrap_or(VectorMetric::Cosine)
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE)
    }

    pub fn chunk_overlap(&self) -> u64 {
        self.chunk_overlap.unwrap_or(DEFAULT_CHUNK_OVERLAP)
    }

    pub fn rerank(&self) -> VectorRerankConfig {
        self.rerank.clone().unwrap_or_default()
    }

    pub fn ocr(&self) -> VectorOcrConfig {
        self.ocr.clone().unwrap_or_default()
    }

    pub fn query_prefix(&self) -> &str {
        self.query_prefix.as_deref().unwrap_or("")
    }

    pub fn document_prefix(&self) -> &str {
        self.document_prefix.as_deref().unwrap_or("")
    }

    pub fn search_mode(&self) -> VectorSearchMode {
        self.search_mode.unwrap_or(VectorSearchMode::Hybrid)
    }

    pub fn chat_min_score(&self) -> f64 {
        self.chat_min_score.unwrap_or(DEFAULT_CHAT_MIN_SCORE)
    }

    pub fn index_type(&self) -> &str {
        self.index_type.as_deref().unwrap_or(FLAT_INDEX)
    }
}

impl Validate for CreateVectorStoreInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, STORE_NAME_MAX)?;
        check::trimmed_name("embeddingModelId", &self.embedding_model_id, MODEL_ID_MAX)?;
        check::max_len("description", self.description(), DESCRIPTION_MAX)?;
        check_dimension(self.dimension())?;
        check_chunk_size(self.chunk_size())?;
        check::safe_integer("chunkOverlap", self.chunk_overlap())?;
        nested("rerank", self.rerank.validate())?;
        nested("ocr", self.ocr.validate())?;
        check_prefix("queryPrefix", self.query_prefix())?;
        check_prefix("documentPrefix", self.document_prefix())?;
        check_ratio("chatMinScore", self.chat_min_score())?;
        if self.index_type() != FLAT_INDEX {
            return Err(invalid("indexType", "literal"));
        }
        if self.chunk_overlap() >= self.chunk_size() {
            return Err(invalid("chunkOverlap", "smallerThanChunkSize"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateVectorStoreInput {
    pub id: VectorStoreId,
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
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub chunk_size: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub chunk_overlap: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub rerank: Option<VectorRerankConfig>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub ocr: Option<VectorOcrConfig>,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub search_mode: Option<VectorSearchMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub chat_min_score: Option<f64>,
}

impl UpdateVectorStoreInput {
    pub const fn new(id: VectorStoreId) -> Self {
        Self {
            id,
            name: None,
            description: None,
            chunk_size: None,
            chunk_overlap: None,
            rerank: None,
            ocr: None,
            query_prefix: None,
            document_prefix: None,
            search_mode: None,
            chat_min_score: None,
        }
    }
}

impl Validate for UpdateVectorStoreInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check::trimmed_name("name", name, STORE_NAME_MAX)?;
        }
        if let Some(description) = &self.description {
            check::max_len("description", description, DESCRIPTION_MAX)?;
        }
        if let Some(size) = self.chunk_size {
            check_chunk_size(size)?;
        }
        if let Some(overlap) = self.chunk_overlap {
            check::safe_integer("chunkOverlap", overlap)?;
        }
        nested("rerank", self.rerank.validate())?;
        nested("ocr", self.ocr.validate())?;
        if let Some(prefix) = &self.query_prefix {
            check_prefix("queryPrefix", prefix)?;
        }
        if let Some(prefix) = &self.document_prefix {
            check_prefix("documentPrefix", prefix)?;
        }
        if let Some(score) = self.chat_min_score {
            check_ratio("chatMinScore", score)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoresRemoveOutput {
    pub id: VectorStoreId,
    pub removed: bool,
}

impl Validate for VectorStoresRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchInput {
    pub store_id: VectorStoreId,
    pub query: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub k: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub min_score: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub mode: Option<VectorSearchMode>,
}

impl VectorSearchInput {
    pub fn new(store_id: VectorStoreId, query: impl Into<String>) -> Self {
        Self {
            store_id,
            query: query.into(),
            k: None,
            min_score: None,
            mode: None,
        }
    }

    pub fn k(&self) -> u64 {
        self.k.unwrap_or(DEFAULT_SEARCH_K)
    }

    pub fn min_score(&self) -> f64 {
        self.min_score.unwrap_or(0.0)
    }
}

impl Validate for VectorSearchInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("query", &self.query, QUERY_MAX)?;
        check::int_range("k", self.k(), 1, 1000)?;
        check_ratio("minScore", self.min_score())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchHitDto {
    pub id: String,
    pub score: f64,
    pub payload: Json,
    pub document_id: String,
    pub chunk_index: u64,
    pub path: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub page: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub vector_score: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub rerank_score: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub keyword_score: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub fused_score: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub provenance: Option<VectorHitProvenance>,
}

fn check_page(page: Option<u64>) -> Result<(), AppError> {
    page.map_or(Ok(()), |page| {
        check::safe_integer("page", page)?;
        check::int_range("page", page, 1, u64::MAX)
    })
}

impl Validate for VectorSearchHitDto {
    fn validate(&self) -> Result<(), AppError> {
        check_ratio("score", self.score)?;
        check::safe_integer("chunkIndex", self.chunk_index)?;
        check_page(self.page)?;
        if let Some(score) = self.vector_score {
            check_ratio("vectorScore", score)?;
        }
        if let Some(score) = self.rerank_score {
            check_ratio("rerankScore", score)?;
        }
        if let Some(score) = self.keyword_score {
            check_non_negative("keywordScore", score)?;
        }
        if let Some(score) = self.fused_score {
            check_non_negative("fusedScore", score)?;
        }
        Ok(())
    }
}

pub type VectorStoresSearchOutput = Vec<VectorSearchHitDto>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResultDto {
    pub hits: Vec<VectorSearchHitDto>,
    pub embedding_ms: f64,
    pub search_ms: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub rerank_ms: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub rerank: Option<VectorRerankOutcome>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub mode: Option<VectorSearchMode>,
}

impl Validate for VectorSearchResultDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("hits", self.hits.validate())?;
        check_non_negative("embeddingMs", self.embedding_ms)?;
        check_non_negative("searchMs", self.search_ms)?;
        if let Some(ms) = self.rerank_ms {
            check_non_negative("rerankMs", ms)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorChunksInput {
    pub store_id: VectorStoreId,
    pub document_id: String,
    pub indices: Vec<u64>,
}

impl Validate for VectorChunksInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("documentId", &self.document_id)?;
        if self.indices.len() > CHUNK_INDICES_MAX {
            return Err(invalid("indices", "maxItems"));
        }
        self.indices
            .iter()
            .enumerate()
            .try_for_each(|(index, value)| check::safe_integer(&format!("indices.{index}"), *value))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorChunkDto {
    pub id: String,
    pub payload: Json,
    pub document_id: String,
    pub chunk_index: u64,
    pub path: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub page: Option<u64>,
}

impl Validate for VectorChunkDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("chunkIndex", self.chunk_index)?;
        check_page(self.page)
    }
}

pub type VectorStoresChunksOutput = Vec<VectorChunkDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSourceDto {
    pub id: String,
    pub store_id: VectorStoreId,
    pub kind: VectorSourceKind,
    pub path: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub recursive: bool,
    pub created_at: Timestamp,
}

impl Validate for VectorSourceDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::trimmed_name("path", &self.path, SOURCE_PATH_MAX)?;
        check_patterns("include", &self.include, None)?;
        check_patterns("exclude", &self.exclude, None)?;
        check::timestamp("createdAt", self.created_at)
    }
}

pub type VectorSourcesListOutput = Vec<VectorSourceDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSourceRef {
    pub id: String,
}

impl Validate for VectorSourceRef {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorSourcesRemoveOutput {
    pub id: String,
    pub removed: bool,
}

impl Validate for VectorSourcesRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::literal_true("removed", self.removed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddVectorSourceInput {
    pub store_id: VectorStoreId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub kind: Option<VectorSourceKind>,
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub include: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub exclude: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub recursive: Option<bool>,
}

impl AddVectorSourceInput {
    pub fn new(store_id: VectorStoreId, path: impl Into<String>) -> Self {
        Self {
            store_id,
            kind: None,
            path: path.into(),
            include: None,
            exclude: None,
            recursive: None,
        }
    }

    pub fn kind(&self) -> VectorSourceKind {
        self.kind.unwrap_or(VectorSourceKind::Folder)
    }

    pub fn include(&self) -> &[String] {
        self.include.as_deref().unwrap_or(&[])
    }

    pub fn exclude(&self) -> &[String] {
        self.exclude.as_deref().unwrap_or(&[])
    }

    pub fn recursive(&self) -> bool {
        self.recursive.unwrap_or(true)
    }
}

impl Validate for AddVectorSourceInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("path", &self.path, SOURCE_PATH_MAX)?;
        check_patterns("include", self.include(), Some(PATTERNS_MAX))?;
        check_patterns("exclude", self.exclude(), Some(PATTERNS_MAX))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickVectorSourceInput {
    pub kind: VectorSourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickVectorSourceResult {
    pub paths: Vec<String>,
}

impl Validate for PickVectorSourceResult {
    fn validate(&self) -> Result<(), AppError> {
        self.paths
            .iter()
            .enumerate()
            .try_for_each(|(index, path)| check::non_empty(&format!("paths.{index}"), path))
    }
}

pub const INDEX_NOTES_MAX: usize = 200;

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorIndexInput {
    pub store_id: VectorStoreId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub full: Option<bool>,
}

impl VectorIndexInput {
    pub const fn new(store_id: VectorStoreId, full: bool) -> Self {
        Self {
            store_id,
            full: Some(full),
        }
    }

    pub fn full(&self) -> bool {
        self.full.unwrap_or(false)
    }
}

impl Validate for VectorIndexInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorIndexReportDto {
    pub store_id: VectorStoreId,
    pub files: u64,
    pub indexed: u64,
    pub unchanged: u64,
    pub removed: u64,
    pub skipped: u64,
    pub failed: u64,
    pub chunks: u64,
    pub cancelled: bool,
    pub notes: Vec<String>,
}

impl Validate for VectorIndexReportDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("files", self.files)?;
        check::safe_integer("indexed", self.indexed)?;
        check::safe_integer("unchanged", self.unchanged)?;
        check::safe_integer("removed", self.removed)?;
        check::safe_integer("skipped", self.skipped)?;
        check::safe_integer("failed", self.failed)?;
        check::safe_integer("chunks", self.chunks)?;
        if self.notes.len() > INDEX_NOTES_MAX {
            return Err(invalid("notes", "maxItems"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorDocumentDto {
    pub id: DocumentId,
    pub store_id: VectorStoreId,
    pub source_path: String,
    pub content_hash: String,
    pub chunk_count: u64,
    pub bytes: u64,
    pub indexed_at: Timestamp,
}

impl Validate for VectorDocumentDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("sourcePath", &self.source_path)?;
        check::non_empty("contentHash", &self.content_hash)?;
        check::safe_integer("chunkCount", self.chunk_count)?;
        check::safe_integer("bytes", self.bytes)?;
        check::timestamp("indexedAt", self.indexed_at)
    }
}

pub type VectorDocumentsListOutput = Vec<VectorDocumentDto>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorDocumentsRemoveInput {
    pub store_id: VectorStoreId,
    pub id: DocumentId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorDocumentsRemoveOutput {
    pub id: DocumentId,
    pub removed: bool,
}

impl Validate for VectorDocumentsRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

fn check_question(question: &str) -> Result<(), AppError> {
    check::trimmed_name("question", question, QUESTION_MAX)
}

fn check_expected(ids: &[DocumentId]) -> Result<(), AppError> {
    if ids.is_empty() {
        return Err(invalid("expectedDocumentIds", "minItems"));
    }
    if ids.len() > EXPECTED_DOCUMENTS_MAX {
        return Err(invalid("expectedDocumentIds", "maxItems"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalCaseDto {
    pub id: VectorEvalCaseId,
    pub store_id: VectorStoreId,
    pub question: String,
    pub expected_document_ids: Vec<DocumentId>,
    pub created_at: Timestamp,
}

impl Validate for VectorEvalCaseDto {
    fn validate(&self) -> Result<(), AppError> {
        check_question(&self.question)?;
        check_expected(&self.expected_document_ids)?;
        check::timestamp("createdAt", self.created_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddVectorEvalCaseInput {
    pub store_id: VectorStoreId,
    pub question: String,
    pub expected_document_ids: Vec<DocumentId>,
}

impl Validate for AddVectorEvalCaseInput {
    fn validate(&self) -> Result<(), AppError> {
        check_question(&self.question)?;
        check_expected(&self.expected_document_ids)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalCaseRef {
    pub store_id: VectorStoreId,
    pub id: VectorEvalCaseId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalRemoveOutput {
    pub id: VectorEvalCaseId,
    pub removed: bool,
}

impl Validate for VectorEvalRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalRunInput {
    pub store_id: VectorStoreId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub k: Option<u64>,
}

impl VectorEvalRunInput {
    pub fn k(&self) -> u64 {
        self.k.unwrap_or(DEFAULT_EVAL_K)
    }
}

impl Validate for VectorEvalRunInput {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("k", self.k(), 1, 100)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalMetricsDto {
    pub cases: u64,
    pub hit_at1: f64,
    pub hit_at5: f64,
    pub recall_at_k: f64,
    pub mrr: f64,
}

impl Validate for VectorEvalMetricsDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("cases", self.cases)?;
        check_ratio("hitAt1", self.hit_at1)?;
        check_ratio("hitAt5", self.hit_at5)?;
        check_ratio("recallAtK", self.recall_at_k)?;
        check_ratio("mrr", self.mrr)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalCaseResultDto {
    pub case_id: VectorEvalCaseId,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub rank: Option<u64>,
    pub found: u64,
    pub expected: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
}

impl Validate for VectorEvalCaseResultDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(rank) = self.rank {
            check::safe_integer("rank", rank)?;
            check::int_range("rank", rank, 1, u64::MAX)?;
        }
        check::safe_integer("found", self.found)?;
        check::safe_integer("expected", self.expected)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalModeResultDto {
    pub mode: VectorEvalMode,
    pub metrics: VectorEvalMetricsDto,
    pub cases: Vec<VectorEvalCaseResultDto>,
    #[serde(default)]
    pub fallbacks: u64,
}

impl Validate for VectorEvalModeResultDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("metrics", self.metrics.validate())?;
        nested("cases", self.cases.validate())?;
        check::safe_integer("fallbacks", self.fallbacks)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalRunDto {
    pub store_id: VectorStoreId,
    pub k: u64,
    pub ran_at: Timestamp,
    pub duration_ms: f64,
    pub modes: Vec<VectorEvalModeResultDto>,
}

impl Validate for VectorEvalRunDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("k", self.k)?;
        check::int_range("k", self.k, 1, u64::MAX)?;
        check::timestamp("ranAt", self.ran_at)?;
        check_non_negative("durationMs", self.duration_ms)?;
        if self.modes.is_empty() {
            return Err(invalid("modes", "minItems"));
        }
        nested("modes", self.modes.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorEvalSetDto {
    pub cases: Vec<VectorEvalCaseDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_run: Option<VectorEvalRunDto>,
}

impl Validate for VectorEvalSetDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("cases", self.cases.validate())?;
        nested("lastRun", self.last_run.validate())
    }
}

no_refinements!(
    VectorStoreRef,
    PickVectorSourceInput,
    VectorDocumentsRemoveInput,
    VectorEvalCaseRef
);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const STORE: &str = "0199aa11-1111-7111-8111-111111111111";
    const PROVIDER: &str = "0199aa11-2222-7111-8111-111111111111";

    #[test]
    fn a_create_input_applies_the_zod_defaults_through_its_accessors() {
        let input = parse::<CreateVectorStoreInput>(json!({
            "name": "Knowledge",
            "embeddingProviderId": PROVIDER,
            "embeddingModelId": "bge-m3",
        }))
        .expect("parses");
        assert_eq!(input.dimension(), 1024);
        assert_eq!(input.chunk_size(), 256);
        assert_eq!(input.chunk_overlap(), 32);
        assert_eq!(input.metric(), VectorMetric::Cosine);
        assert_eq!(input.search_mode(), VectorSearchMode::Hybrid);
        assert_eq!(input.rerank(), VectorRerankConfig::default());
        assert!((input.chat_min_score() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn overlap_must_stay_below_the_chunk_size() {
        let error = parse::<CreateVectorStoreInput>(json!({
            "name": "Knowledge",
            "embeddingProviderId": PROVIDER,
            "embeddingModelId": "bge-m3",
            "chunkOverlap": 999,
        }))
        .expect_err("refused");
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("field").cloned()),
            Some(json!("chunkOverlap"))
        );
    }

    #[test]
    fn a_nested_config_fills_its_own_defaults() {
        let config: VectorRerankConfig =
            serde_json::from_value(json!({ "enabled": true })).expect("parses");
        assert_eq!(config.candidates, 50);
        assert!(serde_json::from_value::<VectorRerankConfig>(json!({ "enabled": null })).is_err());
    }

    #[test]
    fn a_hit_defaults_its_text_section_and_page() {
        let hit = parse::<VectorSearchHitDto>(json!({
            "id": "a:0",
            "score": 0.5,
            "payload": null,
            "documentId": "a",
            "chunkIndex": 0,
            "path": "a.md",
        }))
        .expect("parses");
        assert_eq!(
            serde_json::to_value(&hit).expect("serialises"),
            json!({
                "id": "a:0",
                "score": 0.5,
                "payload": null,
                "documentId": "a",
                "chunkIndex": 0,
                "path": "a.md",
                "text": "",
                "section": "",
                "page": null,
            })
        );
        assert!(parse::<VectorSearchHitDto>(json!({ "id": "a" })).is_err());
    }

    #[test]
    fn the_floor_exempts_keyword_only_hits() {
        assert!(below_relevance_floor(0.1, None, 0.3));
        assert!(below_relevance_floor(
            0.1,
            Some(VectorHitProvenance::Both),
            0.3
        ));
        assert!(!below_relevance_floor(
            0.1,
            Some(VectorHitProvenance::Keyword),
            0.3
        ));
        assert!(!below_relevance_floor(0.3, None, 0.3));
    }

    #[test]
    fn a_search_input_is_trimmed_before_its_length_is_checked() {
        assert!(parse::<VectorSearchInput>(json!({ "storeId": STORE, "query": "   " })).is_err());
        assert!(
            parse::<VectorSearchInput>(json!({ "storeId": STORE, "query": "q", "k": 0 })).is_err()
        );
        assert!(
            parse::<VectorSearchInput>(json!({ "storeId": STORE, "query": "q", "minScore": 1.5 }))
                .is_err()
        );
    }
}
