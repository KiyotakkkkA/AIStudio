use std::path::Path;
use std::time::Duration;

use zvs_contract::{
    AppErrorCode, ConversationId, ExplorerChangeDto, ExplorerChangeKind, ExplorerChangesDto,
    ExplorerEntryKind, FolderGrantScope, HostEventDraft, StreamId,
};

use super::{Host, junction};
use crate::testing::{TemporaryDirectory, temporary_directory};
use crate::workspace::ExplorerService;

fn explorer(host: &Host) -> &ExplorerService {
    &host.services.explorer
}

fn bound(host: &Host, folder: &TemporaryDirectory) -> ConversationId {
    let conversation = host.conversation();
    host.bind(conversation, folder.path(), Some(FolderGrantScope::Always))
        .expect("binds");
    conversation
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("creates folders");
    }
    std::fs::write(path, text).expect("writes");
}

fn batches(host: &Host, stream: StreamId) -> Vec<Vec<ExplorerChangeDto>> {
    host.services
        .run_events
        .events()
        .into_iter()
        .filter(|event| event.stream_id == stream)
        .filter_map(|event| match event.body {
            HostEventDraft::Step { step } => {
                serde_json::from_value::<ExplorerChangesDto>(serde_json::Value::Object(step))
                    .ok()
                    .map(|changes| changes.changes)
            }
            _ => None,
        })
        .collect()
}

fn changes(host: &Host, stream: StreamId) -> Vec<ExplorerChangeDto> {
    batches(host, stream).into_iter().flatten().collect()
}

async fn until(host: &Host, stream: StreamId, path: &str, kind: ExplorerChangeKind) {
    for _ in 0..500 {
        if changes(host, stream)
            .iter()
            .any(|change| change.path == path && change.kind == kind)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "{path} was never reported as {kind:?}: {:?}",
        changes(host, stream)
    );
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

#[tokio::test]
async fn a_listing_is_one_level_with_folders_first_and_ignored_entries_flagged() {
    let host = Host::new();
    let folder = temporary_directory("explorer-list-");
    write(&folder.join(".gitignore"), "dist/\n*.log\n");
    write(&folder.join("b.md"), "# b");
    write(&folder.join("A.txt"), "a");
    write(&folder.join("debug.log"), "x");
    write(&folder.join("src").join("main.rs"), "fn main() {}");
    write(&folder.join("dist").join("out").join("app.js"), "x");
    write(&folder.join(".git").join("HEAD"), "ref");
    let conversation = bound(&host, &folder);

    let root = explorer(&host).list(&conversation, "").expect("lists");
    let names: Vec<(&str, ExplorerEntryKind, bool)> = root
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.kind, entry.ignored))
        .collect();
    assert_eq!(
        names,
        [
            ("dist", ExplorerEntryKind::Folder, true),
            ("src", ExplorerEntryKind::Folder, false),
            (".gitignore", ExplorerEntryKind::File, false),
            ("A.txt", ExplorerEntryKind::File, false),
            ("b.md", ExplorerEntryKind::File, false),
            ("debug.log", ExplorerEntryKind::File, true),
        ]
    );
    assert_eq!(root.path, "");
    assert!(!root.truncated);
    let file = root
        .entries
        .iter()
        .find(|entry| entry.name == "b.md")
        .expect("b.md");
    assert_eq!((file.path.as_str(), file.size), ("b.md", 3));
    assert!(file.modified_at.is_some());

    let nested = explorer(&host).list(&conversation, "dist").expect("lists");
    assert_eq!(nested.path, "dist");
    assert_eq!(nested.entries.len(), 1);
    assert_eq!(nested.entries[0].path, "dist/out");
    assert!(nested.entries[0].ignored);

    let source = explorer(&host).list(&conversation, "src").expect("lists");
    assert_eq!(source.entries[0].path, "src/main.rs");
    assert!(!source.entries[0].ignored);
}

#[tokio::test]
async fn a_read_returns_text_null_for_binary_and_truncates_on_a_line() {
    let host = Host::new();
    let folder = temporary_directory("explorer-read-");
    write(
        &folder.join("docs").join("PLAN.md"),
        "# Plan\r\n\r\n- one\r\n",
    );
    std::fs::write(folder.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).expect("writes");
    let line = format!("{}\n", "x".repeat(1023));
    write(&folder.join("big.txt"), &line.repeat(1100));
    let conversation = bound(&host, &folder);

    let text = explorer(&host)
        .read(&conversation, "docs/PLAN.md")
        .expect("reads");
    assert_eq!(text.path, "docs/PLAN.md");
    assert_eq!(text.text.as_deref(), Some("# Plan\n\n- one\n"));
    assert!(!text.truncated);

    let binary = explorer(&host)
        .read(&conversation, "logo.png")
        .expect("reads");
    assert_eq!(
        (binary.text, binary.truncated, binary.size),
        (None, false, 7)
    );

    let big = explorer(&host)
        .read(&conversation, "big.txt")
        .expect("reads");
    let shown = big.text.expect("text");
    assert!(big.truncated);
    assert_eq!(shown.len(), 1024 * 1024);
    assert!(shown.ends_with('\n'));
    assert_eq!(big.size, 1024 * 1100);

    assert_eq!(
        explorer(&host)
            .read(&conversation, "docs")
            .expect_err("a folder")
            .code,
        AppErrorCode::ValidationFailed
    );
    assert_eq!(
        explorer(&host)
            .read(&conversation, "missing.md")
            .expect_err("missing")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn paths_outside_the_working_folder_are_refused() {
    let host = Host::new();
    let folder = temporary_directory("explorer-bound-");
    let outside = temporary_directory("explorer-outside-");
    write(&outside.join("secret.txt"), "x");
    write(&folder.join("a.txt"), "a");
    junction(&folder.join("link"), outside.path());
    let conversation = bound(&host, &folder);
    let escape = format!(
        "../{}",
        outside
            .path()
            .file_name()
            .expect("a name")
            .to_string_lossy()
    );
    let absolute = outside.join("secret.txt").to_string_lossy().into_owned();

    for path in [
        escape.as_str(),
        absolute.as_str(),
        "link/secret.txt",
        "./a.txt",
    ] {
        assert_eq!(
            explorer(&host)
                .read(&conversation, path)
                .expect_err(path)
                .code,
            AppErrorCode::PermissionDenied,
            "{path}"
        );
    }
    assert_eq!(
        explorer(&host)
            .list(&conversation, "link")
            .expect_err("a junction")
            .code,
        AppErrorCode::PermissionDenied
    );
    let listed = explorer(&host).list(&conversation, "").expect("lists");
    assert!(listed.entries.iter().any(|entry| entry.name == "link"));
}

#[tokio::test]
async fn an_uncovered_working_folder_is_refused_without_a_request() {
    let host = Host::new();
    let folder = temporary_directory("explorer-revoked-");
    write(&folder.join("a.txt"), "a");
    let conversation = bound(&host, &folder);
    let grant = host.folders().list().expect("lists").remove(0);
    host.folders().revoke(&grant.id).expect("revokes");

    assert_eq!(
        explorer(&host)
            .list(&conversation, "")
            .expect_err("revoked")
            .code,
        AppErrorCode::PermissionDenied
    );
    assert_eq!(
        explorer(&host)
            .watch(&conversation)
            .expect_err("revoked")
            .code,
        AppErrorCode::PermissionDenied
    );
    let unbound = host.conversation();
    assert_eq!(
        explorer(&host)
            .list(&unbound, "")
            .expect_err("no folder")
            .code,
        AppErrorCode::Conflict
    );
    assert!(
        host.services
            .run_events
            .events()
            .iter()
            .all(|event| !matches!(event.body, HostEventDraft::Approval { .. }))
    );
}

#[tokio::test]
async fn the_watcher_reports_created_modified_and_removed_files() {
    let host = Host::new();
    let folder = temporary_directory("explorer-watch-");
    let conversation = bound(&host, &folder);
    let stream = explorer(&host)
        .watch(&conversation)
        .expect("watches")
        .stream_id;
    settle().await;

    write(&folder.join("docs").join("PLAN.md"), "# Plan");
    until(&host, stream, "docs/PLAN.md", ExplorerChangeKind::Created).await;
    settle().await;
    write(&folder.join("docs").join("PLAN.md"), "# Plan\n\nmore");
    until(&host, stream, "docs/PLAN.md", ExplorerChangeKind::Modified).await;
    settle().await;
    std::fs::remove_file(folder.join("docs").join("PLAN.md")).expect("removes");
    until(&host, stream, "docs/PLAN.md", ExplorerChangeKind::Removed).await;

    write(&folder.join(".git").join("index"), "x");
    write(&folder.join("after.txt"), "x");
    until(&host, stream, "after.txt", ExplorerChangeKind::Created).await;
    assert!(
        changes(&host, stream)
            .iter()
            .all(|change| !change.path.to_lowercase().contains(".git"))
    );
    explorer(&host).unwatch(&conversation);
}

#[tokio::test]
async fn a_burst_of_writes_arrives_as_one_change() {
    let host = Host::new();
    let folder = temporary_directory("explorer-burst-");
    write(&folder.join("burst.txt"), "0");
    let conversation = bound(&host, &folder);
    let stream = explorer(&host)
        .watch(&conversation)
        .expect("watches")
        .stream_id;
    settle().await;

    for round in 1..=20 {
        write(&folder.join("burst.txt"), &round.to_string());
    }
    until(&host, stream, "burst.txt", ExplorerChangeKind::Modified).await;
    settle().await;
    let reported: Vec<ExplorerChangeDto> = changes(&host, stream)
        .into_iter()
        .filter(|change| change.path == "burst.txt")
        .collect();
    assert_eq!(reported.len(), 1, "{reported:?}");
    explorer(&host).unwatch(&conversation);
}

#[tokio::test]
async fn subscribers_share_one_watcher_and_the_last_one_ends_it() {
    let host = Host::new();
    let folder = temporary_directory("explorer-share-");
    let conversation = bound(&host, &folder);
    let first = explorer(&host).watch(&conversation).expect("watches");
    let second = explorer(&host).watch(&conversation).expect("watches");
    assert_eq!(first.stream_id, second.stream_id);
    assert_eq!(explorer(&host).watching(&conversation), 2);

    explorer(&host).unwatch(&conversation);
    assert_eq!(explorer(&host).watching(&conversation), 1);
    let ended = |host: &Host| {
        host.services.run_events.events().iter().any(|event| {
            event.stream_id == first.stream_id && matches!(event.body, HostEventDraft::End { .. })
        })
    };
    assert!(!ended(&host));
    explorer(&host).unwatch(&conversation);
    assert_eq!(explorer(&host).watching(&conversation), 0);
    assert!(ended(&host));
}

#[tokio::test]
async fn rebinding_moves_the_watcher_and_unbinding_reports_the_root_gone() {
    let host = Host::new();
    let first = temporary_directory("explorer-first-");
    let second = temporary_directory("explorer-second-");
    let conversation = bound(&host, &first);
    let stream = explorer(&host)
        .watch(&conversation)
        .expect("watches")
        .stream_id;

    host.bind(conversation, second.path(), Some(FolderGrantScope::Always))
        .expect("binds");
    explorer(&host).rebind(&conversation);
    until(&host, stream, "", ExplorerChangeKind::Modified).await;
    settle().await;
    write(&first.join("old.txt"), "x");
    write(&second.join("new.txt"), "x");
    until(&host, stream, "new.txt", ExplorerChangeKind::Created).await;
    assert!(
        changes(&host, stream)
            .iter()
            .all(|change| change.path != "old.txt")
    );

    host.services
        .chat
        .unbind_folder(&conversation)
        .expect("unbinds");
    explorer(&host).rebind(&conversation);
    until(&host, stream, "", ExplorerChangeKind::Removed).await;
    explorer(&host).unwatch(&conversation);
}
