use serde_json::json;

use super::*;

fn chunk(index: u64, section: &str) -> VectorChunkDto {
    serde_json::from_value(json!({
        "id": format!("c{index}"),
        "payload": {},
        "documentId": "d",
        "chunkIndex": index,
        "path": "a.md",
        "text": format!("t{index}"),
        "section": section,
        "page": null,
    }))
    .expect("a chunk")
}

fn store(id: &str, name: &str) -> VectorStoreDto {
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "description": "",
        "backend": "lancedb",
        "embeddingProviderId": "0199aa11-2222-7111-8111-111111111111",
        "embeddingModelId": "bge-m3",
        "dimension": 8,
        "metric": "cosine",
        "chunkSize": 256,
        "chunkOverlap": 32,
        "schemaVersion": 2,
        "indexType": "FLAT",
        "status": "healthy",
        "graphStale": false,
        "lastIndexedAt": null,
        "documents": 0,
        "vectors": 0,
        "bytes": 0,
        "createdAt": 1,
        "updatedAt": 1,
    }))
    .expect("a store")
}

#[test]
fn the_read_window_stays_inside_the_document_and_the_chunk_cap() {
    assert_eq!(read_window(None, None, 0), None);
    assert_eq!(read_window(None, None, 5), Some((0, 4)));
    assert_eq!(read_window(None, None, 40), Some((0, READ_MAX_CHUNKS - 1)));
    assert_eq!(read_window(Some(3), Some(1), 10), Some((3, 3)));
    assert_eq!(read_window(Some(30), None, 10), Some((9, 9)));
    assert_eq!(read_window(Some(2), Some(100), 40), Some((2, 13)));
}

#[test]
fn a_section_span_is_the_first_contiguous_run_of_the_matching_section() {
    let chunks = vec![
        chunk(0, "Intro"),
        chunk(1, "Leave › Annual"),
        chunk(2, "Leave › Annual"),
        chunk(3, "Sick"),
        chunk(4, "Leave › Annual"),
    ];
    assert_eq!(section_span(&chunks, "annual"), Some((1, 2)));
    assert_eq!(section_span(&chunks, "sick"), Some((3, 3)));
    assert_eq!(section_span(&chunks, "payroll"), None);
    assert_eq!(section_span(&chunks, "  "), None);
}

#[test]
fn a_named_store_must_be_one_of_the_attached_stores() {
    let first = "0199aa11-1111-7111-8111-000000000001";
    let stores = vec![
        store(first, "Policies"),
        store("0199aa11-1111-7111-8111-000000000002", "Wiki"),
    ];
    assert_eq!(pick_stores(stores.clone(), None).expect("all").len(), 2);
    assert_eq!(
        pick_stores(stores.clone(), Some("policies")).expect("by name")[0].name,
        "Policies"
    );
    assert_eq!(
        pick_stores(stores.clone(), Some(first))
            .expect("by id")
            .len(),
        1
    );
    let refused = pick_stores(stores, Some("Payroll")).expect_err("not attached");
    assert_eq!(refused.code, AppErrorCode::PermissionDenied);
    assert_eq!(refused.message, STORE_OUT_OF_SCOPE);
}
