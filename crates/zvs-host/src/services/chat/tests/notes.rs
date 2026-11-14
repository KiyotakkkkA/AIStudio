use super::tool_loop::{Scripted, call};
use super::*;
use crate::artifacts::ArtifactStore;
use crate::kernel::{NodeRegistry, register_core_nodes};
use crate::services::chat_notes::{NOTES_CITATIONS, NOTES_EMPTY, NOTES_HEADING};
use crate::tools::{ToolRegistry, ToolRegistryOptions};

const DRAFT: &str = "---\ntitle: Shutdown\ncontext: Cancelling a build left children running.\nread_when: Before changing process shutdown.\nupdate_when: When the launcher changes.\ndelete_when: After the launcher is replaced.\n---\n\nTerminate the whole process tree.\n";

struct Setup {
    harness: Harness,
    driver: Arc<Scripted>,
    _root: crate::testing::TemporaryDirectory,
    _database: crate::testing::TemporaryDatabase,
}

fn setup() -> Setup {
    let root = crate::testing::temporary_directory("chat-notes");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let database = crate::testing::temporary_database();
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    let registry = Arc::new(NodeRegistry::new());
    register_core_nodes(&registry).unwrap();
    let driver = Scripted::new();
    let harness = Harness::with(HarnessOptions {
        database: Some(database.shared()),
        tools: Some(tools.clone()),
        registry: Some(registry.clone()),
        providers: Some(Arc::new(OneDriver(driver.clone()))),
        notes: Some(crate::notes::Notes::new(
            store.folders().clone(),
            Arc::new(SystemClock),
            crate::platform::ids::system_ids(),
        )),
        context_window: 100_000,
        ..HarnessOptions::default()
    });
    let folders = crate::workspace::FolderAccessService::new(
        database.shared(),
        harness.runs.approvals(),
        crate::workspace::ForbiddenFolders::new(&[], &[], Arc::new(Vec::new)),
        Arc::new(crate::testing::FakeClock::default()),
    );
    crate::files::register_file_nodes(
        &registry,
        &folders,
        crate::files::FileToolOptions {
            roots: crate::files::VirtualRoots::new(Some(store.clone()), None),
            ..crate::files::FileToolOptions::new(store.root().to_path_buf())
        },
    )
    .unwrap();
    crate::tools::register_kernel_tools(&registry, &tools).unwrap();
    Setup {
        harness,
        driver,
        _root: root,
        _database: database,
    }
}

fn system(request: &crate::drivers::ai::GenerateRequest) -> String {
    request.system.clone().unwrap_or_default()
}

#[tokio::test]
async fn every_round_sees_the_current_directory_and_a_read_note_becomes_a_source() {
    let test = setup();
    test.driver.round(|request| {
        let prompt = system(request);
        assert!(prompt.contains(NOTES_HEADING));
        assert!(prompt.contains(NOTES_EMPTY));
        assert!(prompt.contains(NOTES_CITATIONS));
        vec![call(
            "fs__apply",
            &json!({ "changes": [{ "op": "write", "path": "chat:notes/shutdown.md", "content": DRAFT }] }),
        )]
    });
    test.driver.round(|request| {
        let prompt = system(request);
        assert!(prompt.contains("chat:notes/shutdown.md"));
        assert!(prompt.contains("Before changing process shutdown."));
        assert!(!prompt.contains(NOTES_EMPTY));
        assert!(!prompt.contains("Terminate the whole process tree."));
        vec![call(
            "fs__read",
            &json!({ "paths": [{ "path": "chat:notes/shutdown.md" }] }),
        )]
    });
    test.driver
        .round(|_| vec![TextDelta::text("Stop the whole process tree.")]);
    let conversation = test.harness.conversation();
    let handle = test
        .harness
        .turn(conversation, "How do I stop the build?")
        .await;
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let answer = test.harness.last(conversation);
    assert!(
        answer
            .tool_calls
            .iter()
            .all(|call| call.status == ChatToolCallStatus::Succeeded)
    );
    let notes: Vec<_> = zvs_contract::note_sources(&answer.sources).collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].path, "chat:notes/shutdown.md");
    assert_eq!(notes[0].title, "Shutdown");
    assert!(!answer.content.contains("chat:"));

    test.driver.round(|_| vec![TextDelta::text("Nothing new.")]);
    let handle = test.harness.turn(conversation, "Anything else?").await;
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let last = test.driver.requests().last().cloned().expect("a request");
    assert!(system(&last).contains("chat:notes/shutdown.md"));
    assert!(test.harness.last(conversation).sources.is_empty());
}

#[tokio::test]
async fn a_listing_or_a_write_alone_produces_no_note_source() {
    let test = setup();
    test.driver.round(|_| {
        vec![call(
            "fs__apply",
            &json!({ "changes": [{ "op": "write", "path": "chat:notes/shutdown.md", "content": DRAFT }] }),
        )]
    });
    test.driver
        .round(|_| vec![call("fs__list", &json!({ "path": "chat:notes" }))]);
    test.driver.round(|_| vec![TextDelta::text("Saved.")]);
    let conversation = test.harness.conversation();
    let handle = test.harness.turn(conversation, "Save it").await;
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert!(test.harness.last(conversation).sources.is_empty());
}
