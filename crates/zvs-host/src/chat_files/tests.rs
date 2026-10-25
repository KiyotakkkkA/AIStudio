use std::sync::Arc;

use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;
use zvs_contract::{ApprovalRequestDto, HostEventDraft, RunId};

use super::*;
use crate::kernel::types::ApprovalDecision;
use crate::kernel::{KernelServices, StepLink};
use crate::platform::ids::new_id;

const CHAT: &str = "01a17431-1de8-713c-9981-dbd5ddd5b80b";

struct NoLink;

impl StepLink for NoLink {
    fn emit(&self, _event: HostEventDraft) -> Result<(), AppError> {
        Ok(())
    }

    fn request_approval(
        &self,
        _request: ApprovalRequestDto,
    ) -> BoxFuture<'static, Result<ApprovalDecision, AppError>> {
        Box::pin(async { Ok(ApprovalDecision::Denied) })
    }
}

fn context() -> StepContext {
    StepContext::new(
        RunId::new(new_id()),
        CancellationToken::new(),
        KernelServices::default(),
        Arc::new(NoLink),
    )
}

fn input(action: ChatFileAction, path: Option<&str>, content: Option<&str>) -> ChatFileInput {
    ChatFileInput {
        action,
        path: path.map(str::to_owned),
        content: content.map(str::to_owned),
        conversation_id: CHAT.to_owned(),
    }
}

#[test]
fn paths_stay_inside_the_folder_and_underscores_mark_working_files() {
    let folder = Path::new("/data/artifacts/chats/c1");
    assert_eq!(
        chat_path(folder, "_spec.json").expect("a path"),
        folder.join("_spec.json")
    );
    assert_eq!(
        chat_path(folder, "data\\_kpi.csv").expect("a path"),
        folder.join("data").join("_kpi.csv")
    );
    for bad in [
        "",
        "..",
        "../x.json",
        "a/../../x",
        "C:/Windows/x",
        ".hidden",
        "a/.git/config",
        "name.",
        "a|b",
    ] {
        assert!(chat_path(folder, bad).is_err(), "{bad}");
    }
    assert!(is_working(Path::new("_spec.json")));
    assert!(is_working(Path::new("_work/table.csv")));
    assert!(!is_working(Path::new("report.html")));
    assert!(!is_working(Path::new("data/report_v2.csv")));
}

#[tokio::test]
async fn a_chat_writes_reads_and_lists_its_own_folder_and_loses_it_with_the_chat() {
    let root = tempfile::tempdir().expect("the data folder");
    let store = ArtifactStore::new(root.path().join("artifacts"));
    let node = ChatFileNode(store.clone());
    let written = node
        .run(
            &context(),
            input(
                ChatFileAction::Write,
                Some("_spec.json"),
                Some("{\"title\": \"Реестр\"}"),
            ),
        )
        .await
        .expect("writes");
    let folder = root.path().join("artifacts").join(CHAT_FOLDERS).join(CHAT);
    assert_eq!(written.folder, folder.to_string_lossy());
    assert_eq!(
        written.path.as_deref(),
        Some(folder.join("_spec.json").to_string_lossy().as_ref())
    );
    node.run(
        &context(),
        input(
            ChatFileAction::Write,
            Some("notes/summary.md"),
            Some("# Итоги"),
        ),
    )
    .await
    .expect("writes a nested file");
    let read = node
        .run(
            &context(),
            input(ChatFileAction::Read, Some("_spec.json"), None),
        )
        .await
        .expect("reads");
    assert_eq!(read.content.as_deref(), Some("{\"title\": \"Реестр\"}"));
    let listed = node
        .run(&context(), input(ChatFileAction::List, None, None))
        .await
        .expect("lists");
    assert_eq!(
        listed
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.working))
            .collect::<Vec<_>>(),
        [("_spec.json", true), ("notes/summary.md", false)]
    );
    let missing = node
        .run(
            &context(),
            input(ChatFileAction::Read, Some("absent.txt"), None),
        )
        .await
        .expect_err("missing");
    assert_eq!(missing.message, CHAT_FILE_MISSING);
    let escape = node
        .run(
            &context(),
            input(ChatFileAction::Write, Some("../other/x.txt"), Some("x")),
        )
        .await
        .expect_err("escaping");
    assert_eq!(escape.message, BAD_CHAT_PATH);
    let large = "x".repeat(CHAT_FILE_MAX_BYTES + 1);
    let error = node
        .run(
            &context(),
            input(ChatFileAction::Write, Some("_big.txt"), Some(&large)),
        )
        .await
        .expect_err("too large");
    assert_eq!(error.message, CHAT_FILE_TOO_LARGE);
    let orphan = node
        .run(
            &context(),
            ChatFileInput {
                conversation_id: String::new(),
                ..input(ChatFileAction::List, None, None)
            },
        )
        .await
        .expect_err("no chat");
    assert_eq!(orphan.message, NO_CHAT);
    remove_chat_dir(&store, CHAT);
    assert!(!folder.exists());
    remove_chat_dir(&store, "../..");
    assert!(root.path().join("artifacts").exists());
}

#[test]
fn chat_files_are_offered_wherever_skill_scripts_are() {
    let mut keys = vec!["fs.read".to_owned(), "skill.script".to_owned()];
    with_chat_files(&mut keys);
    with_chat_files(&mut keys);
    assert_eq!(keys, ["fs.read", "skill.script", CHAT_FILE]);
    let mut without = vec!["fs.read".to_owned()];
    with_chat_files(&mut without);
    assert_eq!(without, ["fs.read"]);
}
