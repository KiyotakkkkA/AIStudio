use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::{check, js_slice, js_trim};
use zvs_contract::{
    AppError, AppErrorCode, ChatCitationDto, Validate, VectorChunkDto, VectorSearchHitDto,
    VectorStoreDto, VectorStoreId,
};

use super::registry::NodeRegistry;
use super::types::{NodeDef, PermissionRequirement, StepContext, VectorSearch};
use crate::services::chat_history::{ChunkPiece, join_chunks};
use crate::services::rerank::hit_text;
use crate::services::retrieval_merge::{
    CHAT_CHUNKS_PER_DOCUMENT, CHAT_FETCH_K, MergeOptions, StoreHits, merge_retrieval,
};
use crate::services::vector_stores::VectorSearchOptions;

pub const KNOWLEDGE_SEARCH: &str = "knowledge.search";
pub const KNOWLEDGE_TOOLS: [&str; 1] = [KNOWLEDGE_SEARCH];

pub const SEARCH_LIMIT_DEFAULT: u64 = 6;
pub const SEARCH_LIMIT_MAX: u64 = 10;
pub const QUERY_MAX_CHARS: usize = 1000;
pub const PASSAGE_MAX_CHARS: usize = 1500;
pub const READ_MAX_CHUNKS: u64 = 12;
pub const READ_MAX_CHARS: usize = 6000;
pub const SECTION_SCAN_CHUNKS: u64 = 400;

pub const NO_STORES: &str = "К диалогу не подключены хранилища знаний";
pub const STORE_OUT_OF_SCOPE: &str = "Это хранилище не подключено к диалогу";
pub const DOCUMENT_OUT_OF_SCOPE: &str = "Документ не найден в подключённых хранилищах";
pub const SECTION_NOT_FOUND: &str = "В документе нет раздела с таким названием";
pub const EMPTY_QUERY: &str = "Пустой поисковый запрос";
pub const QUERY_OR_DOCUMENT: &str = "Укажите query для поиска или documentId для чтения документа";
pub const VECTORS_MISSING: &str = "Сервис векторного поиска недоступен";

pub const SEARCH_DESCRIPTION: &str = "Поиск по базам знаний, подключённым к диалогу. Возвращает нумерованные фрагменты с путём документа, разделом, страницей и оценкой релевантности. Вызывайте, когда ответ может содержаться в документах пользователя; для приветствий и светской беседы — не вызывайте. Формулируйте краткий запрос своими словами, не копируйте сообщение пользователя. Для сравнения или вопроса из нескольких частей выполняйте отдельный поиск по каждой части. Если фрагменты неполные или не по теме, повторите поиск с другой формулировкой. Когда найденного фрагмента недостаточно, вместо query передайте documentId из результата и section — название раздела — либо диапазон fromChunk–toChunk: вернётся непрерывная часть документа. Каждое утверждение, взятое из фрагмента, снабжайте ссылкой [n] с его номером. Если повторный поиск не дал релевантных результатов, сообщите, что ответа в базе знаний нет.";

fn missing_vectors() -> AppError {
    AppError::new(AppErrorCode::Conflict, VECTORS_MISSING)
}

fn require_scope(scope: &[VectorStoreId]) -> Result<(), AppError> {
    if scope.is_empty() {
        return Err(AppError::new(AppErrorCode::PermissionDenied, NO_STORES));
    }
    Ok(())
}

async fn scoped_stores(
    vectors: &dyn VectorSearch,
    scope: &[VectorStoreId],
) -> Result<Vec<VectorStoreDto>, AppError> {
    let listed = vectors.list().await?;
    Ok(scope
        .iter()
        .filter_map(|id| listed.iter().find(|store| store.id == *id).cloned())
        .collect())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchInput {
    #[serde(default)]
    #[schemars(description = "What to look for, phrased as a focused search query.")]
    pub query: String,
    #[serde(default)]
    #[schemars(
        description = "Optional: the name or id of one attached knowledge base to search. Omit to search all of them."
    )]
    pub store: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional: how many passages to return, 1–10. Default 6.")]
    pub limit: Option<u64>,
    #[serde(default)]
    #[schemars(description = "Read mode: the documentId of a returned passage, instead of query.")]
    pub document_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Read mode: read the section with this title (or part of it), as shown on a passage."
    )]
    pub section: Option<String>,
    #[serde(default)]
    #[schemars(description = "Read mode: the first chunk number to read, 0-based.")]
    pub from_chunk: Option<u64>,
    #[serde(default)]
    #[schemars(
        description = "Read mode: the last chunk number to read, inclusive. At most 12 chunks are returned."
    )]
    pub to_chunk: Option<u64>,
    #[serde(default)]
    #[schemars(skip)]
    pub scope: Vec<VectorStoreId>,
}

impl KnowledgeSearchInput {
    pub fn document(&self) -> Option<&str> {
        self.document_id
            .as_deref()
            .map(js_trim)
            .filter(|document| !document.is_empty())
    }
}

impl Validate for KnowledgeSearchInput {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("query", &self.query, QUERY_MAX_CHARS * 4)?;
        for (field, value) in [
            ("limit", self.limit),
            ("fromChunk", self.from_chunk),
            ("toChunk", self.to_chunk),
        ] {
            if let Some(value) = value {
                check::safe_integer(field, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePassage {
    pub store_id: VectorStoreId,
    pub store: String,
    pub document_id: String,
    pub path: String,
    pub section: String,
    pub page: Option<u64>,
    pub chunk_index: u64,
    pub score: f64,
    pub text: String,
    pub truncated: bool,
}

impl KnowledgePassage {
    pub fn citation(&self) -> ChatCitationDto {
        ChatCitationDto {
            store_id: self.store_id,
            document_id: self.document_id.clone(),
            source_path: self.path.clone(),
            chunk_index: self.chunk_index,
            score: self.score.clamp(0.0, 1.0),
            section: self.section.clone(),
            page: self.page,
            related: None,
        }
    }
}

impl Validate for KnowledgePassage {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("chunkIndex", self.chunk_index)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchOutput {
    pub query: String,
    pub passages: Vec<KnowledgePassage>,
    pub below_floor: u64,
    pub searched: Vec<String>,
}

impl Validate for KnowledgeSearchOutput {
    fn validate(&self) -> Result<(), AppError> {
        for passage in &self.passages {
            passage.validate()?;
        }
        check::safe_integer("belowFloor", self.below_floor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum KnowledgeResult {
    Read(KnowledgeReadOutput),
    Search(KnowledgeSearchOutput),
}

impl Validate for KnowledgeResult {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Read(output) => output.validate(),
            Self::Search(output) => output.validate(),
        }
    }
}

fn pick_stores(
    stores: Vec<VectorStoreDto>,
    wanted: Option<&str>,
) -> Result<Vec<VectorStoreDto>, AppError> {
    let Some(wanted) = wanted.map(js_trim).filter(|wanted| !wanted.is_empty()) else {
        return Ok(stores);
    };
    let lowered = wanted.to_lowercase();
    let names: Vec<String> = stores.iter().map(|store| store.name.clone()).collect();
    let chosen: Vec<VectorStoreDto> = stores
        .into_iter()
        .filter(|store| store.id.to_string() == wanted || store.name.to_lowercase() == lowered)
        .collect();
    if chosen.is_empty() {
        return Err(
            AppError::new(AppErrorCode::PermissionDenied, STORE_OUT_OF_SCOPE)
                .with_detail("store", wanted)
                .with_detail("available", names),
        );
    }
    Ok(chosen)
}

fn passage(store: &VectorStoreDto, hit: &VectorSearchHitDto) -> KnowledgePassage {
    let text = hit_text(&hit.text, &hit.payload);
    let sliced = js_slice(&text, PASSAGE_MAX_CHARS);
    KnowledgePassage {
        store_id: store.id,
        store: store.name.clone(),
        document_id: hit.document_id.clone(),
        path: hit.path.clone(),
        section: hit.section.clone(),
        page: hit.page,
        chunk_index: hit.chunk_index,
        score: hit.score,
        truncated: sliced.len() < text.len(),
        text: sliced.to_owned(),
    }
}

pub struct KnowledgeSearchNode;

#[async_trait]
impl NodeDef for KnowledgeSearchNode {
    type Input = KnowledgeSearchInput;
    type Output = KnowledgeResult;

    fn node_type(&self) -> &str {
        KNOWLEDGE_SEARCH
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(KNOWLEDGE_SEARCH)
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: KnowledgeSearchInput,
    ) -> Result<KnowledgeResult, AppError> {
        context.check_cancelled()?;
        require_scope(&input.scope)?;
        let query = js_slice(js_trim(&input.query), QUERY_MAX_CHARS).to_owned();
        if query.is_empty() && input.document().is_none() {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, QUERY_OR_DOCUMENT)
                    .with_detail("field", "query"),
            );
        }
        let vectors = context
            .services
            .vector_stores
            .as_ref()
            .ok_or_else(missing_vectors)?;
        if let Some(document) = input.document() {
            return read_document(context, vectors.as_ref(), &input, document)
                .await
                .map(KnowledgeResult::Read);
        }
        let stores = pick_stores(
            scoped_stores(vectors.as_ref(), &input.scope).await?,
            input.store.as_deref(),
        )?;
        let limit = input
            .limit
            .unwrap_or(SEARCH_LIMIT_DEFAULT)
            .clamp(1, SEARCH_LIMIT_MAX);
        let mut gathered = Vec::new();
        let mut failure = None;
        for store in &stores {
            context.check_cancelled()?;
            let options = VectorSearchOptions {
                k: Some(CHAT_FETCH_K),
                ..VectorSearchOptions::default()
            };
            match vectors
                .search(&store.id.to_string(), &query, options, &context.cancel)
                .await
            {
                Ok(hits) => gathered.push(StoreHits {
                    store_id: store.id.to_string(),
                    hits,
                    floor: store.chat_min_score,
                }),
                Err(error) if context.is_cancelled() => return Err(error),
                Err(error) => failure = Some(error),
            }
        }
        if gathered.is_empty()
            && let Some(error) = failure
        {
            return Err(error);
        }
        let merged = merge_retrieval(
            &gathered,
            MergeOptions {
                limit: usize::try_from(limit).unwrap_or(usize::MAX),
                per_document: CHAT_CHUNKS_PER_DOCUMENT,
            },
        );
        let passages = merged
            .hits
            .iter()
            .filter_map(|merged| {
                stores
                    .iter()
                    .find(|store| store.id.to_string() == merged.store_id)
                    .map(|store| passage(store, &merged.hit))
            })
            .collect();
        Ok(KnowledgeResult::Search(KnowledgeSearchOutput {
            query,
            passages,
            below_floor: merged.stats.below_floor as u64,
            searched: stores.into_iter().map(|store| store.name).collect(),
        }))
    }

    fn tool_meta(&self, _input: &Value, output: &KnowledgeResult) -> Option<Value> {
        Some(match output {
            KnowledgeResult::Read(output) => json!({ "citations": [output.citation()] }),
            KnowledgeResult::Search(output) => json!({
                "query": output.query,
                "citations": output
                    .passages
                    .iter()
                    .map(KnowledgePassage::citation)
                    .collect::<Vec<_>>(),
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeReadOutput {
    pub store_id: VectorStoreId,
    pub store: String,
    pub document_id: String,
    pub path: String,
    pub section: String,
    pub page: Option<u64>,
    pub from_chunk: u64,
    pub to_chunk: u64,
    pub chunk_count: u64,
    pub text: String,
    pub truncated: bool,
}

impl KnowledgeReadOutput {
    pub fn citation(&self) -> ChatCitationDto {
        ChatCitationDto {
            store_id: self.store_id,
            document_id: self.document_id.clone(),
            source_path: self.path.clone(),
            chunk_index: self.from_chunk,
            score: 1.0,
            section: self.section.clone(),
            page: self.page,
            related: None,
        }
    }
}

impl Validate for KnowledgeReadOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("fromChunk", self.from_chunk)?;
        check::safe_integer("toChunk", self.to_chunk)?;
        check::safe_integer("chunkCount", self.chunk_count)
    }
}

pub fn read_window(from: Option<u64>, to: Option<u64>, count: u64) -> Option<(u64, u64)> {
    if count == 0 {
        return None;
    }
    let last = count - 1;
    let start = from.unwrap_or(0).min(last);
    let end = to
        .unwrap_or(start + READ_MAX_CHUNKS - 1)
        .max(start)
        .min(start + READ_MAX_CHUNKS - 1)
        .min(last);
    Some((start, end))
}

pub fn section_span(chunks: &[VectorChunkDto], wanted: &str) -> Option<(u64, u64)> {
    let lowered = js_trim(wanted).to_lowercase();
    if lowered.is_empty() {
        return None;
    }
    let first = chunks
        .iter()
        .find(|chunk| chunk.section.to_lowercase().contains(&lowered))?;
    let section = first.section.clone();
    let mut end = first.chunk_index;
    for chunk in chunks
        .iter()
        .filter(|chunk| chunk.chunk_index > first.chunk_index)
    {
        if chunk.section != section || chunk.chunk_index != end + 1 {
            break;
        }
        end = chunk.chunk_index;
    }
    Some((
        first.chunk_index,
        end.min(first.chunk_index + READ_MAX_CHUNKS - 1),
    ))
}

async fn read_document(
    context: &StepContext,
    vectors: &dyn VectorSearch,
    input: &KnowledgeSearchInput,
    document_id: &str,
) -> Result<KnowledgeReadOutput, AppError> {
    let document_id = document_id.to_owned();
    let stores = scoped_stores(vectors, &input.scope).await?;
    let mut located = None;
    for store in stores {
        if let Some(document) = vectors
            .document(&store.id.to_string(), &document_id)
            .await?
        {
            located = Some((store, document));
            break;
        }
    }
    let (store, document) = located.ok_or_else(|| {
        AppError::new(AppErrorCode::NotFound, DOCUMENT_OUT_OF_SCOPE)
            .with_detail("documentId", document_id.clone())
    })?;
    let store_key = store.id.to_string();
    let count = document.chunk_count;
    let section = input
        .section
        .as_deref()
        .map(js_trim)
        .filter(|section| !section.is_empty());
    let window = match section {
        Some(section) => {
            let scanned: Vec<u64> = (0..count.min(SECTION_SCAN_CHUNKS)).collect();
            let chunks = vectors.chunks(&store_key, &document_id, &scanned).await?;
            section_span(&chunks, section).ok_or_else(|| {
                AppError::new(AppErrorCode::NotFound, SECTION_NOT_FOUND)
                    .with_detail("section", section)
            })?
        }
        None => read_window(input.from_chunk, input.to_chunk, count).ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, DOCUMENT_OUT_OF_SCOPE)
                .with_detail("documentId", document_id.clone())
        })?,
    };
    context.check_cancelled()?;
    let indices: Vec<u64> = (window.0..=window.1).collect();
    let mut chunks = vectors.chunks(&store_key, &document_id, &indices).await?;
    chunks.sort_by_key(|chunk| chunk.chunk_index);
    let joined = join_chunks(
        &chunks
            .iter()
            .map(|chunk| ChunkPiece {
                text: chunk.text.clone(),
                payload: chunk.payload.clone(),
            })
            .collect::<Vec<_>>(),
    );
    let sliced = js_slice(&joined, READ_MAX_CHARS);
    let first = chunks.first();
    Ok(KnowledgeReadOutput {
        store_id: store.id,
        store: store.name,
        document_id,
        path: first.map_or_else(|| document.source_path.clone(), |chunk| chunk.path.clone()),
        section: first.map(|chunk| chunk.section.clone()).unwrap_or_default(),
        page: first.and_then(|chunk| chunk.page),
        from_chunk: window.0,
        to_chunk: chunks.last().map_or(window.1, |chunk| chunk.chunk_index),
        chunk_count: count,
        truncated: sliced.len() < joined.len(),
        text: sliced.to_owned(),
    })
}

pub fn register_knowledge_nodes(registry: &NodeRegistry) -> Result<(), AppError> {
    registry.register(KnowledgeSearchNode)
}

#[cfg(test)]
mod tests;
