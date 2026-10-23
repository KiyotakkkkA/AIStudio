use std::sync::Arc;

use serde_json::json;
use zvs_contract::{
    ChatToolCallStatus, ConversationId, MemoryScope, MemorySource, ProviderCapability, RunStatus,
};

use super::tool_loop::{Scripted, call, text};
use super::{Harness, HarnessOptions, OneDriver, within};
use crate::data::Repositories;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::services::chat_memory::{MEMORY_HEADING, MEMORY_RULES, MEMORY_TOOL};
use crate::services::memory::{MEMORY_ENABLED_KEY, MEMORY_MODEL_KEY};

struct Memory {
    harness: Harness,
    driver: Arc<Scripted>,
}

impl Memory {
    fn new() -> Self {
        let driver = Scripted::new();
        let harness = Harness::with(HarnessOptions {
            providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
            capabilities: vec![ProviderCapability::Text],
            context_window: 16384,
            max_output: 512,
            memory: true,
            ..HarnessOptions::default()
        });
        Self { harness, driver }
    }

    fn switch(&self, key: &str, on: bool) {
        self.harness
            .database
            .with_connection(|connection| {
                Repositories::new(connection)
                    .settings
                    .set(key, &on.to_string(), 2)?;
                Ok(())
            })
            .expect("a setting");
    }

    fn remember(&self, scope: MemoryScope, conversation: Option<ConversationId>, content: &str) {
        let conversation = conversation.map(|id| id.to_string());
        self.harness
            .chat
            .memory()
            .save(scope, conversation.as_deref(), content, MemorySource::User)
            .expect("saved");
    }

    fn system(&self, round: usize) -> String {
        self.driver.requests()[round]
            .system
            .clone()
            .unwrap_or_default()
    }

    fn offers_memory(&self, round: usize) -> bool {
        self.driver.requests()[round]
            .tools
            .iter()
            .any(|tool| tool.name == "memory")
    }
}

#[tokio::test]
async fn a_turn_carries_account_and_dialog_memory_in_the_system_prompt() {
    let test = Memory::new();
    let created = test.harness.conversation();
    let other = test.harness.conversation();
    test.remember(MemoryScope::Account, None, "Пишет на Rust");
    test.remember(MemoryScope::Conversation, Some(created), "Ветка main");
    test.remember(MemoryScope::Conversation, Some(other), "Ветка develop");
    within(test.harness.turn(created, "Привет")).await;

    let system = test.system(0);
    assert!(system.contains(MEMORY_HEADING));
    assert!(system.contains(MEMORY_RULES));
    assert!(system.contains("[a1] Пишет на Rust"));
    assert!(system.contains("[d1] Ветка main"));
    assert!(!system.contains("Ветка develop"));
    assert!(test.offers_memory(0));
}

#[tokio::test]
async fn a_model_memory_call_saves_the_fact_and_returns_its_ref() {
    let test = Memory::new();
    test.driver.round(|_| {
        vec![call(
            "memory",
            &json!({ "action": "save", "scope": "conversation", "content": "Проект называется ZVS" }),
        )]
    });
    test.driver.fallback(|_| vec![text("Запомнил.")]);
    let created = test.harness.conversation();
    let handle = within(test.harness.turn(created, "Запомни: проект называется ZVS")).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let assistant = test.harness.last(created);
    let record = &assistant.tool_calls[0];
    assert_eq!(record.key, MEMORY_TOOL);
    assert_eq!(record.status, ChatToolCallStatus::Succeeded);
    assert_eq!(record.result.as_deref(), Some("Saved as [d1]."));
    let snapshot = test
        .harness
        .chat
        .memory()
        .snapshot(&created.to_string())
        .expect("a snapshot");
    assert_eq!(snapshot.conversation[0].content, "Проект называется ZVS");
    assert_eq!(snapshot.conversation[0].source, MemorySource::Model);
    assert!(
        test.driver.requests()[1]
            .messages
            .last()
            .is_some_and(|message| message.content.contains("Saved as [d1]."))
    );

    within(test.harness.turn(created, "Как называется проект?")).await;
    assert!(test.system(2).contains("[d1] Проект называется ZVS"));
}

#[tokio::test]
async fn switches_turn_off_the_bridge_or_the_whole_memory() {
    let test = Memory::new();
    test.remember(MemoryScope::Account, None, "Пишет на Rust");
    let created = test.harness.conversation();

    test.switch(MEMORY_MODEL_KEY, false);
    within(test.harness.turn(created, "Первый")).await;
    assert!(test.system(0).contains("[a1] Пишет на Rust"));
    assert!(!test.system(0).contains(MEMORY_RULES));
    assert!(!test.offers_memory(0));

    test.switch(MEMORY_ENABLED_KEY, false);
    within(test.harness.turn(created, "Второй")).await;
    assert!(!test.system(1).contains(MEMORY_HEADING));
    assert!(!test.offers_memory(1));
}
