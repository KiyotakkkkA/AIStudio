use super::*;
use crate::notes::{NOTE_BAD_PATH, NOTE_FIELD_MISSING, parse as parse_note};

const DRAFT: &str = "---\ntitle: Shutdown\ncontext: Cancelling a build left children running.\nread_when: Before changing process shutdown.\nupdate_when: When the launcher changes.\ndelete_when: After the launcher is replaced.\n---\n\n# Shutdown\n\nTerminate the whole tree.\n";

fn notes_folder(host: &Host, conversation: ConversationId) -> PathBuf {
    host.services
        .workspace
        .join(crate::conversations::CONVERSATIONS_DIR)
        .join(conversation.to_string())
        .join(crate::conversations::WORK_DIR)
        .join("notes")
}

fn ask_everything(host: &Host) {
    for key in [FS_READ, FS_APPLY] {
        let tool = host.services.tools.find_by_key(key).unwrap().unwrap();
        host.services
            .tools
            .set_tier(&tool.id.to_string(), ToolTier::Ask, "user")
            .unwrap();
    }
}

fn read_note(conversation: ConversationId, after: &[&str], path: &str) -> Value {
    json!({
        "id": "read",
        "type": FS_READ,
        "dependencies": after,
        "input": { "conversationId": conversation.to_string(), "paths": [{ "path": path }] }
    })
}

#[tokio::test]
async fn a_model_note_is_written_without_asking_and_gets_managed_fields() {
    let host = Host::new();
    ask_everything(&host);
    let conversation = host.conversation();
    let handle = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": "chat:notes/shutdown.md", "content": DRAFT }])
    )]));
    assert_eq!(host.finish(&handle.id).await, RunStatus::Succeeded);
    assert!(host.approvals(&handle.id).is_empty());
    let stored = std::fs::read(notes_folder(&host, conversation).join("shutdown.md")).unwrap();
    let note = parse_note(&stored).expect("a complete note");
    assert_eq!(note.fields.title, "Shutdown");
    let output = host.output(&handle.id, "apply");
    assert_eq!(output["notes"][0]["path"], "chat:notes/shutdown.md");
    assert_eq!(output["notes"][0]["id"], note.id.to_string());
    assert!(
        output["text"]
            .as_str()
            .unwrap()
            .contains(&note.id.to_string())
    );
    assert!(
        output["changes"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("schema_version")
    );
}

#[tokio::test]
async fn an_invalid_note_fails_the_whole_batch() {
    let host = Host::new();
    let conversation = host.conversation();
    let broken = DRAFT.replace("read_when: Before changing process shutdown.\n", "");
    let handle = host.start(json!([apply_node(
        conversation,
        &[],
        json!([
            { "op": "write", "path": "chat:plain.txt", "content": "kept out" },
            { "op": "write", "path": "chat:notes/broken.md", "content": broken }
        ])
    )]));
    assert_eq!(host.finish(&handle.id).await, RunStatus::Failed);
    assert_eq!(
        host.failure(&handle.id).as_deref(),
        Some(NOTE_FIELD_MISSING)
    );
    let folder = notes_folder(&host, conversation);
    assert!(!folder.join("broken.md").exists());
    assert!(!folder.parent().unwrap().join("plain.txt").exists());
}

#[tokio::test]
async fn nested_and_non_markdown_note_paths_are_refused() {
    let host = Host::new();
    let conversation = host.conversation();
    for path in ["chat:notes/deep/topic.md", "chat:notes/topic.txt"] {
        let handle = host.start(json!([apply_node(
            conversation,
            &[],
            json!([{ "op": "write", "path": path, "content": DRAFT }])
        )]));
        assert_eq!(host.finish(&handle.id).await, RunStatus::Failed);
        assert_eq!(host.failure(&handle.id).as_deref(), Some(NOTE_BAD_PATH));
    }
    assert!(!notes_folder(&host, conversation).exists());
}

#[tokio::test]
async fn a_rename_keeps_the_identity_and_never_overwrites_another_note() {
    let host = Host::new();
    let conversation = host.conversation();
    let created = host.start(json!([apply_node(
        conversation,
        &[],
        json!([
            { "op": "write", "path": "chat:notes/first.md", "content": DRAFT },
            { "op": "write", "path": "chat:notes/second.md", "content": DRAFT.replace("Shutdown", "Second") }
        ])
    )]));
    assert_eq!(host.finish(&created.id).await, RunStatus::Succeeded);
    let folder = notes_folder(&host, conversation);
    let before = parse_note(&std::fs::read(folder.join("first.md")).unwrap()).unwrap();
    let collision = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "move", "from": "chat:notes/first.md", "to": "chat:notes/second.md" }])
    )]));
    assert_eq!(host.finish(&collision.id).await, RunStatus::Failed);
    assert!(folder.join("first.md").exists());
    let renamed = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "move", "from": "chat:notes/first.md", "to": "chat:notes/renamed.md" }])
    )]));
    assert_eq!(host.finish(&renamed.id).await, RunStatus::Succeeded);
    let after = parse_note(&std::fs::read(folder.join("renamed.md")).unwrap()).unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);
    assert!(!folder.join("first.md").exists());
}

#[tokio::test]
async fn a_note_read_reports_the_source_and_other_reads_do_not() {
    let host = Host::new();
    let conversation = host.conversation();
    let created = host.start(json!([apply_node(
        conversation,
        &[],
        json!([
            { "op": "write", "path": "chat:notes/shutdown.md", "content": DRAFT },
            { "op": "write", "path": "chat:plain.md", "content": "plain" }
        ])
    )]));
    assert_eq!(host.finish(&created.id).await, RunStatus::Succeeded);
    let stored = std::fs::read(notes_folder(&host, conversation).join("shutdown.md")).unwrap();
    let note = parse_note(&stored).unwrap();
    let read = host.start(json!([read_note(
        conversation,
        &[],
        "chat:notes/shutdown.md"
    )]));
    assert_eq!(host.finish(&read.id).await, RunStatus::Succeeded);
    let output = host.output(&read.id, "read");
    assert_eq!(
        output["notes"],
        json!([{
            "id": note.id.to_string(),
            "title": "Shutdown",
            "path": "chat:notes/shutdown.md",
            "hash": crate::files::text::content_hash(&stored)
        }])
    );
    for path in ["chat:plain.md", "chat:notes/missing.md"] {
        let other = host.start(json!([read_note(conversation, &[], path)]));
        assert_eq!(host.finish(&other.id).await, RunStatus::Succeeded);
        assert!(host.output(&other.id, "read").get("notes").is_none());
    }
}

#[tokio::test]
async fn notes_of_another_dialog_stay_out_of_reach() {
    let host = Host::new();
    let conversation = host.conversation();
    let other = host.conversation();
    let target = notes_folder(&host, other).join("private.md");
    let handle = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": target, "content": DRAFT }])
    )]));
    assert_eq!(host.finish(&handle.id).await, RunStatus::Failed);
    assert!(!target.exists());
    host.set_tier(ToolTier::Off);
    let blocked = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": "chat:notes/shutdown.md", "content": DRAFT }])
    )]));
    assert_eq!(host.finish(&blocked.id).await, RunStatus::Failed);
    assert!(
        !notes_folder(&host, conversation)
            .join("shutdown.md")
            .exists()
    );
}

#[tokio::test]
async fn an_absolute_path_into_the_own_notes_folder_is_validated_and_read_as_a_note() {
    let host = Host::new();
    let conversation = host.conversation();
    let target = notes_folder(&host, conversation).join("absolute.md");
    let broken = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": target, "content": "no frontmatter" }])
    )]));
    assert_eq!(host.finish(&broken.id).await, RunStatus::Failed);
    assert!(!target.exists());
    let written = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": target, "content": DRAFT }])
    )]));
    assert_eq!(host.finish(&written.id).await, RunStatus::Succeeded);
    assert!(parse_note(&std::fs::read(&target).unwrap()).is_ok());
    let read = host.start(json!([read_note(
        conversation,
        &[],
        target.to_str().unwrap()
    )]));
    assert_eq!(host.finish(&read.id).await, RunStatus::Succeeded);
    assert_eq!(
        host.output(&read.id, "read")["notes"][0]["path"],
        "chat:notes/absolute.md"
    );
}
