use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::{VectorDocumentDraft, VectorStoreDraft};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::ids::new_id;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TestServices, seed_embedding_provider, test_services};

const CITES: &str = "01999a00-0001-7000-8000-000000000001";

fn server() -> (TestServices, IpcServer, String, Vec<String>) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let store = new_id().hyphenated().to_string();
    let documents = host
        .database
        .with_connection(|connection| {
            let provider = seed_embedding_provider(connection);
            let repositories = Repositories::new(connection);
            repositories
                .vector_stores
                .create(&VectorStoreDraft::new(&store, "K", &provider, "m", 3, 1))?;
            ["a.md", "b.md"]
                .iter()
                .map(|path| {
                    Ok(repositories
                        .vector_documents
                        .record_indexed(&VectorDocumentDraft {
                            id: new_id().hyphenated().to_string(),
                            store_id: store.clone(),
                            source_path: (*path).to_owned(),
                            content_hash: (*path).to_owned(),
                            chunk_count: 1,
                            bytes: 1,
                            indexed_at: 1,
                        })?
                        .id)
                })
                .collect::<Result<Vec<_>, crate::data::DataError>>()
        })
        .expect("seeds");
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    (host, server, store, documents)
}

async fn call(server: &IpcServer, channel: &str, payload: Value) -> Value {
    let response = server.dispatch(channel, payload).await;
    assert_eq!(response["ok"], json!(true), "{channel}: {response}");
    response["data"].clone()
}

#[tokio::test]
async fn handlers_map_the_graph_channels_onto_the_service() {
    let (_host, server, store, documents) = server();
    assert_eq!(
        call(&server, "graphRelations.list", Value::Null)
            .await
            .as_array()
            .map(Vec::len),
        Some(4)
    );
    let key = json!({
        "storeId": store,
        "fromDocumentId": documents[0],
        "toDocumentId": documents[1],
        "relationTypeId": CITES,
    });
    let mut weighted = key.clone();
    weighted["weight"] = json!(1);
    let edge = call(&server, "graphRelations.addEdge", weighted).await;
    assert_eq!(edge["weight"], json!(1.0));
    assert_eq!(edge["metadata"], Value::Null);
    assert_eq!(
        call(&server, "graphRelations.listEdges", json!({ "id": store }))
            .await
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        call(
            &server,
            "graphRelations.shortestPath",
            json!({ "storeId": store, "fromDocumentId": documents[0], "toDocumentId": documents[1], "direction": "both" })
        )
        .await["edges"][0]["relation"],
        json!("cites")
    );
    assert_eq!(
        call(
            &server,
            "graphRelations.neighbors",
            json!({ "storeId": store, "documentId": documents[1], "direction": "incoming" })
        )
        .await[0]["node"]["id"],
        json!(documents[0])
    );
    assert_eq!(
        call(
            &server,
            "graphRelations.kHopSubgraph",
            json!({ "storeId": store, "documentId": documents[0], "hops": 1 })
        )
        .await["nodes"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        call(&server, "graphRelations.reconcile", json!({ "id": store })).await["graphStale"],
        json!(false)
    );
    assert_eq!(
        call(&server, "graphRelations.removeEdge", key).await,
        json!({ "id": edge["id"], "removed": true })
    );
    assert_eq!(
        call(
            &server,
            "graphRelations.shortestPath",
            json!({ "storeId": store, "fromDocumentId": documents[0], "toDocumentId": documents[1] })
        )
        .await,
        Value::Null
    );
}
