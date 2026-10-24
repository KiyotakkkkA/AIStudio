use std::sync::Arc;

use serde_json::json;
use zvs_contract::{ChatSettings, ProviderCapability, ProviderKind, RetrievalMode};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::ProviderDraft;
use crate::data::schema::ConversationRow;
use crate::platform::clock::FakeClock;
use crate::testing::{TemporaryDatabase, temporary_database};

const CHAT: &str = "0199aa11-2222-7111-8111-111111111111";

fn service() -> (TemporaryDatabase, MemoryService) {
    let database = temporary_database();
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
    let service = MemoryService::new(database.shared(), Arc::new(FakeClock::new(1_000)));
    (database, service)
}

fn row(id: &str, scope: MemoryScope, content: &str) -> MemoryRow {
    MemoryRow {
        id: id.to_owned(),
        scope,
        conversation_id: (scope == MemoryScope::Conversation).then(|| CHAT.to_owned()),
        content: content.to_owned(),
        source: MemorySource::User,
        created_at: 1,
        updated_at: 1,
    }
}

#[test]
fn the_section_lists_both_scopes_with_refs_and_the_rules() {
    let snapshot = MemorySnapshot {
        account: vec![
            row("x1", MemoryScope::Account, "Пишет на Rust"),
            row("x2", MemoryScope::Account, "Отвечать кратко"),
        ],
        conversation: vec![row("y1", MemoryScope::Conversation, "Ветка main")],
    };
    let (section, refs) = memory_section(&snapshot, Some("memory"));
    assert!(section.starts_with(&format!("{MEMORY_HEADING}\n{MEMORY_INTRO}")));
    assert!(section.contains(MEMORY_RULES));
    assert!(section.contains("The memory tool is memory."));
    assert!(section.contains(&format!(
        "{ACCOUNT_LABEL}\n<memory>\n[a1] Пишет на Rust\n[a2] Отвечать кратко\n</memory>"
    )));
    assert!(section.contains(&format!(
        "{CONVERSATION_LABEL}\n<memory>\n[d1] Ветка main\n</memory>"
    )));
    assert_eq!(refs.id("a2"), Some("x2"));
    assert_eq!(refs.id("[D1]"), Some("y1"));
}

#[test]
fn read_only_memory_omits_the_rules_and_an_empty_one_is_left_out() {
    let snapshot = MemorySnapshot {
        account: vec![row("x1", MemoryScope::Account, "Пишет на Rust")],
        conversation: Vec::new(),
    };
    let (section, _) = memory_section(&snapshot, None);
    assert!(section.contains("[a1] Пишет на Rust"));
    assert!(!section.contains(MEMORY_RULES));
    assert_eq!(memory_section(&MemorySnapshot::default(), None).0, "");
    let (empty, _) = memory_section(&MemorySnapshot::default(), Some("memory"));
    assert!(empty.contains(MEMORY_EMPTY));
}

#[test]
fn facts_beyond_the_size_cap_are_counted_not_shown() {
    let long = "я".repeat(MEMORY_CONTENT_MAX);
    let account: Vec<MemoryRow> = (0..20)
        .map(|index| row(&format!("x{index}"), MemoryScope::Account, &long))
        .collect();
    let snapshot = MemorySnapshot {
        account,
        conversation: vec![row("y1", MemoryScope::Conversation, "Ветка main")],
    };
    let (section, refs) = memory_section(&snapshot, Some("memory"));
    let shown = (MEMORY_PROMPT_CHARS - "Ветка main".chars().count()) / MEMORY_CONTENT_MAX;
    assert_eq!(refs.entries.len(), shown + 1);
    assert!(section.contains("[d1] Ветка main"));
    assert!(section.contains(&format!("{} older facts are not shown", 20 - shown)));
}

#[test]
fn the_model_saves_updates_and_forgets_through_refs() {
    let (_database, service) = service();
    let mut refs = MemoryRefs::default();
    let saved = apply_memory_call(
        &service,
        &json!({ "action": "save", "scope": "account", "content": "Пишет на Go" }),
        CHAT,
        &mut refs,
    );
    assert_eq!(saved.as_deref(), Ok("Saved as [a1]."));
    let local = apply_memory_call(
        &service,
        &json!({ "action": "save", "scope": "conversation", "content": "Ветка main" }),
        CHAT,
        &mut refs,
    );
    assert_eq!(local.as_deref(), Ok("Saved as [d1]."));
    let again = apply_memory_call(
        &service,
        &json!({ "action": "save", "content": "пишет на go" }),
        CHAT,
        &mut refs,
    );
    assert_eq!(again.as_deref(), Ok("Already in memory as [a1]."));
    assert_eq!(
        apply_memory_call(
            &service,
            &json!({ "action": "update", "ref": "a1", "content": "Пишет на Rust" }),
            CHAT,
            &mut refs,
        )
        .as_deref(),
        Ok("Updated [a1].")
    );
    assert_eq!(
        apply_memory_call(
            &service,
            &json!({ "action": "forget", "ref": "d1" }),
            CHAT,
            &mut refs,
        )
        .as_deref(),
        Ok("Forgot [d1].")
    );
    let snapshot = service.snapshot(CHAT).expect("a snapshot");
    assert_eq!(snapshot.account[0].content, "Пишет на Rust");
    assert_eq!(snapshot.account[0].source, MemorySource::Model);
    assert!(snapshot.conversation.is_empty());
}

#[test]
fn malformed_calls_and_unknown_refs_go_back_to_the_model() {
    let (_database, service) = service();
    let mut refs = MemoryRefs::default();
    let problem = apply_memory_call(&service, &json!({ "action": "shout" }), CHAT, &mut refs)
        .expect_err("malformed");
    assert!(problem.starts_with("Invalid memory call"));
    let problem = apply_memory_call(
        &service,
        &json!({ "action": "forget", "ref": "a7" }),
        CHAT,
        &mut refs,
    )
    .expect_err("unknown");
    assert!(problem.contains("Unknown memory ref a7"));
    let problem = apply_memory_call(
        &service,
        &json!({ "action": "save", "content": "   " }),
        CHAT,
        &mut refs,
    )
    .expect_err("blank");
    assert!(!problem.is_empty());
}
