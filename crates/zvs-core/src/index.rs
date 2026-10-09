use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt32Array,
    types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::{
    Table,
    index::{
        Index, IndexType,
        scalar::{FtsIndexBuilder, FullTextSearchQuery},
        vector::IvfHnswSqIndexBuilder,
    },
    query::{ExecutableQuery, QueryBase, Select},
    table::{OptimizeAction, OptimizeOptions},
};
use serde::{Deserialize, Serialize};

use crate::{CancellationToken, Error, Result, error::check_cancelled};

pub const DEFAULT_DIMENSION: u32 = 1024;
pub const UPSERT_BATCH_SIZE: usize = 256;
pub const SCHEMA_VERSION: u32 = 2;
pub const RRF_K: f32 = 60.0;
pub const DEFAULT_ANN_THRESHOLD: usize = 100_000;
pub const ANN_INDEX_TYPE: &str = "IVF_HNSW_SQ";
const TEXT_INDEX_LANGUAGE: &str = "Russian";
const ANN_NPROBES: usize = 32;
const ANN_REFINE_FACTOR: u32 = 4;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    #[default]
    Cosine,
    L2,
    Dot,
}

impl Metric {
    fn distance(self) -> lancedb::DistanceType {
        match self {
            Self::Cosine => lancedb::DistanceType::Cosine,
            Self::L2 => lancedb::DistanceType::L2,
            Self::Dot => lancedb::DistanceType::Dot,
        }
    }

    fn score(self, distance: f32) -> f32 {
        match self {
            Self::Cosine => (1.0 - distance).clamp(0.0, 1.0),
            Self::L2 => 1.0 / (1.0 + distance.max(0.0)),
            Self::Dot => 1.0 / (1.0 + (distance - 1.0).exp()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VectorRow {
    pub id: String,
    pub vector: Vec<f32>,
    pub payload: serde_json::Value,
    pub document_id: String,
    pub chunk_index: u32,
    pub path: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub page: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    Vector,
    Keyword,
    Both,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    #[default]
    Vector,
    Hybrid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
    pub payload: serde_json::Value,
    pub document_id: String,
    pub chunk_index: u32,
    pub path: String,
    pub text: String,
    pub section: String,
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fused_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredChunk {
    pub id: String,
    pub payload: serde_json::Value,
    pub document_id: String,
    pub chunk_index: u32,
    pub path: String,
    pub text: String,
    pub section: String,
    pub page: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fused {
    pub id: String,
    pub score: f32,
    pub vector_rank: Option<usize>,
    pub keyword_rank: Option<usize>,
}

impl Fused {
    pub fn provenance(&self) -> Provenance {
        match (self.vector_rank, self.keyword_rank) {
            (Some(_), Some(_)) => Provenance::Both,
            (None, Some(_)) => Provenance::Keyword,
            _ => Provenance::Vector,
        }
    }

    fn best_rank(&self) -> usize {
        self.vector_rank
            .unwrap_or(usize::MAX)
            .min(self.keyword_rank.unwrap_or(usize::MAX))
    }
}

pub fn reciprocal_rank_fusion(vector: &[&str], keyword: &[&str], k: f32) -> Vec<Fused> {
    let mut fused: Vec<Fused> = Vec::new();
    let mut positions: HashMap<&str, usize> = HashMap::new();
    for (list, from_vector) in [(vector, true), (keyword, false)] {
        for (rank, id) in list.iter().enumerate() {
            let position = *positions.entry(id).or_insert_with(|| {
                fused.push(Fused {
                    id: (*id).to_owned(),
                    score: 0.0,
                    vector_rank: None,
                    keyword_rank: None,
                });
                fused.len() - 1
            });
            let entry = &mut fused[position];
            let slot = if from_vector {
                &mut entry.vector_rank
            } else {
                &mut entry.keyword_rank
            };
            if slot.is_none() {
                *slot = Some(rank + 1);
                entry.score += 1.0 / (k + (rank + 1) as f32);
            }
        }
    }
    fused.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| {
                right
                    .keyword_rank
                    .is_some()
                    .cmp(&left.keyword_rank.is_some())
            })
            .then_with(|| left.best_rank().cmp(&right.best_rank()))
            .then_with(|| left.id.cmp(&right.id))
    });
    fused
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub row_count: usize,
    pub document_count: usize,
    pub dimension: u32,
    pub metric: Metric,
    pub on_disk_bytes: u64,
    pub index_type: String,
    pub schema_version: u32,
    pub keyword_index: bool,
}

pub struct VectorIndex {
    table: Table,
    directory: PathBuf,
    dimension: u32,
    metric: Metric,
    schema_version: u32,
}

fn backend(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}

fn schema(dimension: u32, metric: Metric, version: u32) -> Arc<Schema> {
    let mut fields = vec![
        Field::new("id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimension as i32,
            ),
            false,
        ),
        Field::new("payload", DataType::Utf8, false),
        Field::new("document_id", DataType::Utf8, false),
        Field::new("chunk_index", DataType::UInt32, false),
        Field::new("path", DataType::Utf8, false),
    ];
    let mut metadata = HashMap::from([(
        "zvs.metric".to_owned(),
        serde_json::to_string(&metric).expect("metric serializes"),
    )]);
    if version >= 2 {
        fields.extend([
            Field::new("text", DataType::Utf8, false),
            Field::new("section", DataType::Utf8, false),
            Field::new("page", DataType::UInt32, true),
        ]);
        metadata.insert("zvs.schema".into(), version.to_string());
    }
    Arc::new(Schema::new_with_metadata(fields, metadata))
}

fn text_index() -> Result<Index> {
    Ok(Index::FTS(
        FtsIndexBuilder::default()
            .language(TEXT_INDEX_LANGUAGE)
            .map_err(backend)?
            .stem(true)
            .remove_stop_words(true)
            .lower_case(true)
            .ascii_folding(true),
    ))
}

fn local_path(path: &Path) -> Result<&str> {
    if !path.is_absolute() {
        return Err(Error::InvalidInput(
            "Vector index path must be absolute and supplied by the host".into(),
        ));
    }
    path.to_str()
        .ok_or_else(|| Error::InvalidInput("Vector index path must be UTF-8".into()))
}

async fn collect(
    stream: impl futures::Stream<Item = lancedb::Result<RecordBatch>>,
) -> Result<Vec<RecordBatch>> {
    stream.try_collect().await.map_err(backend)
}

impl VectorIndex {
    pub fn remove(path: &Path) -> Result<()> {
        local_path(path)?;
        if path.file_name().is_none() {
            return Err(Error::InvalidInput(
                "Cannot remove a filesystem root".into(),
            ));
        }
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn create(path: &Path, dimension: u32, metric: Metric) -> Result<Self> {
        if dimension == 0 || dimension > i32::MAX as u32 {
            return Err(Error::InvalidInput(
                "Dimension must be between 1 and 2147483647".into(),
            ));
        }
        let db = lancedb::connect(local_path(path)?)
            .execute()
            .await
            .map_err(backend)?;
        let table = match db
            .create_empty_table("vectors", schema(dimension, metric, SCHEMA_VERSION))
            .execute()
            .await
        {
            Ok(table) => table,
            Err(lancedb::Error::TableAlreadyExists { .. }) => {
                db.open_table("vectors").execute().await.map_err(backend)?
            }
            Err(error) => return Err(backend(error)),
        };
        let index = Self::from_table(path, table).await?;
        if index.dimension != dimension || index.metric != metric {
            return Err(Error::InvalidInput(format!(
                "Existing vector index has dimension {} and metric {:?}; requested dimension {dimension} and metric {metric:?}",
                index.dimension, index.metric
            )));
        }
        index.ensure_text_index().await?;
        Ok(index)
    }

    pub async fn open(path: &Path) -> Result<Self> {
        local_path(path)?;
        if !path.join("vectors.lance").is_dir() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Vector index does not exist",
            )));
        }
        let db = lancedb::connect(local_path(path)?)
            .execute()
            .await
            .map_err(backend)?;
        let table = db.open_table("vectors").execute().await.map_err(backend)?;
        Self::from_table(path, table).await
    }

    async fn from_table(path: &Path, table: Table) -> Result<Self> {
        let actual = table.schema().await.map_err(backend)?;
        let dimension = match actual
            .field_with_name("vector")
            .map_err(backend)?
            .data_type()
        {
            DataType::FixedSizeList(_, dimension) if *dimension > 0 => *dimension as u32,
            _ => return Err(Error::InvalidInput("Unsupported vector schema".into())),
        };
        let metric = actual
            .metadata()
            .get("zvs.metric")
            .ok_or_else(|| Error::InvalidInput("Vector index has no metric metadata".into()))?;
        let metric: Metric = serde_json::from_str(metric).map_err(backend)?;
        let schema_version = match actual.metadata().get("zvs.schema") {
            None => 1,
            Some(version) => version
                .parse::<u32>()
                .ok()
                .filter(|version| (2..=SCHEMA_VERSION).contains(version))
                .ok_or_else(|| {
                    Error::InvalidInput(format!("Unsupported vector index schema {version}"))
                })?,
        };
        if actual.fields() != schema(dimension, metric, schema_version).fields() {
            return Err(Error::InvalidInput(
                "Unsupported vector index schema".into(),
            ));
        }
        Ok(Self {
            table,
            directory: path.join("vectors.lance"),
            dimension,
            metric,
            schema_version,
        })
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn validate_vector(&self, vector: &[f32]) -> Result<()> {
        if vector.len() != self.dimension as usize {
            return Err(Error::InvalidInput(format!(
                "Vector dimension {} does not match index dimension {}",
                vector.len(),
                self.dimension
            )));
        }
        let norm = vector.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        if vector.iter().any(|v| !v.is_finite())
            || norm > f64::from(f32::MAX) / 4.0
            || (self.metric == Metric::Cosine && norm < f64::from(f32::MIN_POSITIVE))
        {
            return Err(Error::InvalidInput(
                "Vectors must be finite, within float32 distance limits, and nonzero for cosine"
                    .into(),
            ));
        }
        Ok(())
    }

    fn validate_search(&self, vector: &[f32], k: usize, min_score: f32) -> Result<()> {
        self.validate_vector(vector)?;
        if k == 0 || !min_score.is_finite() || !(0.0..=1.0).contains(&min_score) {
            return Err(Error::InvalidInput(
                "Search requires positive k and minimum score in [0, 1]".into(),
            ));
        }
        Ok(())
    }

    pub async fn upsert(&self, rows: &[VectorRow], token: &CancellationToken) -> Result<usize> {
        if self.schema_version < SCHEMA_VERSION {
            return Err(Error::InvalidInput(
                "Vector index uses an older schema and must be fully re-indexed".into(),
            ));
        }
        let mut ids = HashSet::new();
        for row in rows {
            check_cancelled(token)?;
            self.validate_vector(&row.vector)?;
            if row.id.is_empty() || row.document_id.is_empty() || !ids.insert(&row.id) {
                return Err(Error::InvalidInput(
                    "Rows require nonempty unique IDs and nonempty document IDs".into(),
                ));
            }
        }
        check_cancelled(token)?;
        let schema = schema(self.dimension, self.metric, self.schema_version);
        for batch in rows.chunks(UPSERT_BATCH_SIZE) {
            check_cancelled(token)?;
            let data = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(StringArray::from_iter_values(
                        batch.iter().map(|row| row.id.as_str()),
                    )),
                    Arc::new(
                        FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                            batch
                                .iter()
                                .map(|row| Some(row.vector.iter().copied().map(Some))),
                            self.dimension as i32,
                        ),
                    ),
                    Arc::new(StringArray::from_iter_values(
                        batch.iter().map(|row| row.payload.to_string()),
                    )),
                    Arc::new(StringArray::from_iter_values(
                        batch.iter().map(|row| row.document_id.as_str()),
                    )),
                    Arc::new(UInt32Array::from_iter_values(
                        batch.iter().map(|row| row.chunk_index),
                    )),
                    Arc::new(StringArray::from_iter_values(
                        batch.iter().map(|row| row.path.as_str()),
                    )),
                    Arc::new(StringArray::from_iter_values(
                        batch.iter().map(|row| row.text.as_str()),
                    )),
                    Arc::new(StringArray::from_iter_values(
                        batch.iter().map(|row| row.section.as_str()),
                    )),
                    Arc::new(UInt32Array::from_iter(batch.iter().map(|row| row.page))),
                ],
            )
            .map_err(backend)?;
            let mut merge = self.table.merge_insert(&["id"]);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all();
            let reader = arrow_array::RecordBatchIterator::new(vec![Ok(data)], schema.clone());
            merge.execute(Box::new(reader)).await.map_err(backend)?;
        }
        check_cancelled(token)?;
        Ok(rows.len())
    }

    pub async fn search(
        &self,
        vector: &[f32],
        k: usize,
        min_score: f32,
        filter: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        self.validate_search(vector, k, min_score)?;
        Ok(self
            .nearest(vector, k, filter)
            .await?
            .into_iter()
            .filter(|hit| hit.score >= min_score)
            .collect())
    }

    async fn nearest(
        &self,
        vector: &[f32],
        k: usize,
        filter: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        let mut query = self
            .table
            .query()
            .nearest_to(vector)
            .map_err(backend)?
            .distance_type(self.metric.distance())
            .limit(k);
        query = if self.vector_index().await?.is_some() {
            query.nprobes(ANN_NPROBES).refine_factor(ANN_REFINE_FACTOR)
        } else {
            query.bypass_vector_index()
        };
        if let Some(filter) = filter {
            query = query.only_if(filter);
        }
        self.scored(collect(query.execute().await.map_err(backend)?).await?)
    }

    fn scored(&self, batches: Vec<RecordBatch>) -> Result<Vec<SearchHit>> {
        let mut hits = Vec::new();
        for batch in batches {
            let distances = column::<Float32Array>(&batch, "_distance")?;
            for (row, chunk) in self.read_chunks(&batch)?.into_iter().enumerate() {
                let score = self.metric.score(distances.value(row));
                if !score.is_finite() {
                    return Err(Error::Backend("Search returned a non-finite score".into()));
                }
                hits.push(hit(chunk, score));
            }
        }
        Ok(hits)
    }

    pub async fn keyword_search(
        &self,
        query: &str,
        k: usize,
        filter: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        if k == 0 {
            return Err(Error::InvalidInput("Search requires positive k".into()));
        }
        if self.schema_version < 2 || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let search = FullTextSearchQuery::new(query.to_owned())
            .with_column("text".into())
            .map_err(backend)?;
        let mut request = self.table.query().full_text_search(search).limit(k);
        if let Some(filter) = filter {
            request = request.only_if(filter);
        }
        let batches = collect(request.execute().await.map_err(backend)?).await?;
        let mut hits = Vec::new();
        for batch in batches {
            let scores = column::<Float32Array>(&batch, "_score")?;
            for (row, chunk) in self.read_chunks(&batch)?.into_iter().enumerate() {
                let mut found = hit(chunk, 0.0);
                found.keyword_score = Some(scores.value(row).max(0.0));
                hits.push(found);
            }
        }
        hits.sort_by(|left, right| {
            right
                .keyword_score
                .unwrap_or(0.0)
                .total_cmp(&left.keyword_score.unwrap_or(0.0))
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(k);
        Ok(hits)
    }

    pub async fn hybrid_search(
        &self,
        vector: &[f32],
        query: &str,
        k: usize,
        min_score: f32,
        filter: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        self.validate_search(vector, k, min_score)?;
        let semantic = self.nearest(vector, k, filter).await?;
        let lexical = self.keyword_search(query, k, filter).await?;
        let fused = reciprocal_rank_fusion(
            &semantic
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            &lexical
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            RRF_K,
        );
        let mut by_id: HashMap<String, SearchHit> = semantic
            .into_iter()
            .map(|hit| (hit.id.clone(), hit))
            .collect();
        let keyword_scores: HashMap<String, f32> = lexical
            .iter()
            .map(|hit| (hit.id.clone(), hit.keyword_score.unwrap_or(0.0)))
            .collect();
        let missing: Vec<&str> = lexical
            .iter()
            .filter(|hit| !by_id.contains_key(&hit.id))
            .map(|hit| hit.id.as_str())
            .collect();
        if !missing.is_empty() {
            let request = self
                .table
                .query()
                .nearest_to(vector)
                .map_err(backend)?
                .distance_type(self.metric.distance())
                .bypass_vector_index()
                .only_if(format!("id IN ({})", list(&missing)))
                .limit(missing.len());
            for found in self.scored(collect(request.execute().await.map_err(backend)?).await?)? {
                by_id.insert(found.id.clone(), found);
            }
        }
        for found in lexical {
            by_id.entry(found.id.clone()).or_insert(found);
        }
        Ok(fused
            .into_iter()
            .filter_map(|entry| {
                let mut found = by_id.remove(&entry.id)?;
                found.keyword_score = keyword_scores.get(&entry.id).copied();
                found.fused_score = Some(entry.score);
                found.provenance = Some(entry.provenance());
                (found.score >= min_score || entry.keyword_rank.is_some()).then_some(found)
            })
            .take(k)
            .collect())
    }

    pub async fn chunks(&self, document_id: &str, indices: &[u32]) -> Result<Vec<StoredChunk>> {
        if indices.is_empty() {
            return Ok(Vec::new());
        }
        let filter = format!(
            "document_id = {} AND chunk_index IN ({})",
            literal(document_id),
            indices
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        let batches = collect(
            self.table
                .query()
                .only_if(filter)
                .execute()
                .await
                .map_err(backend)?,
        )
        .await?;
        let mut chunks = Vec::new();
        for batch in batches {
            chunks.extend(self.read_chunks(&batch)?);
        }
        chunks.sort_by_key(|chunk| chunk.chunk_index);
        Ok(chunks)
    }

    fn read_chunks(&self, batch: &RecordBatch) -> Result<Vec<StoredChunk>> {
        let ids = column::<StringArray>(batch, "id")?;
        let payloads = column::<StringArray>(batch, "payload")?;
        let documents = column::<StringArray>(batch, "document_id")?;
        let chunks = column::<UInt32Array>(batch, "chunk_index")?;
        let paths = column::<StringArray>(batch, "path")?;
        let current = if self.schema_version >= 2 {
            Some((
                column::<StringArray>(batch, "text")?,
                column::<StringArray>(batch, "section")?,
                column::<UInt32Array>(batch, "page")?,
            ))
        } else {
            None
        };
        let mut rows = Vec::with_capacity(batch.num_rows());
        for row in 0..batch.num_rows() {
            let payload: serde_json::Value =
                serde_json::from_str(payloads.value(row)).map_err(backend)?;
            let (text, section, page) = match current {
                Some((texts, sections, pages)) => (
                    texts.value(row).to_owned(),
                    sections.value(row).to_owned(),
                    pages.is_valid(row).then(|| pages.value(row)),
                ),
                None => (
                    payload
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    String::new(),
                    None,
                ),
            };
            rows.push(StoredChunk {
                id: ids.value(row).into(),
                payload,
                document_id: documents.value(row).into(),
                chunk_index: chunks.value(row),
                path: paths.value(row).into(),
                text,
                section,
                page,
            });
        }
        Ok(rows)
    }

    async fn vector_index(&self) -> Result<Option<String>> {
        Ok(self
            .table
            .list_indices()
            .await
            .map_err(backend)?
            .into_iter()
            .find(|index| index.columns.iter().any(|column| column == "vector"))
            .map(|index| index_type_name(&index.index_type)))
    }

    async fn has_text_index(&self) -> Result<bool> {
        if self.schema_version < 2 {
            return Ok(false);
        }
        Ok(self
            .table
            .list_indices()
            .await
            .map_err(backend)?
            .iter()
            .any(|index| {
                index.index_type == IndexType::FTS
                    && index.columns.iter().any(|column| column == "text")
            }))
    }

    async fn ensure_text_index(&self) -> Result<bool> {
        if self.schema_version < 2 || self.has_text_index().await? {
            return Ok(false);
        }
        self.table
            .create_index(&["text"], text_index()?)
            .execute()
            .await
            .map_err(backend)?;
        Ok(true)
    }

    pub async fn maintain(&self, ann_threshold: usize) -> Result<IndexStats> {
        let rebuilt = self.ensure_text_index().await?;
        let rows = self.table.count_rows(None).await.map_err(backend)?;
        let mut trained = false;
        if ann_threshold > 0 && rows >= ann_threshold && self.vector_index().await?.is_none() {
            self.table
                .create_index(
                    &["vector"],
                    Index::IvfHnswSq(
                        IvfHnswSqIndexBuilder::default().distance_type(self.metric.distance()),
                    ),
                )
                .execute()
                .await
                .map_err(backend)?;
            trained = true;
        }
        if !rebuilt && !trained && !self.table.list_indices().await.map_err(backend)?.is_empty() {
            self.table
                .optimize(OptimizeAction::Index(OptimizeOptions::default()))
                .await
                .map_err(backend)?;
        }
        self.stats().await
    }

    pub async fn delete_by_source(&self, document_id: &str) -> Result<()> {
        self.table
            .delete(&format!("document_id = {}", literal(document_id)))
            .await
            .map(|_| ())
            .map_err(backend)
    }

    pub async fn delete_by_ids(&self, ids: &[String]) -> Result<()> {
        for batch in ids.chunks(UPSERT_BATCH_SIZE) {
            self.table
                .delete(&format!(
                    "id IN ({})",
                    list(&batch.iter().map(String::as_str).collect::<Vec<_>>())
                ))
                .await
                .map_err(backend)?;
        }
        Ok(())
    }

    pub async fn stats(&self) -> Result<IndexStats> {
        let mut documents = HashSet::new();
        let mut row_count = 0;
        let mut batches = self
            .table
            .query()
            .select(Select::columns(&["document_id"]))
            .execute()
            .await
            .map_err(backend)?;
        while let Some(batch) = batches.try_next().await.map_err(backend)? {
            row_count += batch.num_rows();
            for document in column::<StringArray>(&batch, "document_id")?
                .iter()
                .flatten()
            {
                documents.insert(document.to_owned());
            }
        }
        Ok(IndexStats {
            row_count,
            document_count: documents.len(),
            dimension: self.dimension,
            metric: self.metric,
            on_disk_bytes: directory_bytes(&self.directory)?,
            index_type: self.vector_index().await?.unwrap_or_else(|| "FLAT".into()),
            schema_version: self.schema_version,
            keyword_index: self.has_text_index().await?,
        })
    }
}

fn hit(chunk: StoredChunk, score: f32) -> SearchHit {
    SearchHit {
        id: chunk.id,
        score,
        payload: chunk.payload,
        document_id: chunk.document_id,
        chunk_index: chunk.chunk_index,
        path: chunk.path,
        text: chunk.text,
        section: chunk.section,
        page: chunk.page,
        keyword_score: None,
        fused_score: None,
        provenance: None,
    }
}

fn index_type_name(index_type: &IndexType) -> String {
    match index_type {
        IndexType::IvfFlat => "IVF_FLAT",
        IndexType::IvfSq => "IVF_SQ",
        IndexType::IvfPq => "IVF_PQ",
        IndexType::IvfRq => "IVF_RQ",
        IndexType::IvfHnswPq => "IVF_HNSW_PQ",
        IndexType::IvfHnswSq => ANN_INDEX_TYPE,
        IndexType::IvfHnswFlat => "IVF_HNSW_FLAT",
        _ => "OTHER",
    }
    .into()
}

pub fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn column<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> Result<&'a T> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<T>())
        .ok_or_else(|| Error::Backend(format!("Missing or invalid {name} column")))
}

fn directory_bytes(path: &Path) -> Result<u64> {
    let mut bytes = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            bytes += directory_bytes(&entry.path())?;
        } else if kind.is_file() {
            bytes += entry.metadata()?.len();
        }
    }
    Ok(bytes)
}
