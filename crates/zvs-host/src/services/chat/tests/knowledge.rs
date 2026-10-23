use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError};

use serde_json::json;
use zvs_contract::{
    ChatToolCallStatus, ConversationId, CreateConversationInput, ProviderCapability, RetrievalMode,
    RunStatus, ToolMode, VectorChunkDto, VectorDocumentDto, VectorSearchHitDto, VectorStoreDto,
    VectorStoreId,
};

use super::tool_loop::{Scripted, call, call_with_id, text};
use super::{FakeSearch, Harness, HarnessOptions, OneDriver, within};
use crate::drivers::ai::ChatMessage;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::kernel::knowledge_nodes::{SEARCH_DESCRIPTION, STORE_OUT_OF_SCOPE};
use crate::kernel::{NodeRegistry, register_core_nodes};
use crate::services::chat::{Compactable, compact_knowledge};
use crate::services::chat_tools::{HYBRID_INSTRUCTION, KNOWLEDGE_INSTRUCTION};
use crate::testing::{TemporaryDatabase, temporary_database};
use crate::tools::{ToolRegistry, ToolRegistryOptions, register_kernel_tools};

const LEAVE: &str = "0199cc11-1111-7111-8111-000000000001";
const SICK: &str = "0199cc11-1111-7111-8111-000000000002";

struct Knowledge {
    harness: Harness,
    driver: Arc<Scripted>,
    search: Arc<FakeSearch>,
    store: VectorStoreId,
    _database: TemporaryDatabase,
}

fn store_dto(id: VectorStoreId, name: &str) -> VectorStoreDto {
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "description": "Company HR policies",
        "backend": "lancedb",
        "embeddingProviderId": "0199aa11-2222-7111-8111-111111111111",
        "embeddingModelId": "bge-m3",
        "dimension": 1024,
        "metric": "cosine",
        "chunkSize": 256,
        "chunkOverlap": 32,
        "searchMode": "hybrid",
        "chatMinScore": 0.3,
        "schemaVersion": 2,
        "indexType": "FLAT",
        "status": "healthy",
        "graphStale": false,
        "lastIndexedAt": 1_700_000_000_000_i64,
        "documents": 2,
        "vectors": 4,
        "bytes": 4096,
        "createdAt": 1_700_000_000_000_i64,
        "updatedAt": 1_700_000_000_000_i64,
    }))
    .expect("a store")
}

fn hit(document: &str, chunk: u64, path: &str, body: &str) -> VectorSearchHitDto {
    serde_json::from_value(json!({
        "id": format!("{document}:{chunk}"),
        "documentId": document,
        "path": path,
        "chunkIndex": chunk,
        "score": 0.8,
        "text": body,
        "section": "Leave",
        "page": 2,
        "payload": {},
    }))
    .expect("a hit")
}

impl Knowledge {
    fn new(mode: ToolMode) -> Self {
        let database = temporary_database();
        let registry = Arc::new(NodeRegistry::new());
        register_core_nodes(&registry).expect("registers the core nodes");
        let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
        register_kernel_tools(&registry, &tools).expect("registers the kernel tools");
        let driver = Scripted::new();
        *driver.mode.lock().unwrap_or_else(PoisonError::into_inner) = mode;
        let search = FakeSearch::new(None);
        let harness = Harness::with(HarnessOptions {
            database: Some(database.shared()),
            providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
            tools: Some(tools),
            registry: Some(registry),
            search: Some(Arc::clone(&search)),
            capabilities: vec![ProviderCapability::Text],
            context_window: 16384,
            max_output: 512,
            ..HarnessOptions::default()
        });
        let store = harness.store();
        search
            .listed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(store_dto(store, "Policies"));
        let counter = Arc::new(AtomicUsize::new(0));
        search.answer(move |_| match counter.fetch_add(1, Ordering::SeqCst) {
            0 => vec![hit(
                LEAVE,
                1,
                "policies/leave.md",
                "Annual leave is 28 days.",
            )],
            _ => vec![hit(SICK, 4, "policies/sick.md", "Sick leave needs a note.")],
        });
        Self {
            harness,
            driver,
            search,
            store,
            _database: database,
        }
    }

    fn conversation(&self, mode: RetrievalMode) -> ConversationId {
        self.harness.create(CreateConversationInput {
            attached_store_ids: Some(vec![self.store]),
            retrieval_mode: Some(mode),
            ..self.harness.input()
        })
    }

    fn queries(&self) -> Vec<String> {
        self.search
            .calls()
            .into_iter()
            .map(|(store, query, _)| {
                assert_eq!(store, self.store.to_string());
                query
            })
            .collect()
    }
}

#[tokio::test]
async fn in_agentic_mode_a_greeting_searches_nothing_but_the_knowledge_tools_are_offered() {
    let test = Knowledge::new(ToolMode::Native);
    test.driver.fallback(|_| vec![text("Hello!")]);
    let conversation = test.conversation(RetrievalMode::Agentic);
    let handle = within(test.harness.turn(conversation, "Hi there")).await;
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert!(test.queries().is_empty());
    let requests = test.driver.requests();
    let names: Vec<&str> = requests[0]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "knowledge_list",
            "knowledge_read",
            "knowledge_search",
            "ask_user"
        ]
    );
    let search = requests[0]
        .tools
        .iter()
        .find(|tool| tool.name == "knowledge_search")
        .expect("search is offered");
    assert_eq!(search.description, SEARCH_DESCRIPTION);
    assert!(search.parameters["properties"].get("scope").is_none());
    let system = requests[0].system.clone().unwrap_or_default();
    assert!(system.contains(KNOWLEDGE_INSTRUCTION));
    assert!(system.contains(&format!("- {} (0 documents)", test.store)));
    assert!(test.harness.last(conversation).citations.is_empty());
}

#[tokio::test]
async fn a_comparison_searches_twice_and_citations_number_on_across_the_rounds() {
    let test = Knowledge::new(ToolMode::Native);
    test.driver.round(|_| {
        vec![call_with_id(
            "knowledge_search",
            &json!({ "query": "annual leave policy" }),
            "first",
        )]
    });
    test.driver.round(|_| {
        vec![call_with_id(
            "knowledge_search",
            &json!({ "query": "sick leave policy", "store": "policies" }),
            "second",
        )]
    });
    test.driver
        .fallback(|_| vec![text("Leave is 28 days [1]; sick leave needs a note [2].")]);
    let conversation = test.conversation(RetrievalMode::Agentic);
    let handle = within(
        test.harness
            .turn(conversation, "Compare leave with sick leave"),
    )
    .await;
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.queries(), ["annual leave policy", "sick leave policy"]);
    let assistant = test.harness.last(conversation);
    assert_eq!(
        assistant
            .citations
            .iter()
            .map(|citation| (citation.document_id.as_str(), citation.chunk_index))
            .collect::<Vec<_>>(),
        [(LEAVE, 1), (SICK, 4)]
    );
    assert!(
        assistant
            .tool_calls
            .iter()
            .all(|call| call.status == ChatToolCallStatus::Succeeded)
    );
    assert_eq!(
        assistant.tool_calls[0].arguments,
        json!({ "query": "annual leave policy" })
    );
    let requests = test.driver.requests();
    let second = requests[2]
        .messages
        .iter()
        .rev()
        .find(|message| message.tool_call_id.as_deref() == Some("second"))
        .expect("the second result");
    assert!(second.content.contains("<source id=\"2\""));
    assert!(second.content.contains("Sick leave needs a note."));
    assert!(second.content.contains(&format!("documentId=\"{SICK}\"")));
}

#[tokio::test]
async fn a_search_outside_the_attached_stores_is_refused_and_a_forged_scope_is_ignored() {
    let test = Knowledge::new(ToolMode::Native);
    let foreign = VectorStoreId::parse("0199cc22-2222-7222-8222-000000000009").expect("an id");
    test.driver.round(move |_| {
        vec![
            call_with_id(
                "knowledge_search",
                &json!({ "query": "q", "store": "Payroll" }),
                "a",
            ),
            call_with_id(
                "knowledge_search",
                &json!({ "query": "q", "scope": [foreign] }),
                "b",
            ),
        ]
    });
    test.driver.fallback(|_| vec![text("Done.")]);
    let conversation = test.conversation(RetrievalMode::Agentic);
    within(test.harness.turn(conversation, "Look it up")).await;
    let calls = test.harness.last(conversation).tool_calls;
    assert_eq!(calls[0].status, ChatToolCallStatus::Failed);
    assert_eq!(calls[0].error.as_deref(), Some(STORE_OUT_OF_SCOPE));
    assert_eq!(calls[1].status, ChatToolCallStatus::Succeeded);
    assert_eq!(test.queries(), ["q"]);
}

#[tokio::test]
async fn an_empty_search_tells_the_model_to_say_the_knowledge_base_has_no_answer() {
    let test = Knowledge::new(ToolMode::Native);
    test.search.answer(|_| Vec::new());
    test.driver.round(|_| {
        vec![call(
            "knowledge_search",
            &json!({ "query": "parking rules" }),
        )]
    });
    test.driver
        .fallback(|_| vec![text("The knowledge base does not cover parking.")]);
    let conversation = test.conversation(RetrievalMode::Agentic);
    within(
        test.harness
            .turn(conversation, "What are the parking rules?"),
    )
    .await;
    let requests = test.driver.requests();
    let result = requests[1].messages.last().expect("a tool result");
    assert!(
        result
            .content
            .contains("No passages above the relevance floor")
    );
    assert!(result.content.contains("does not contain the answer"));
    assert!(test.harness.last(conversation).citations.is_empty());
}

#[tokio::test]
async fn hybrid_mode_offers_a_few_sources_up_front_and_numbers_tool_results_after_them() {
    let test = Knowledge::new(ToolMode::Native);
    test.search.answer(|_| {
        (0..6)
            .map(|index| {
                hit(
                    &format!("0199cc11-1111-7111-8111-00000000010{index}"),
                    0,
                    &format!("doc{index}.md"),
                    "Up front.",
                )
            })
            .collect()
    });
    test.driver
        .round(|_| vec![call("knowledge_search", &json!({ "query": "more detail" }))]);
    test.driver.fallback(|_| vec![text("Answer.")]);
    let conversation = test.conversation(RetrievalMode::Hybrid);
    within(test.harness.turn(conversation, "Tell me about leave")).await;
    assert_eq!(test.queries().len(), 2);
    let requests = test.driver.requests();
    assert!(
        requests[0]
            .system
            .clone()
            .unwrap_or_default()
            .contains(HYBRID_INSTRUCTION)
    );
    assert!(!requests[0].tools.is_empty());
    let citations = test.harness.last(conversation).citations;
    assert_eq!(citations.len(), 6);
    let result = requests[1].messages.last().expect("a tool result");
    assert!(result.content.contains("<source id=\"4\""));
}

#[tokio::test]
async fn auto_mode_searches_up_front_and_offers_no_knowledge_tools() {
    let test = Knowledge::new(ToolMode::Native);
    test.driver.fallback(|_| vec![text("Answer.")]);
    let conversation = test.conversation(RetrievalMode::Auto);
    within(test.harness.turn(conversation, "Tell me about leave")).await;
    assert_eq!(test.queries().len(), 1);
    assert_eq!(test.driver.requests()[0].tools, [super::ask_spec()]);
    assert_eq!(test.harness.last(conversation).citations.len(), 1);
}

#[tokio::test]
async fn agentic_mode_without_tool_support_still_searches_up_front() {
    let test = Knowledge::new(ToolMode::None);
    test.driver.fallback(|_| vec![text("Answer.")]);
    let conversation = test.conversation(RetrievalMode::Agentic);
    within(test.harness.turn(conversation, "Tell me about leave")).await;
    assert_eq!(test.queries(), ["Tell me about leave"]);
    assert!(test.driver.requests()[0].tools.is_empty());
    assert_eq!(test.harness.last(conversation).citations.len(), 1);
}

#[tokio::test]
async fn reading_a_document_returns_the_span_and_reuses_the_number_of_a_cited_chunk() {
    let test = Knowledge::new(ToolMode::Native);
    test.search
        .documents
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(
            serde_json::from_value::<VectorDocumentDto>(json!({
                "id": LEAVE,
                "storeId": test.store,
                "sourcePath": "policies/leave.md",
                "contentHash": "hash",
                "chunkCount": 3,
                "bytes": 100,
                "indexedAt": 1,
            }))
            .expect("a document"),
        );
    *test
        .search
        .stored_chunks
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = (0..3)
        .map(|index| {
            serde_json::from_value::<VectorChunkDto>(json!({
                "id": format!("c{index}"),
                "payload": {},
                "documentId": LEAVE,
                "chunkIndex": index,
                "path": "policies/leave.md",
                "text": format!("Part {index}."),
                "section": "Leave",
                "page": 2,
            }))
            .expect("a chunk")
        })
        .collect();
    test.driver
        .round(|_| vec![call("knowledge_search", &json!({ "query": "leave" }))]);
    test.driver.round(|_| {
        vec![call(
            "knowledge_read",
            &json!({ "documentId": LEAVE, "section": "leave" }),
        )]
    });
    test.driver.fallback(|_| vec![text("All of it [1].")]);
    let conversation = test.conversation(RetrievalMode::Agentic);
    within(test.harness.turn(conversation, "Everything about leave")).await;
    let requests = test.driver.requests();
    let read = requests[2].messages.last().expect("a read result");
    assert!(read.content.contains("cite it as [1]"));
    assert!(read.content.contains("Part 0."));
    assert!(read.content.contains("Part 2."));
    assert!(read.content.contains("chunks=\"0-2 of 3\""));
    assert_eq!(test.harness.last(conversation).citations.len(), 1);
}

#[test]
fn older_knowledge_results_are_compacted_first_when_a_round_needs_room() {
    let mut messages = vec![
        ChatMessage::user("question"),
        ChatMessage::tool("a", "x".repeat(4000)),
        ChatMessage::tool("b", "y".repeat(4000)),
    ];
    let mut pending = vec![
        Compactable {
            index: 1,
            round: 0,
            compact: "[compact a]".to_owned(),
        },
        Compactable {
            index: 2,
            round: 1,
            compact: "[compact b]".to_owned(),
        },
    ];
    assert_eq!(
        compact_knowledge(&mut messages, &mut pending, 1, 100_000),
        0
    );
    assert_eq!(compact_knowledge(&mut messages, &mut pending, 1, 1500), 1);
    assert_eq!(messages[1].content, "[compact a]");
    assert_eq!(messages[2].content.len(), 4000);
    assert_eq!(compact_knowledge(&mut messages, &mut pending, 1, 10), 0);
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn the_end_to_end_evaluation_asks_each_question_per_mode_and_checks_the_citations() {
    let test = Knowledge::new(ToolMode::Native);
    test.search.answer(|_| {
        vec![hit(
            LEAVE,
            1,
            "policies/leave.md",
            "Annual leave is 28 days.",
        )]
    });
    let store = test.store.to_string();
    test.harness
        .database
        .with_connection(|connection| {
            crate::data::Repositories::new(connection)
                .vector_eval_cases
                .add(&crate::data::repositories::VectorEvalCaseDraft {
                    id: "0199cc33-3333-7333-8333-000000000001".to_owned(),
                    store_id: store.clone(),
                    question: "How long is annual leave?".to_owned(),
                    expected_document_ids: vec![LEAVE.to_owned()],
                    created_at: 1,
                })?;
            Ok(())
        })
        .expect("adds a case");
    test.driver.round(|_| vec![text("28 days [1].")]);
    test.driver.round(|_| {
        vec![call(
            "knowledge_search",
            &json!({ "query": "annual leave length" }),
        )]
    });
    test.driver.round(|_| vec![text("28 days [1].")]);
    let run = within(test.harness.chat.evaluate(&zvs_contract::ChatEvalInput {
        store_id: test.store,
        provider_id: test.harness.provider_id,
        model_id: test.harness.model_id.clone(),
        modes: None,
    }))
    .await
    .expect("evaluates");
    assert_eq!(
        run.modes.iter().map(|mode| mode.mode).collect::<Vec<_>>(),
        [RetrievalMode::Auto, RetrievalMode::Agentic]
    );
    for mode in &run.modes {
        assert_eq!(mode.hit_rate, 1.0, "{mode:?}");
        assert_eq!(mode.cases[0].cited, [LEAVE]);
        assert_eq!(mode.cases[0].error, None);
    }
    assert_eq!(run.modes[0].average_searches, 0.0);
    assert_eq!(run.modes[1].average_searches, 1.0);
    assert!(test.harness.chat.list().expect("lists").is_empty());
}
