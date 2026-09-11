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
    query::{ExecutableQuery, QueryBase},
};
use serde::{Deserialize, Serialize};

use crate::{CancellationToken, Error, Result, error::check_cancelled};

pub const DEFAULT_DIMENSION: u32 = 1024;
pub const UPSERT_BATCH_SIZE: usize = 256;

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
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
    pub payload: serde_json::Value,
    pub document_id: String,
    pub chunk_index: u32,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub row_count: usize,
    pub dimension: u32,
    pub metric: Metric,
    pub on_disk_bytes: u64,
    pub index_type: String,
}

pub struct VectorIndex {
    table: Table,
    directory: PathBuf,
    dimension: u32,
    metric: Metric,
}

fn backend(error: impl std::fmt::Display) -> Error {
    Error::Backend(error.to_string())
}

fn schema(dimension: u32, metric: Metric) -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![
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
        ],
        HashMap::from([(
            "zvs.metric".into(),
            serde_json::to_string(&metric).expect("metric serializes"),
        )]),
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

impl VectorIndex {
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
            .create_empty_table("vectors", schema(dimension, metric))
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
        if actual.fields() != schema(dimension, metric).fields() {
            return Err(Error::InvalidInput(
                "Unsupported vector index schema".into(),
            ));
        }
        Ok(Self {
            table,
            directory: path.join("vectors.lance"),
            dimension,
            metric,
        })
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

    pub async fn upsert(&self, rows: &[VectorRow], token: &CancellationToken) -> Result<usize> {
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
        for batch in rows.chunks(UPSERT_BATCH_SIZE) {
            check_cancelled(token)?;
            let data = RecordBatch::try_new(
                schema(self.dimension, self.metric),
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
                ],
            )
            .map_err(backend)?;
            let mut merge = self.table.merge_insert(&["id"]);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all();
            let reader = arrow_array::RecordBatchIterator::new(
                vec![Ok(data)],
                schema(self.dimension, self.metric),
            );
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
        self.validate_vector(vector)?;
        if k == 0 || !min_score.is_finite() || !(0.0..=1.0).contains(&min_score) {
            return Err(Error::InvalidInput(
                "Search requires positive k and minimum score in [0, 1]".into(),
            ));
        }
        let mut query = self
            .table
            .query()
            .nearest_to(vector)
            .map_err(backend)?
            .distance_type(self.metric.distance())
            .bypass_vector_index()
            .limit(k);
        if let Some(filter) = filter {
            query = query.only_if(filter);
        }
        let batches: Vec<RecordBatch> = query
            .execute()
            .await
            .map_err(backend)?
            .try_collect()
            .await
            .map_err(backend)?;
        let mut hits = Vec::new();
        for batch in batches {
            let ids = column::<StringArray>(&batch, "id")?;
            let payloads = column::<StringArray>(&batch, "payload")?;
            let documents = column::<StringArray>(&batch, "document_id")?;
            let chunks = column::<UInt32Array>(&batch, "chunk_index")?;
            let paths = column::<StringArray>(&batch, "path")?;
            let distances = column::<Float32Array>(&batch, "_distance")?;
            for row in 0..batch.num_rows() {
                let score = self.metric.score(distances.value(row));
                if !score.is_finite() {
                    return Err(Error::Backend("Search returned a non-finite score".into()));
                }
                if score >= min_score {
                    hits.push(SearchHit {
                        id: ids.value(row).into(),
                        score,
                        payload: serde_json::from_str(payloads.value(row)).map_err(backend)?,
                        document_id: documents.value(row).into(),
                        chunk_index: chunks.value(row),
                        path: paths.value(row).into(),
                    });
                }
            }
        }
        Ok(hits)
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
                    batch
                        .iter()
                        .map(|id| literal(id))
                        .collect::<Vec<_>>()
                        .join(",")
                ))
                .await
                .map_err(backend)?;
        }
        Ok(())
    }

    pub async fn stats(&self) -> Result<IndexStats> {
        Ok(IndexStats {
            row_count: self.table.count_rows(None).await.map_err(backend)?,
            dimension: self.dimension,
            metric: self.metric,
            on_disk_bytes: directory_bytes(&self.directory)?,
            index_type: "FLAT".into(),
        })
    }
}

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
