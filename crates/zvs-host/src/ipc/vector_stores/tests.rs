use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::VectorSourceKind;

use super::*;
use crate::data::Repositories;
use crate::data::repositories::VectorDocumentDraft;
use crate::drivers::core::{VectorCore, VectorRow};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::ids::new_id;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{FakeDriver, TestServices, seed_embedding_provider, test_services};

fn server() -> (TestServices, IpcServer, String) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    host.drivers
        .set(FakeDriver::with_vectors(vec![vec![1.0, 0.0, 0.0]]));
    let provider = host
        .database
        .with_connection(|connection| Ok(seed_embedding_provider(connection)))
        .expect("seeds");
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    (host, server, provider)
}

async fn call(server: &IpcServer, channel: &str, payload: Value) -> Value {
    let response = server.dispatch(channel, payload).await;
    assert_eq!(response["ok"], json!(true), "{channel}: {response}");
    response["data"].clone()
}

async fn fail(server: &IpcServer, channel: &str, payload: Value) -> Value {
    let response = server.dispatch(channel, payload).await;
    assert_eq!(response["ok"], json!(false), "{channel}: {response}");
    response["error"].clone()
}

fn document(host: &TestServices, store_id: &str, path: &str) -> String {
    host.database
        .with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_documents
                .record_indexed(&VectorDocumentDraft {
                    id: new_id().hyphenated().to_string(),
                    store_id: store_id.to_owned(),
                    source_path: path.to_owned(),
                    content_hash: path.to_owned(),
                    chunk_count: 1,
                    bytes: 10,
                    indexed_at: 1,
                })?
                .id)
        })
        .expect("records")
}

fn row(id: &str, document_id: &str, vector: [f32; 3], text: &str) -> VectorRow {
    VectorRow {
        id: id.to_owned(),
        vector: vector.to_vec(),
        payload: Value::Null,
        document_id: document_id.to_owned(),
        chunk_index: 0,
        path: format!("{id}.md"),
        text: text.to_owned(),
        section: String::new(),
        page: None,
    }
}

async fn create(server: &IpcServer, provider: &str, name: &str) -> String {
    call(
        server,
        "vectorStores.create",
        json!({ "name": name, "embeddingProviderId": provider, "embeddingModelId": "embed-model", "dimension": 3 }),
    )
    .await["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

#[tokio::test]
async fn channels_cover_crud_and_reconciliation_with_validated_dtos_and_immutable_index_configuration()
 {
    let (_host, server, provider) = server();
    let store = call(
        &server,
        "vectorStores.create",
        json!({ "name": "Knowledge", "embeddingProviderId": provider, "embeddingModelId": "embed-model", "dimension": 3 }),
    )
    .await;
    assert!(store.get("tableCreatedAt").is_none());
    assert!(store.get("lastEval").is_none());
    assert_eq!(store["status"], json!("healthy"));
    assert_eq!(store["schemaVersion"], json!(2));
    let id = store["id"].clone();
    assert_eq!(
        call(&server, "vectorStores.list", Value::Null)
            .await
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        call(
            &server,
            "vectorStores.update",
            json!({ "id": id, "name": "New" })
        )
        .await["name"],
        json!("New")
    );
    assert_eq!(
        call(&server, "vectorStores.get", json!({ "id": id })).await["status"],
        json!("healthy")
    );
    assert_eq!(
        call(&server, "vectorStores.reconcile", json!({ "id": id })).await["status"],
        json!("healthy")
    );
    assert_eq!(
        call(
            &server,
            "vectorStores.search",
            json!({ "storeId": id, "query": "q", "k": 3, "minScore": 0 })
        )
        .await,
        json!([])
    );
    assert_eq!(
        call(
            &server,
            "vectorStores.searchTimed",
            json!({ "storeId": id, "query": "q" })
        )
        .await["mode"],
        json!("hybrid")
    );
    assert_eq!(
        fail(
            &server,
            "vectorStores.create",
            json!({ "name": "New", "embeddingProviderId": provider, "embeddingModelId": "m" })
        )
        .await["code"],
        json!("CONFLICT")
    );
    assert_eq!(
        fail(
            &server,
            "vectorStores.create",
            json!({ "name": "Q", "embeddingProviderId": provider, "embeddingModelId": "m", "backend": "qdrant" })
        )
        .await["code"],
        json!("UNSUPPORTED_FORMAT")
    );
    assert_eq!(
        fail(
            &server,
            "vectorStores.update",
            json!({ "id": id, "chunkOverlap": 999 })
        )
        .await["code"],
        json!("VALIDATION_FAILED")
    );
    assert_eq!(
        fail(
            &server,
            "vectorStores.create",
            json!({ "name": "X", "embeddingProviderId": provider, "embeddingModelId": "m", "chunkOverlap": 999 })
        )
        .await["message"],
        json!(zvs_contract::VALIDATION_MESSAGE)
    );
    assert_eq!(
        call(&server, "vectorStores.remove", json!({ "id": id })).await,
        json!({ "id": id, "removed": true })
    );
    assert_eq!(
        fail(&server, "vectorStores.get", json!({ "id": id })).await["code"],
        json!("NOT_FOUND")
    );
}

#[tokio::test]
async fn search_hits_reach_the_renderer_in_the_contract_shape() {
    let (host, server, provider) = server();
    let id = create(&server, &provider, "Native").await;
    let path = host.vector_stores.store_path(&id).expect("a path");
    host.core
        .upsert(
            &path,
            vec![
                row("a", "doc-a", [1.0, 0.0, 0.0], "A"),
                row("b", "doc-a", [0.9, 0.1, 0.0], "B"),
                row("c", "doc-b", [0.0, 1.0, 0.0], "C"),
            ],
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .expect("upserts");
    let hits = call(
        &server,
        "vectorStores.search",
        json!({ "storeId": id, "query": "A", "k": 2, "minScore": 0.5 }),
    )
    .await;
    assert_eq!(
        hits.as_array()
            .map(|hits| hits.iter().map(|hit| hit["id"].clone()).collect::<Vec<_>>()),
        Some(vec![json!("a"), json!("b")])
    );
    assert_eq!(hits[0]["text"], json!("A"));
    assert_eq!(hits[0]["provenance"], json!("both"));
    let timed = call(
        &server,
        "vectorStores.searchTimed",
        json!({ "storeId": id, "query": "A", "k": 2, "minScore": 0.5 }),
    )
    .await;
    assert_eq!(timed["hits"], hits);
    let vector_only = call(
        &server,
        "vectorStores.searchTimed",
        json!({ "storeId": id, "query": "C", "k": 1, "minScore": 0, "mode": "vector" }),
    )
    .await;
    assert_eq!(vector_only["mode"], json!("vector"));
    assert!(vector_only["hits"][0].get("provenance").is_none());
}

#[tokio::test]
async fn sources_documents_chunks_and_the_picker_are_served() {
    let (host, server, provider) = server();
    let id = create(&server, &provider, "K").await;
    let folder = std::env::temp_dir().join("zvs-sources");
    let source = call(
        &server,
        "vectorStores.sources.add",
        json!({ "storeId": id, "path": format!("  {}  ", folder.display()), "include": [" *.md "] }),
    )
    .await;
    assert_eq!(source["kind"], json!("folder"));
    assert_eq!(source["recursive"], json!(true));
    assert_eq!(source["path"], json!(folder.to_string_lossy()));
    assert_eq!(source["include"], json!(["*.md"]));
    assert_eq!(
        call(&server, "vectorStores.sources.list", json!({ "id": id }))
            .await
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        call(
            &server,
            "vectorStores.sources.remove",
            json!({ "id": source["id"] })
        )
        .await,
        json!({ "id": source["id"], "removed": true })
    );
    assert_eq!(
        fail(
            &server,
            "vectorStores.sources.remove",
            json!({ "id": source["id"] })
        )
        .await["code"],
        json!("NOT_FOUND")
    );

    host.picker.answer(&["C:/docs"]);
    assert_eq!(
        call(
            &server,
            "vectorStores.sources.pick",
            json!({ "kind": "file" })
        )
        .await,
        json!({ "paths": ["C:/docs"] })
    );
    assert_eq!(host.picker.asked(), [VectorSourceKind::File]);

    let kept = document(&host, &id, "a.md");
    let gone = document(&host, &id, "b.md");
    let path = host.vector_stores.store_path(&id).expect("a path");
    host.core
        .upsert(
            &path,
            vec![
                row("a:0", &kept, [1.0, 0.0, 0.0], "kept"),
                row("b:0", &gone, [0.0, 1.0, 0.0], "gone"),
            ],
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .expect("upserts");
    assert_eq!(
        call(
            &server,
            "vectorStores.chunks",
            json!({ "storeId": id, "documentId": kept, "indices": [0] })
        )
        .await[0]["text"],
        json!("kept")
    );
    assert_eq!(
        call(&server, "vectorStores.documents.list", json!({ "id": id }))
            .await
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        call(
            &server,
            "vectorStores.documents.remove",
            json!({ "storeId": id, "id": gone })
        )
        .await,
        json!({ "id": gone, "removed": true })
    );
    let documents = call(&server, "vectorStores.documents.list", json!({ "id": id })).await;
    assert_eq!(documents.as_array().map(Vec::len), Some(1));
    assert_eq!(documents[0]["id"], json!(kept));
    assert_eq!(
        call(&server, "vectorStores.get", json!({ "id": id })).await["vectors"],
        json!(1)
    );
    assert_eq!(
        fail(
            &server,
            "vectorStores.documents.remove",
            json!({ "storeId": id, "id": gone })
        )
        .await["message"],
        json!("Документ не найден")
    );
    assert_eq!(
        call(&server, "vectorStores.clear", json!({ "id": id })).await["status"],
        json!("pending")
    );
    assert_eq!(
        call(&server, "vectorStores.documents.list", json!({ "id": id })).await,
        json!([])
    );
}

#[tokio::test]
async fn the_eval_channels_round_trip_through_the_contract() {
    let (host, server, provider) = server();
    let id = create(&server, &provider, "K").await;
    let cats = document(&host, &id, "C:/docs/cats.md");
    let added = call(
        &server,
        "vectorStores.eval.add",
        json!({ "storeId": id, "question": "cats", "expectedDocumentIds": [cats] }),
    )
    .await;
    assert_eq!(added["question"], json!("cats"));
    let set = call(&server, "vectorStores.eval.list", json!({ "id": id })).await;
    assert_eq!(set["cases"].as_array().map(Vec::len), Some(1));
    assert_eq!(set["lastRun"], Value::Null);
    let run = call(&server, "vectorStores.eval.run", json!({ "storeId": id })).await;
    assert_eq!(run["k"], json!(10));
    assert_eq!(
        run["modes"].as_array().map(|modes| modes
            .iter()
            .map(|mode| mode["mode"].clone())
            .collect::<Vec<_>>()),
        Some(vec![json!("vector"), json!("hybrid")])
    );
    assert_eq!(
        call(&server, "vectorStores.eval.list", json!({ "id": id })).await["lastRun"],
        run
    );
    assert_eq!(
        call(
            &server,
            "vectorStores.eval.remove",
            json!({ "storeId": id, "id": added["id"] })
        )
        .await,
        json!({ "id": added["id"], "removed": true })
    );
}

#[tokio::test]
async fn the_index_channel_starts_an_indexing_run_that_fills_the_documents_tab() {
    let (host, server, provider) = server();
    let error = fail(
        &server,
        "vectorStores.index",
        json!({ "storeId": "0199aa11-1111-7111-8111-111111111111" }),
    )
    .await;
    assert_eq!(error["code"], json!("NOT_FOUND"));

    let id = create(&server, &provider, "Indexed").await;
    let corpus = host.workspace.join("corpus");
    std::fs::create_dir_all(&corpus).expect("creates");
    std::fs::write(corpus.join("guide.md"), "alpha beta gamma").expect("writes");
    call(
        &server,
        "vectorStores.sources.add",
        json!({ "storeId": id, "path": corpus.to_string_lossy() }),
    )
    .await;

    let handle = call(&server, "vectorStores.index", json!({ "storeId": id })).await;
    let run_id =
        zvs_contract::RunId::parse(handle["id"].as_str().expect("an id")).expect("a run id");
    host.runs.wait(&run_id).await;
    let run = host.runs.get(&run_id).expect("a run");
    assert_eq!(run.kind, zvs_contract::RunKind::Indexing);
    assert_eq!(run.title.as_deref(), Some("Индексация «Indexed»"));
    assert_eq!(run.status, zvs_contract::RunStatus::Succeeded);
    let steps = host.runs.steps(&run_id).expect("steps");
    assert_eq!(steps[0].step_type, "vector.index");
    assert_eq!(
        steps[0].output.as_ref().expect("an output")["indexed"],
        json!(1)
    );

    let documents = call(&server, "vectorStores.documents.list", json!({ "id": id })).await;
    assert_eq!(
        documents[0]["sourcePath"],
        json!(corpus.join("guide.md").to_string_lossy())
    );
}
