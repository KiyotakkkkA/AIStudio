use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{
    AppErrorCode, BindFolderInput, BindFolderResultDto, ConversationId, CreateConversationInput,
    FolderAccessAnswer, FolderGrantScope, HostEventDraft, ProviderCapability, ProviderId,
    ProviderKind, RunGraph, RunHandleDto, RunId, RunKind, RunStatus, WorkingFolderAccess,
};

use super::paths::{ForbiddenFolders, TargetKind, canonical, covers, grant_target, is_drive_root};
use super::service::{FOLDER_SUBJECT, FolderAccessService, FolderTarget, NO_WORKING_FOLDER};
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft, RunDraft};
use crate::kernel::{FnNode, PermissionRequirement, StepContext};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::ids::new_id;
use crate::services::SystemService;
use crate::testing::{TemporaryDirectory, TestServices, temporary_directory, test_services};

const NODE: &str = "test.folders";

fn junction(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("runs mklink");
        assert!(output.status.success(), "{output:?}");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("links");
}

fn settled(directory: &TemporaryDirectory) -> PathBuf {
    canonical(directory.path()).expect("a canonical path")
}

#[test]
fn a_grant_covers_its_subfolders_on_component_boundaries_and_ignores_case() {
    let root = settled(&temporary_directory("cover-"));
    let work = root.join("work");
    assert!(covers(&work, &work));
    assert!(covers(&work, &work.join("site").join("assets")));
    assert!(!covers(&work, &root.join("workshop")));
    assert!(!covers(&work, &root));
    assert!(covers(&root.join("Work"), &root.join("WORK").join("x")));
    assert!(!covers(Path::new(""), &work));
}

#[test]
fn a_junction_that_points_outside_the_grant_is_not_covered() {
    let granted = temporary_directory("granted-");
    let outside = temporary_directory("outside-");
    std::fs::write(outside.join("secret.txt"), "x").expect("writes");
    let link = granted.join("link");
    junction(&link, outside.path());
    let target = grant_target(&link.join("secret.txt"), TargetKind::File).expect("resolves");
    assert_eq!(target.folder, settled(&outside));
    assert!(!covers(&settled(&granted), &target.folder));
    let listing = grant_target(&link, TargetKind::Folder).expect("resolves");
    assert!(!covers(&settled(&granted), &listing.folder));
}

#[test]
fn the_grant_unit_is_the_nearest_existing_parent() {
    let directory = temporary_directory("nearest-");
    let root = settled(&directory);
    std::fs::create_dir(root.join("docs")).expect("creates");
    std::fs::write(root.join("docs").join("a.txt"), "x").expect("writes");

    let file = grant_target(&root.join("docs").join("a.txt"), TargetKind::File).expect("a file");
    assert_eq!(file.folder, root.join("docs"));
    assert!(file.creates.is_empty());

    let listing = grant_target(&root.join("docs"), TargetKind::Folder).expect("a listing");
    assert_eq!(listing.folder, root.join("docs"));

    let missing = grant_target(
        &root.join("out").join("logs").join("run.txt"),
        TargetKind::File,
    )
    .expect("a missing file");
    assert_eq!(missing.folder, root);
    assert_eq!(
        missing.creates,
        [PathBuf::from("out"), Path::new("out").join("logs")]
    );
    assert_eq!(missing.path, root.join("out").join("logs").join("run.txt"));

    let folder = grant_target(&root.join("fresh"), TargetKind::Folder).expect("a missing folder");
    assert_eq!(folder.folder, root);
    assert_eq!(folder.creates, [PathBuf::from("fresh")]);

    assert!(grant_target(&root.join("docs").join("a.txt").join("b"), TargetKind::File).is_err());
    assert!(grant_target(Path::new("relative"), TargetKind::File).is_err());
}

#[test]
fn forbidden_folders_and_grants_that_would_contain_them_are_refused() {
    let base = temporary_directory("forbidden-");
    let root = settled(&base);
    let make = |name: &str| {
        let path = root.join(name);
        std::fs::create_dir_all(&path).expect("creates");
        path
    };
    let windows = make("Windows");
    let programs = make("Program Files");
    let programs_x86 = make("Program Files (x86)");
    let program_data = make("ProgramData");
    let profile = make("Users").join("me");
    std::fs::create_dir_all(&profile).expect("creates");
    let data = profile.join("AppData").join("Roaming").join("studio");
    std::fs::create_dir_all(&data).expect("creates");
    let profile_child = make("Users/me/projects");
    let usb = make("usb");
    let mounts = usb.clone();
    let forbidden = ForbiddenFolders::new(
        &[
            windows.clone(),
            programs.clone(),
            programs_x86.clone(),
            program_data.clone(),
            data.clone(),
        ],
        std::slice::from_ref(&profile),
        Arc::new(move || vec![mounts.clone()]),
    );
    for refused in [
        &windows,
        &windows.join("System32"),
        &programs,
        &programs_x86,
        &program_data.join("vendor"),
        &data,
        &data.join("logs"),
        &profile,
        &profile.join("AppData"),
        &root.join("Users"),
        &usb.join("photos"),
    ] {
        assert!(forbidden.refuses(refused), "{}", refused.display());
    }
    let drive = root.ancestors().last().expect("a root").to_path_buf();
    assert!(is_drive_root(&drive));
    assert!(forbidden.refuses(&drive));
    assert!(forbidden.refuses(Path::new(r"\\server\share\docs")));
    assert!(!forbidden.refuses(&profile_child));
    assert!(!forbidden.refuses(&root.join("work")));
}

struct Host {
    services: TestServices,
    provider_id: ProviderId,
}

impl Host {
    fn new() -> Self {
        let services = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let provider_id = services
            .database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories.providers.create(&ProviderDraft::new(
                    ProviderKind::Ollama,
                    "Fake",
                    "http://localhost:11434",
                    vec![ProviderCapability::Text],
                    1,
                ))?;
                repositories.models.replace_for_provider(
                    &provider.id,
                    &[ModelDraft {
                        context_window: Some(1024),
                        max_output: Some(128),
                        ..ModelDraft::new("fake-model", "Fake", 1)
                    }],
                    1,
                )?;
                Ok(provider.id)
            })
            .expect("seeds a provider");
        let folders = services.folders.clone();
        services
            .nodes
            .register(FnNode::new(
                NODE,
                PermissionRequirement::None,
                true,
                move |context: StepContext, input: Value| {
                    let folders = folders.clone();
                    async move { access(&folders, &context, &input).await }
                },
            ))
            .expect("registers");
        Self {
            services,
            provider_id: ProviderId::parse(&provider_id).expect("a provider id"),
        }
    }

    fn folders(&self) -> &FolderAccessService {
        &self.services.folders
    }

    fn conversation(&self) -> ConversationId {
        self.services
            .chat
            .create(&CreateConversationInput::new(
                self.provider_id,
                "fake-model".to_owned(),
            ))
            .expect("creates")
            .id
    }

    fn start(&self, conversation: ConversationId, calls: Value) -> RunHandleDto {
        self.services
            .runs
            .start(
                zvs_contract::parse(json!({
                    "kind": "job",
                    "graph": { "nodes": [{
                        "id": "a",
                        "type": NODE,
                        "input": { "conversationId": conversation.to_string(), "calls": calls }
                    }] }
                }))
                .expect("a start input"),
            )
            .expect("starts")
    }

    fn approvals(&self, run: &RunId) -> Vec<serde_json::Map<String, Value>> {
        self.services
            .run_events
            .events()
            .into_iter()
            .filter_map(|event| match event.body {
                HostEventDraft::Approval { request } => Some(request),
                _ => None,
            })
            .filter(|request| request.get("runId") == Some(&json!(run.to_string())))
            .collect()
    }

    async fn approval(&self, run: &RunId, index: usize) -> serde_json::Map<String, Value> {
        for _ in 0..5_000 {
            if let Some(request) = self.approvals(run).get(index) {
                return request.clone();
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("approval {index} was never requested");
    }

    async fn answer(&self, run: &RunId, index: usize, answer: FolderAccessAnswer) {
        let request = self.approval(run, index).await;
        self.folders()
            .answer(run, request["id"].as_str().expect("an id"), answer)
            .expect("answers");
    }

    async fn finish(&self, run: &RunId) -> (RunStatus, Option<AppErrorCode>) {
        tokio::time::timeout(Duration::from_secs(120), self.services.runs.wait(run))
            .await
            .expect("finishes in time");
        let run = self.services.runs.get(run).expect("a run");
        (run.status, run.outcome.and_then(|outcome| outcome.code))
    }

    fn bind(
        &self,
        conversation: ConversationId,
        path: &Path,
        grant: Option<FolderGrantScope>,
    ) -> Result<BindFolderResultDto, zvs_contract::AppError> {
        self.services.chat.bind_folder(&BindFolderInput {
            id: conversation,
            path: path.to_string_lossy().into_owned(),
            grant,
        })
    }

    fn access_of(&self, conversation: ConversationId) -> Option<WorkingFolderAccess> {
        self.services
            .chat
            .get(&conversation)
            .expect("a conversation")
            .conversation
            .working_folder_access
    }
}

async fn access(
    folders: &FolderAccessService,
    context: &StepContext,
    input: &Value,
) -> Result<Value, zvs_contract::AppError> {
    let conversation = input["conversationId"].as_str().unwrap_or_default();
    let mut granted = Vec::new();
    for call in input["calls"].as_array().into_iter().flatten() {
        let handle = folders.call(conversation, "files.write");
        for step in call.as_array().into_iter().flatten() {
            let targets: Vec<FolderTarget> = step
                .as_array()
                .into_iter()
                .flatten()
                .map(|target| {
                    let path = target["path"].as_str().unwrap_or_default();
                    if target["folder"].as_bool() == Some(true) {
                        FolderTarget::folder(path)
                    } else {
                        FolderTarget::file(path)
                    }
                })
                .collect();
            for resolved in folders
                .ensure_access(context, &handle, "Запись файла", &targets)
                .await?
            {
                granted.push(resolved.folder.to_string_lossy().into_owned());
            }
        }
    }
    Ok(json!(granted))
}

fn file(path: &Path) -> Value {
    json!({ "path": path.to_string_lossy() })
}

fn one(path: &Path) -> Value {
    json!([[[file(path)]]])
}

#[tokio::test]
async fn once_lets_the_call_through_and_the_next_call_asks_again() {
    let host = Host::new();
    let folder = temporary_directory("once-");
    let conversation = host.conversation();
    let handle = host.start(
        conversation,
        json!([
            [[file(&folder.join("a.txt"))], [file(&folder.join("b.txt"))]],
            [[file(&folder.join("c.txt"))]]
        ]),
    );
    let request = host.approval(&handle.id, 0).await;
    assert_eq!(request["kind"], json!("folder"));
    assert_eq!(request["subject"], json!(FOLDER_SUBJECT));
    assert_eq!(request["tool"], json!("files.write"));
    assert_eq!(request["action"], json!("Запись файла"));
    assert_eq!(request["conversationId"], json!(conversation.to_string()));
    assert_eq!(
        request["folders"],
        json!([{ "path": settled(&folder).to_string_lossy(), "creates": [] }])
    );
    host.answer(&handle.id, 0, FolderAccessAnswer::Once).await;
    host.answer(&handle.id, 1, FolderAccessAnswer::Once).await;
    assert_eq!(host.finish(&handle.id).await, (RunStatus::Succeeded, None));
    assert_eq!(host.approvals(&handle.id).len(), 2);
    assert!(host.folders().grants().expect("lists").is_empty());
}

#[tokio::test]
async fn a_denied_request_fails_the_call_with_permission_denied() {
    let host = Host::new();
    let folder = temporary_directory("deny-");
    let handle = host.start(host.conversation(), one(&folder.join("a.txt")));
    host.answer(&handle.id, 0, FolderAccessAnswer::Deny).await;
    assert_eq!(
        host.finish(&handle.id).await,
        (RunStatus::Failed, Some(AppErrorCode::PermissionDenied))
    );
    assert!(host.folders().grants().expect("lists").is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_request_times_out_as_denied() {
    let host = Host::new();
    let folder = temporary_directory("timeout-");
    let handle = host.start(host.conversation(), one(&folder.join("a.txt")));
    host.approval(&handle.id, 0).await;
    assert_eq!(
        host.finish(&handle.id).await,
        (RunStatus::Failed, Some(AppErrorCode::PermissionDenied))
    );
}

#[tokio::test]
async fn a_dialog_grant_covers_its_chat_only() {
    let host = Host::new();
    let folder = temporary_directory("dialog-");
    let first = host.conversation();
    let second = host.conversation();
    let handle = host.start(first, one(&folder.join("a.txt")));
    host.answer(&handle.id, 0, FolderAccessAnswer::Conversation)
        .await;
    assert_eq!(host.finish(&handle.id).await, (RunStatus::Succeeded, None));

    let again = host.start(first, one(&folder.join("nested").join("b.txt")));
    assert_eq!(host.finish(&again.id).await, (RunStatus::Succeeded, None));
    assert!(host.approvals(&again.id).is_empty());

    let other = host.start(second, one(&folder.join("a.txt")));
    host.answer(&other.id, 0, FolderAccessAnswer::Deny).await;
    assert_eq!(
        host.finish(&other.id).await,
        (RunStatus::Failed, Some(AppErrorCode::PermissionDenied))
    );
    let grants = host.folders().grants().expect("lists");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].scope, FolderGrantScope::Conversation);
    assert_eq!(grants[0].conversation_id, Some(first.to_string()));
    assert!(grants[0].last_used_at.is_some());
}

#[tokio::test]
async fn an_always_grant_lets_another_chat_bind_without_a_request() {
    let host = Host::new();
    let folder = temporary_directory("always-");
    let handle = host.start(host.conversation(), one(&folder.join("a.txt")));
    host.answer(&handle.id, 0, FolderAccessAnswer::Always).await;
    assert_eq!(host.finish(&handle.id).await, (RunStatus::Succeeded, None));
    let other = host.conversation();
    match host.bind(other, folder.path(), None).expect("binds") {
        BindFolderResultDto::Bound { conversation } => {
            assert_eq!(
                conversation.working_folder,
                Some(settled(&folder).to_string_lossy().into_owned())
            );
            assert_eq!(
                conversation.working_folder_access,
                Some(WorkingFolderAccess::Granted)
            );
        }
        BindFolderResultDto::NeedsGrant { .. } => panic!("asked again"),
    }
    let listed = host.folders().list().expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].scope, FolderGrantScope::Always);
    assert_eq!(listed[0].bound_conversations.len(), 1);
    assert_eq!(listed[0].bound_conversations[0].id, other);
}

#[tokio::test]
async fn several_uncovered_folders_in_one_call_raise_one_request() {
    let host = Host::new();
    let first = temporary_directory("first-");
    let second = temporary_directory("second-");
    let handle = host.start(
        host.conversation(),
        json!([[[
            file(&first.join("a.txt")),
            file(&first.join("out").join("b.txt")),
            file(&second.join("c.txt")),
            json!({ "path": first.join("sub").to_string_lossy(), "folder": true })
        ]]]),
    );
    let request = host.approval(&handle.id, 0).await;
    assert_eq!(
        request["folders"],
        json!([
            { "path": settled(&first).to_string_lossy(), "creates": ["out", "sub"] },
            { "path": settled(&second).to_string_lossy(), "creates": [] }
        ])
    );
    host.answer(&handle.id, 0, FolderAccessAnswer::Once).await;
    assert_eq!(host.finish(&handle.id).await, (RunStatus::Succeeded, None));
    assert_eq!(host.approvals(&handle.id).len(), 1);
}

#[tokio::test]
async fn forbidden_folders_are_refused_without_a_request() {
    let host = Host::new();
    let inside = host.services.workspace.join("data");
    std::fs::create_dir_all(&inside).expect("creates");
    let parent = host.services.workspace.path().parent().expect("a parent");
    for target in [inside.join("a.txt"), parent.join("a.txt")] {
        let handle = host.start(host.conversation(), one(&target));
        assert_eq!(
            host.finish(&handle.id).await,
            (RunStatus::Failed, Some(AppErrorCode::PermissionDenied))
        );
        assert!(host.approvals(&handle.id).is_empty());
    }
}

#[tokio::test]
async fn a_relative_path_resolves_against_the_working_folder() {
    let host = Host::new();
    let folder = temporary_directory("relative-");
    let conversation = host.conversation();
    let unbound = host.start(conversation, one(Path::new("a.txt")));
    assert_eq!(host.finish(&unbound.id).await.0, RunStatus::Failed);
    assert_eq!(
        host.services
            .runs
            .get(&unbound.id)
            .expect("a run")
            .error
            .as_deref(),
        Some(NO_WORKING_FOLDER)
    );
    assert!(host.approvals(&unbound.id).is_empty());
    host.bind(
        conversation,
        folder.path(),
        Some(FolderGrantScope::Conversation),
    )
    .expect("binds");
    let handle = host.start(
        conversation,
        one(Path::new("notes").join("a.txt").as_path()),
    );
    assert_eq!(host.finish(&handle.id).await, (RunStatus::Succeeded, None));
    assert!(host.approvals(&handle.id).is_empty());
}

#[tokio::test]
async fn binding_asks_only_for_uncovered_folders_and_refuses_forbidden_ones() {
    let host = Host::new();
    let folder = temporary_directory("bind-");
    let conversation = host.conversation();
    assert_eq!(
        host.bind(conversation, folder.path(), None)
            .expect("answers"),
        BindFolderResultDto::NeedsGrant {
            path: settled(&folder).to_string_lossy().into_owned()
        }
    );
    assert_eq!(host.access_of(conversation), None);
    assert!(matches!(
        host.bind(
            conversation,
            folder.path(),
            Some(FolderGrantScope::Conversation)
        )
        .expect("binds"),
        BindFolderResultDto::Bound { .. }
    ));
    assert_eq!(
        host.access_of(conversation),
        Some(WorkingFolderAccess::Granted)
    );
    let nested = folder.join("nested");
    std::fs::create_dir(&nested).expect("creates");
    assert!(matches!(
        host.bind(conversation, &nested, None).expect("binds"),
        BindFolderResultDto::Bound { .. }
    ));

    let forbidden = host.services.workspace.path().to_path_buf();
    let parent = forbidden.parent().expect("a parent").to_path_buf();
    for path in [forbidden, parent] {
        assert_eq!(
            host.bind(conversation, &path, Some(FolderGrantScope::Always))
                .expect_err("refused")
                .code,
            AppErrorCode::PermissionDenied
        );
    }
    assert_eq!(
        host.bind(conversation, &folder.join("missing"), None)
            .expect_err("missing")
            .code,
        AppErrorCode::NotFound
    );
    assert_eq!(host.folders().grants().expect("lists").len(), 1);
}

#[tokio::test]
async fn binding_is_refused_while_a_turn_runs() {
    let host = Host::new();
    let folder = temporary_directory("busy-");
    let conversation = host.conversation();
    host.services
        .database
        .with_connection(|connection| {
            Repositories::new(connection).runs.create(&RunDraft {
                id: new_id().hyphenated().to_string(),
                kind: RunKind::Chat,
                subject_id: Some(conversation.to_string()),
                title: None,
                retry_of_id: None,
                status: RunStatus::Running,
                graph: serde_json::from_value::<RunGraph>(
                    json!({ "nodes": [{ "id": "a", "type": "chat.prepare" }] }),
                )
                .expect("a graph"),
                input: json!({}),
                stream_id: new_id().hyphenated().to_string(),
                concurrency: 1,
                created_at: 1,
                started_at: Some(1),
            })?;
            Ok(())
        })
        .expect("seeds a running turn");
    assert_eq!(
        host.bind(conversation, folder.path(), Some(FolderGrantScope::Always))
            .expect_err("busy")
            .code,
        AppErrorCode::Conflict
    );
    assert_eq!(
        host.services
            .chat
            .unbind_folder(&conversation)
            .expect_err("busy")
            .code,
        AppErrorCode::Conflict
    );
}

#[tokio::test]
async fn revoking_keeps_the_binding_and_marks_access_missing() {
    let host = Host::new();
    let folder = temporary_directory("revoke-");
    let conversation = host.conversation();
    host.bind(conversation, folder.path(), Some(FolderGrantScope::Always))
        .expect("binds");
    let grant = host.folders().list().expect("lists").remove(0);
    host.folders().revoke(&grant.id).expect("revokes");
    let detail = host
        .services
        .chat
        .get(&conversation)
        .expect("a conversation");
    assert_eq!(
        detail.conversation.working_folder,
        Some(settled(&folder).to_string_lossy().into_owned())
    );
    assert_eq!(
        detail.conversation.working_folder_access,
        Some(WorkingFolderAccess::Missing)
    );
    assert_eq!(
        host.folders().revoke(&grant.id).expect_err("gone").code,
        AppErrorCode::NotFound
    );
    let handle = host.start(conversation, one(Path::new("a.txt")));
    host.answer(&handle.id, 0, FolderAccessAnswer::Deny).await;
    assert_eq!(
        host.finish(&handle.id).await,
        (RunStatus::Failed, Some(AppErrorCode::PermissionDenied))
    );
    let unbound = host
        .services
        .chat
        .unbind_folder(&conversation)
        .expect("unbinds");
    assert_eq!(unbound.working_folder, None);
    assert_eq!(unbound.working_folder_access, None);
}

#[tokio::test]
async fn deleting_a_chat_deletes_its_dialog_grants() {
    let host = Host::new();
    let folder = temporary_directory("cascade-");
    let kept = temporary_directory("kept-");
    let conversation = host.conversation();
    host.bind(
        conversation,
        folder.path(),
        Some(FolderGrantScope::Conversation),
    )
    .expect("binds");
    host.bind(conversation, kept.path(), Some(FolderGrantScope::Always))
        .expect("binds");
    assert_eq!(host.folders().grants().expect("lists").len(), 2);
    host.services.chat.remove(&conversation).expect("removes");
    let grants = host.folders().grants().expect("lists");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].scope, FolderGrantScope::Always);
}

#[tokio::test]
async fn a_stale_answer_conflicts() {
    let host = Host::new();
    let folder = temporary_directory("stale-");
    let handle = host.start(host.conversation(), one(&folder.join("a.txt")));
    let request = host.approval(&handle.id, 0).await;
    let id = request["id"].as_str().expect("an id").to_owned();
    host.folders()
        .answer(&handle.id, &id, FolderAccessAnswer::Once)
        .expect("answers");
    host.finish(&handle.id).await;
    assert_eq!(
        host.folders()
            .answer(&handle.id, &id, FolderAccessAnswer::Always)
            .expect_err("stale")
            .code,
        AppErrorCode::Conflict
    );
    assert!(host.folders().grants().expect("lists").is_empty());
}
