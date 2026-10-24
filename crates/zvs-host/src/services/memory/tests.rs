use std::sync::Arc;

use serde_json::json;
use zvs_contract::{AppErrorCode, ChatSettings, ProviderCapability, ProviderKind, RetrievalMode};

use super::*;
use crate::data::repositories::ProviderDraft;
use crate::platform::clock::FakeClock;
use crate::testing::{TemporaryDatabase, temporary_database};

const CHAT: &str = "0199aa11-2222-7111-8111-111111111111";
const MISSING_CHAT: &str = "0199aa11-2222-7111-8111-999999999999";

struct Fixture {
    _database: TemporaryDatabase,
    database: Arc<Database>,
    clock: Arc<FakeClock>,
    service: MemoryService,
}

fn fixture() -> Fixture {
    let temporary = temporary_database();
    let database = temporary.shared();
    database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let provider = repositories
                .providers
                .create(&ProviderDraft::new(
                    ProviderKind::Ollama,
                    "Fake",
                    "http://localhost:11434",
                    vec![ProviderCapability::Text],
                    1,
                ))?
                .id;
            repositories.chat.create(&ConversationRow {
                id: CHAT.to_owned(),
                title: "Планы".to_owned(),
                provider_id: provider,
                model_id: "m".to_owned(),
                settings: ChatSettings::default(),
                system_prompt: String::new(),
                attached_store_ids: Vec::new(),
                use_relations: false,
                enabled_tools: Vec::new(),
                retrieval_mode: RetrievalMode::Hybrid,
                working_folder: None,
                turn_folder: None,
                profile: String::new(),
                created_at: 1,
                updated_at: 1,
            })?;
            Ok(())
        })
        .expect("a dialog");
    let clock = Arc::new(FakeClock::new(1_000));
    let service = MemoryService::new(Arc::clone(&database), clock.clone());
    Fixture {
        _database: temporary,
        database,
        clock,
        service,
    }
}

fn account(content: &str) -> CreateMemoryInput {
    CreateMemoryInput {
        scope: MemoryScope::Account,
        conversation_id: None,
        content: content.to_owned(),
    }
}

fn dialog(content: &str) -> CreateMemoryInput {
    CreateMemoryInput {
        scope: MemoryScope::Conversation,
        conversation_id: ConversationId::parse(CHAT),
        content: content.to_owned(),
    }
}

#[test]
fn facts_are_normalised_and_listed_with_their_dialog() {
    let fixture = fixture();
    let saved = fixture
        .service
        .create(&account("  Пишет   на\nRust  "))
        .expect("saved");
    assert_eq!(saved.content, "Пишет на Rust");
    assert_eq!(saved.source, MemorySource::User);
    fixture.clock.advance(5);
    let local = fixture
        .service
        .create(&dialog("Ветка main"))
        .expect("saved");
    assert_eq!(
        local.conversation.as_ref().map(|chat| chat.title.as_str()),
        Some("Планы")
    );
    let listed = fixture.service.list().expect("listed");
    assert_eq!(
        listed
            .iter()
            .map(|memory| memory.content.as_str())
            .collect::<Vec<_>>(),
        ["Ветка main", "Пишет на Rust"]
    );
}

#[test]
fn a_repeated_fact_refreshes_the_existing_row_in_its_scope() {
    let fixture = fixture();
    let first = fixture
        .service
        .save(
            MemoryScope::Account,
            None,
            "Отвечать кратко",
            MemorySource::Model,
        )
        .expect("saved");
    assert!(first.created);
    fixture.clock.advance(10);
    let again = fixture
        .service
        .save(
            MemoryScope::Account,
            None,
            "отвечать  КРАТКО",
            MemorySource::Model,
        )
        .expect("saved");
    assert!(!again.created);
    assert_eq!(again.row.id, first.row.id);
    assert_eq!(again.row.content, "Отвечать кратко");
    assert_eq!(again.row.updated_at, 1_010);
    let elsewhere = fixture
        .service
        .save(
            MemoryScope::Conversation,
            Some(CHAT),
            "Отвечать кратко",
            MemorySource::Model,
        )
        .expect("saved");
    assert!(elsewhere.created);
}

#[test]
fn each_scope_is_capped() {
    let fixture = fixture();
    for index in 0..MEMORY_CONVERSATION_MAX {
        fixture
            .service
            .create(&dialog(&format!("Факт {index}")))
            .expect("saved");
    }
    let error = fixture
        .service
        .create(&dialog("Ещё один"))
        .expect_err("full");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, CONVERSATION_MEMORY_FULL);
    fixture
        .service
        .create(&account("Общий факт"))
        .expect("the account scope is separate");
}

#[test]
fn a_dialog_fact_needs_an_existing_dialog() {
    let fixture = fixture();
    let error = fixture
        .service
        .create(&CreateMemoryInput {
            conversation_id: ConversationId::parse(MISSING_CHAT),
            ..dialog("Ветка main")
        })
        .expect_err("no dialog");
    assert_eq!(error.code, AppErrorCode::NotFound);
    let error = fixture
        .service
        .save(
            MemoryScope::Conversation,
            None,
            "Ветка main",
            MemorySource::Model,
        )
        .expect_err("no dialog");
    assert_eq!(error.code, AppErrorCode::NotFound);
}

#[test]
fn update_remove_and_clear() {
    let fixture = fixture();
    let saved = fixture
        .service
        .create(&account("Пишет на Go"))
        .expect("saved");
    fixture
        .service
        .create(&dialog("Ветка main"))
        .expect("saved");
    let updated = fixture
        .service
        .update(&UpdateMemoryInput {
            id: saved.id,
            content: "Пишет на Rust".to_owned(),
        })
        .expect("updated");
    assert_eq!(updated.content, "Пишет на Rust");
    let missing = MemoryId::parse("0199aa11-7777-7111-8111-999999999999").expect("an id");
    let error = fixture
        .service
        .update(&UpdateMemoryInput {
            id: missing,
            content: "x".to_owned(),
        })
        .expect_err("unknown");
    assert_eq!(error.code, AppErrorCode::NotFound);
    assert_eq!(
        fixture
            .service
            .remove(&missing.to_string())
            .expect_err("unknown")
            .code,
        AppErrorCode::NotFound
    );
    fixture
        .service
        .clear(&ClearMemoryInput {
            scope: MemoryScope::Conversation,
            conversation_id: ConversationId::parse(CHAT),
        })
        .expect("cleared");
    let snapshot = fixture.service.snapshot(CHAT).expect("a snapshot");
    assert!(snapshot.conversation.is_empty());
    assert_eq!(snapshot.account.len(), 1);
    fixture
        .service
        .remove(&saved.id.to_string())
        .expect("removed");
    assert!(fixture.service.list().expect("listed").is_empty());
}

#[test]
fn switches_default_on_and_follow_the_settings() {
    let fixture = fixture();
    assert_eq!(
        fixture.service.switches().expect("switches"),
        MemorySwitches {
            enabled: true,
            model: true
        }
    );
    fixture
        .database
        .with_connection(|connection| {
            Repositories::new(connection).settings.set(
                MEMORY_MODEL_KEY,
                &json!(false).to_string(),
                1,
            )?;
            Ok(())
        })
        .expect("a setting");
    assert!(!fixture.service.switches().expect("switches").model);
}
