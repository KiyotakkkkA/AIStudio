use std::sync::Arc;

use serde_json::{Value, json};
use zvs_contract::{ChatContextInput, ProviderCapability};

use super::tool_loop::Scripted;
use super::{Harness, HarnessOptions, OneDriver, within};
use crate::data::Repositories;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::services::profile::{
    ABOUT_HEADING, PROFILE_ABOUT_KEY, PROFILE_STYLE_CUSTOM_KEY, PROFILE_STYLE_KEY, STYLE_FORMAL,
    STYLE_HEADING,
};

struct Profile {
    harness: Harness,
    driver: Arc<Scripted>,
}

impl Profile {
    fn new() -> Self {
        let driver = Scripted::new();
        let harness = Harness::with(HarnessOptions {
            providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
            capabilities: vec![ProviderCapability::Text],
            context_window: 16384,
            max_output: 512,
            ..HarnessOptions::default()
        });
        Self { harness, driver }
    }

    fn set(&self, key: &str, value: &Value) {
        self.harness
            .database
            .with_connection(|connection| {
                Repositories::new(connection)
                    .settings
                    .set(key, &value.to_string(), 2)?;
                Ok(())
            })
            .expect("a setting");
    }

    fn system(&self, round: usize) -> String {
        self.driver.requests()[round]
            .system
            .clone()
            .unwrap_or_default()
    }
}

#[tokio::test]
async fn the_standard_style_and_no_profile_leave_the_prompt_untouched() {
    let test = Profile::new();
    let created = test.harness.conversation();
    within(test.harness.turn(created, "Привет")).await;

    let system = test.system(0);
    assert!(!system.contains(STYLE_HEADING));
    assert!(!system.contains(ABOUT_HEADING));
}

#[tokio::test]
async fn the_reply_style_applies_to_every_dialog_on_the_next_turn() {
    let test = Profile::new();
    let created = test.harness.conversation();
    test.set(PROFILE_STYLE_KEY, &json!("formal"));
    within(test.harness.turn(created, "Привет")).await;
    assert!(test.system(0).contains(STYLE_FORMAL));

    test.set(PROFILE_STYLE_KEY, &json!("custom"));
    test.set(PROFILE_STYLE_CUSTOM_KEY, &json!("Отвечать списками"));
    within(test.harness.turn(created, "Ещё")).await;
    let system = test.system(1);
    assert!(!system.contains(STYLE_FORMAL));
    assert!(system.contains("<reply_style>\nОтвечать списками\n</reply_style>"));
}

#[tokio::test]
async fn profile_information_reaches_only_dialogs_created_after_it_was_saved() {
    let test = Profile::new();
    let before = test.harness.conversation();
    test.set(
        PROFILE_ABOUT_KEY,
        &json!("Backend-разработчик, пишет на Rust"),
    );
    let after = test.harness.conversation();
    test.set(PROFILE_ABOUT_KEY, &json!("Изменено позже"));

    within(test.harness.turn(before, "Привет")).await;
    within(test.harness.turn(after, "Привет")).await;

    assert!(!test.system(0).contains(ABOUT_HEADING));
    let system = test.system(1);
    assert!(system.contains("<user_profile>\nBackend-разработчик, пишет на Rust\n</user_profile>"));
    assert!(!system.contains("Изменено позже"));
}

#[tokio::test]
async fn the_context_breakdown_counts_the_profile_a_new_dialog_would_carry() {
    let test = Profile::new();
    let input = |conversation_id| ChatContextInput {
        conversation_id,
        provider_id: test.harness.provider_id,
        model_id: test.harness.model_id.clone(),
        settings: None,
        system_prompt: None,
        enabled_tools: None,
    };
    let created = test.harness.conversation();
    let empty = test.harness.chat.context(&input(None)).expect("context");
    test.set(
        PROFILE_ABOUT_KEY,
        &json!("Backend-разработчик, пишет на Rust"),
    );

    let fresh = test.harness.chat.context(&input(None)).expect("context");
    let existing = test
        .harness
        .chat
        .context(&input(Some(created)))
        .expect("context");
    assert!(fresh.system > empty.system);
    assert_eq!(existing.system, empty.system);
}
