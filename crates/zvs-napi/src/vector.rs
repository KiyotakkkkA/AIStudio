use std::{
    collections::HashMap,
    panic::AssertUnwindSafe,
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use futures::FutureExt;
use napi_derive::napi;
use serde::Deserialize;
use zvs_core::{
    CancellationToken, Error,
    index::{Metric, VectorIndex, VectorRow},
};

use crate::{CoreFailure, guarded, native_error};

static TOKENS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn tokens() -> &'static Mutex<HashMap<String, CancellationToken>> {
    TOKENS.get_or_init(Mutex::default)
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Create {
        path: String,
        dimension: u32,
        #[serde(default)]
        metric: Metric,
    },
    Open {
        path: String,
    },
    Upsert {
        path: String,
        rows: Vec<VectorRow>,
        #[serde(rename = "operationId")]
        operation_id: String,
    },
    Search {
        path: String,
        vector: Vec<f32>,
        k: usize,
        #[serde(rename = "minScore")]
        min_score: f32,
        filter: Option<String>,
    },
    DeleteByIds {
        path: String,
        ids: Vec<String>,
    },
    DeleteBySource {
        path: String,
        #[serde(rename = "documentId")]
        document_id: String,
    },
    Stats {
        path: String,
    },
}

struct Operation(String);

impl Drop for Operation {
    fn drop(&mut self) {
        tokens()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

async fn execute(request: Request) -> zvs_core::Result<serde_json::Value> {
    match request {
        Request::Create {
            path,
            dimension,
            metric,
        } => {
            let index = VectorIndex::create(Path::new(&path), dimension, metric).await?;
            serde_json::to_value(index.stats().await?).map_err(|e| Error::Backend(e.to_string()))
        }
        Request::Open { path } | Request::Stats { path } => {
            let index = VectorIndex::open(Path::new(&path)).await?;
            serde_json::to_value(index.stats().await?).map_err(|e| Error::Backend(e.to_string()))
        }
        Request::Upsert {
            path,
            rows,
            operation_id,
        } => {
            let _operation = Operation(operation_id.clone());
            let token = tokens()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&operation_id)
                .cloned()
                .ok_or_else(|| Error::InvalidInput("Unknown upsert operation".into()))?;
            let index = VectorIndex::open(Path::new(&path)).await?;
            Ok(serde_json::json!(index.upsert(&rows, &token).await?))
        }
        Request::Search {
            path,
            vector,
            k,
            min_score,
            filter,
        } => {
            let index = VectorIndex::open(Path::new(&path)).await?;
            serde_json::to_value(
                index
                    .search(&vector, k, min_score, filter.as_deref())
                    .await?,
            )
            .map_err(|e| Error::Backend(e.to_string()))
        }
        Request::DeleteByIds { path, ids } => {
            VectorIndex::open(Path::new(&path))
                .await?
                .delete_by_ids(&ids)
                .await?;
            Ok(serde_json::Value::Null)
        }
        Request::DeleteBySource { path, document_id } => {
            VectorIndex::open(Path::new(&path))
                .await?
                .delete_by_source(&document_id)
                .await?;
            Ok(serde_json::Value::Null)
        }
    }
}

#[napi]
pub async fn vector_call(request: String) -> napi::Result<String> {
    match AssertUnwindSafe(async {
        let request =
            serde_json::from_str(&request).map_err(|e| Error::InvalidInput(e.to_string()))?;
        Ok::<_, Error>(execute(request).await?.to_string())
    })
    .catch_unwind()
    .await
    {
        Ok(result) => result.map_err(|error| CoreFailure(error).into()),
        Err(_) => Err(native_error(
            "NATIVE_ERROR",
            "Rust vector operation panicked",
        )),
    }
}

#[napi]
pub async fn vector_begin_upsert() -> napi::Result<String> {
    guarded(|| {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed).to_string();
        tokens()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), CancellationToken::new());
        Ok(id)
    })
}

#[napi]
pub async fn vector_cancel(operation_id: String) -> napi::Result<()> {
    guarded(|| {
        if let Some(token) = tokens()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&operation_id)
        {
            token.cancel();
        }
        Ok(())
    })
}

#[napi]
pub async fn vector_release(operation_id: String) -> napi::Result<()> {
    guarded(|| {
        if let Some(token) = tokens()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&operation_id)
        {
            token.cancel();
        }
        Ok(())
    })
}
