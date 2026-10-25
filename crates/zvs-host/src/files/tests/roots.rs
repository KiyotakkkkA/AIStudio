use super::*;
use crate::files::nodes::FS_LIST;
use crate::files::roots::{BAD_ROOT_PATH, ROOTS_ONLY, SKILL_READ_ONLY};

const SKILL: &str = "extract";

fn chat_folder(host: &Host, conversation: ConversationId) -> PathBuf {
    host.services
        .workspace
        .join("artifacts")
        .join(crate::chat_files::CHAT_FOLDERS)
        .join(conversation.to_string())
}

fn with_skill(host: &Host) {
    host.services
        .skills
        .create(&zvs_contract::CreateSkillInput {
            key: SKILL.to_owned(),
            name: "Извлечение".to_owned(),
            description: "Извлекает таблицы из документов".to_owned(),
            instructions: "Прочитайте references/guide.md.".to_owned(),
            enabled: None,
            files: Some(vec![zvs_contract::SkillFileInput {
                path: "references/guide.md".to_owned(),
                content: "Руководство".to_owned(),
                encoding: None,
            }]),
        })
        .expect("creates the skill");
}

fn read_paths(conversation: ConversationId, after: &[&str], paths: &[&str]) -> Value {
    json!({
        "id": "read",
        "type": FS_READ,
        "dependencies": after,
        "input": {
            "conversationId": conversation.to_string(),
            "paths": paths.iter().map(|path| json!({ "path": path })).collect::<Vec<_>>()
        }
    })
}

#[tokio::test]
async fn chat_paths_are_written_read_and_listed_without_asking() {
    let host = Host::new();
    let conversation = host.conversation();
    let handle = host.start(json!([
        apply_node(
            conversation,
            &[],
            json!([
                { "op": "write", "path": "chat:_spec.json", "content": "{\"title\": \"Реестр\"}" },
                { "op": "write", "path": "chat:notes/summary.md", "content": "# Итоги" }
            ])
        ),
        read_paths(conversation, &["apply"], &["chat:_spec.json"]),
        {
            "id": "list",
            "type": FS_LIST,
            "dependencies": ["read"],
            "input": { "conversationId": conversation.to_string(), "path": "chat:", "recursive": true }
        }
    ]));
    assert_eq!(host.finish(&handle.id).await, RunStatus::Succeeded);
    assert!(host.approvals(&handle.id).is_empty());
    let folder = chat_folder(&host, conversation);
    assert_eq!(text(&folder.join("_spec.json")), "{\"title\": \"Реестр\"}");
    assert_eq!(text(&folder.join("notes").join("summary.md")), "# Итоги");
    let read = host.output(&handle.id, "read");
    assert!(
        read["text"]
            .as_str()
            .expect("a text")
            .contains("{\"title\": \"Реестр\"}")
    );
    let listing = host.output(&handle.id, "list");
    let listing = listing["text"].as_str().expect("a listing");
    assert!(listing.contains("_spec.json"));
    assert!(listing.contains("notes/summary.md"));

    let rewritten = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": "chat:_spec.json", "content": "{}" }])
    )]));
    assert_eq!(host.finish(&rewritten.id).await, RunStatus::Succeeded);
    assert!(host.approvals(&rewritten.id).is_empty());
    assert_eq!(text(&folder.join("_spec.json")), "{}");
}

#[tokio::test]
async fn a_real_path_still_asks_and_a_skill_path_is_read_only() {
    let host = Host::new();
    with_skill(&host);
    let folder = temporary_directory("roots-real-");
    let root = canonical(folder.path()).expect("a canonical path");
    let conversation = host.bound(&root);

    let real = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": "kept.txt", "content": "x" }])
    )]));
    let request = host.approval(&real.id).await;
    assert_eq!(request["kind"], json!("files"));
    assert!(!root.join("kept.txt").exists());
    host.services
        .runs
        .approve(&real.id, request["id"].as_str().expect("an id"), false)
        .expect("approves");
    assert_eq!(host.finish(&real.id).await, RunStatus::Succeeded);
    assert_eq!(text(&root.join("kept.txt")), "x");

    let skill = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": "skill:extract/references/guide.md", "content": "x" }])
    )]));
    assert_eq!(host.finish(&skill.id).await, RunStatus::Failed);
    assert_eq!(host.failure(&skill.id).as_deref(), Some(SKILL_READ_ONLY));
    assert!(host.approvals(&skill.id).is_empty());

    let read = host.start(json!([read_paths(
        conversation,
        &[],
        &["skill:extract/references/guide.md"]
    )]));
    assert_eq!(host.finish(&read.id).await, RunStatus::Succeeded);
    assert!(host.approvals(&read.id).is_empty());
    assert!(
        host.output(&read.id, "read")["text"]
            .as_str()
            .expect("a text")
            .contains("Руководство")
    );
}

#[tokio::test]
async fn an_escape_is_refused_on_every_root() {
    let host = Host::new();
    with_skill(&host);
    let conversation = host.conversation();
    for path in [
        "chat:../escape.txt",
        "chat:a/../../escape.txt",
        "chat:.hidden",
        "skill:extract/../escape.txt",
        "skill:extract/references/../../escape.txt",
        "skill:extract/.git/config",
    ] {
        let read = host.start(json!([read_paths(conversation, &[], &[path])]));
        assert_eq!(host.finish(&read.id).await, RunStatus::Failed, "{path}");
        assert_eq!(
            host.failure(&read.id).as_deref(),
            Some(BAD_ROOT_PATH),
            "{path}"
        );
        let write = host.start(json!([apply_node(
            conversation,
            &[],
            json!([{ "op": "write", "path": path, "content": "x" }])
        )]));
        assert_eq!(host.finish(&write.id).await, RunStatus::Failed, "{path}");
        assert!(host.approvals(&write.id).is_empty(), "{path}");
    }
}

#[tokio::test]
async fn roots_only_calls_reach_no_real_path() {
    let host = Host::new();
    let folder = temporary_directory("roots-only-");
    let root = canonical(folder.path()).expect("a canonical path");
    let file = root.join("data.csv");
    std::fs::write(&file, "a,b\n").expect("writes");
    let conversation = host.bound(&root);
    let refused = host.start(json!([{
        "id": "read",
        "type": FS_READ,
        "input": {
            "conversationId": conversation.to_string(),
            "rootsOnly": true,
            "paths": [{ "path": file.to_string_lossy() }]
        }
    }]));
    assert_eq!(host.finish(&refused.id).await, RunStatus::Failed);
    assert_eq!(host.failure(&refused.id).as_deref(), Some(ROOTS_ONLY));
    let listed = host.start(json!([{
        "id": "list",
        "type": FS_LIST,
        "input": { "conversationId": conversation.to_string(), "rootsOnly": true }
    }]));
    assert_eq!(host.finish(&listed.id).await, RunStatus::Succeeded);
    assert!(host.approvals(&listed.id).is_empty());
    let listing = host.output(&listed.id, "list");
    assert!(
        listing["text"]
            .as_str()
            .expect("a listing")
            .contains(&chat_folder(&host, conversation).display().to_string())
    );
}

#[tokio::test]
async fn a_mixed_batch_asks_once_for_every_change_and_off_closes_the_chat_folder() {
    let host = Host::new();
    let folder = temporary_directory("roots-mixed-");
    let root = canonical(folder.path()).expect("a canonical path");
    let conversation = host.bound(&root);
    let mixed = host.start(json!([apply_node(
        conversation,
        &[],
        json!([
            { "op": "write", "path": "chat:_draft.md", "content": "x" },
            { "op": "write", "path": "report.md", "content": "y" }
        ])
    )]));
    let request = host.approval(&mixed.id).await;
    assert_eq!(request["kind"], json!("files"));
    assert_eq!(request["changes"].as_array().map(Vec::len), Some(2));
    host.services
        .runs
        .approve(&mixed.id, request["id"].as_str().expect("an id"), false)
        .expect("approves");
    assert_eq!(host.finish(&mixed.id).await, RunStatus::Succeeded);
    assert_eq!(host.approvals(&mixed.id).len(), 1);
    assert_eq!(
        text(&chat_folder(&host, conversation).join("_draft.md")),
        "x"
    );
    assert_eq!(text(&root.join("report.md")), "y");

    host.set_tier(ToolTier::Off);
    let off = host.start(json!([apply_node(
        conversation,
        &[],
        json!([{ "op": "write", "path": "chat:_other.md", "content": "x" }])
    )]));
    assert_eq!(host.finish(&off.id).await, RunStatus::Failed);
    assert!(host.approvals(&off.id).is_empty());
    assert!(!chat_folder(&host, conversation).join("_other.md").exists());
}
