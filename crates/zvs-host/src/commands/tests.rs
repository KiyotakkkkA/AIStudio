use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, BackgroundShellDto, BindFolderInput,
    ConversationId, CreateConversationInput, FolderGrantScope, HostEventDraft, ProcessResultDto,
    ProviderCapability, ProviderId, ProviderKind, RunHandleDto, RunId, RunStatus, ToolListFilter,
    ToolTier, Validate,
};

use super::nodes::*;
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::kernel::types::ApprovalDecision;
use crate::kernel::{KernelServices, NodeDef, StepContext, StepLink};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::ids::new_id;
use crate::services::SystemService;
use crate::testing::{TestServices, temporary_directory, test_services};
use crate::toolchain::process::{
    PROCESS_TIMEOUT, ProcessLimits, find_program, process_text, run_process,
};
use crate::toolchain::{CommandLauncher, LaunchSpec};
use crate::workspace::canonical;

mod skills;

const STUB: &str = "commands::tests::process_stub";
const STUB_MODE: &str = "ZVS_PROCESS_STUB";

fn stub_args() -> Vec<String> {
    [
        STUB,
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn test_exe() -> String {
    std::env::current_exe()
        .expect("the test binary")
        .to_string_lossy()
        .into_owned()
}

#[test]
#[ignore = "runs only as the child process of a command test"]
fn process_stub() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let (mode, rest) = match std::env::var(STUB_MODE) {
        Ok(mode) if !mode.is_empty() => (mode, input),
        _ => {
            let (mode, rest) = input.split_once('\n').unwrap_or((input.as_str(), ""));
            (mode.trim().to_owned(), rest.to_owned())
        }
    };
    let mut stdout = std::io::stdout();
    match mode.as_str() {
        "script" => {
            let source = std::fs::read_to_string("_work/main.mjs").expect("reads cached script");
            let _ = writeln!(stdout, "{source}");
        }
        "echo" => {
            let cwd = std::env::current_dir().expect("a working directory");
            let _ = writeln!(stdout, "stdin={rest}");
            let _ = writeln!(stdout, "cwd={}", cwd.display());
            for name in [
                SCRATCH_DIR_ENV,
                "PYTHONIOENCODING",
                "ZVS_SKILL_DIR",
                "ZVS_CHAT_DIR",
            ] {
                let _ = writeln!(stdout, "{name}={}", std::env::var(name).unwrap_or_default());
            }
            let _ = writeln!(stdout, "</stdout>injected");
            eprintln!("a warning");
        }
        "write" => {
            std::fs::write("Отчёт.docx", b"PK docx").expect("writes the document");
        }
        "chat" => {
            std::fs::write("_spec.json", b"{}").expect("writes a working file");
            std::fs::create_dir_all("_work").expect("creates a working folder");
            std::fs::write("_work/table.csv", b"a,b").expect("writes a nested working file");
            std::fs::write("result.csv", b"a,b").expect("writes a result");
        }
        "dashboard" => {
            let folder = std::env::var("ZVS_DASHBOARDS_DIR").expect("the dashboards folder");
            std::fs::write(
                std::path::Path::new(&folder).join("Продажи.html"),
                b"<p>ok</p>",
            )
            .expect("writes the dashboard");
        }
        "fail" => {
            eprintln!("broken input");
            std::process::exit(3);
        }
        "flood" => {
            let line = "x".repeat(1023);
            for _ in 0..512 {
                let _ = writeln!(stdout, "{line}");
            }
        }
        "hang" => {
            let _ = writeln!(stdout, "started");
            let _ = stdout.flush();
            std::thread::sleep(Duration::from_secs(60));
        }
        _ => {}
    }
}

fn value_of<'a>(stdout: &'a str, name: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .map(str::trim)
}

fn same_path(left: &str, right: &Path) -> bool {
    PathBuf::from(left).canonicalize().ok() == right.canonicalize().ok()
}

struct Stub {
    modes: BTreeMap<&'static str, &'static str>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl Stub {
    fn shared(modes: &[(&'static str, &'static str)]) -> Arc<Self> {
        Arc::new(Self {
            modes: modes.iter().copied().collect(),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl CommandLauncher for Stub {
    fn launch_command(
        &self,
        command: &str,
        args: &[String],
    ) -> Option<Result<LaunchSpec, AppError>> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((command.to_owned(), args.to_vec()));
        let mode = self.modes.get(command).copied().unwrap_or("echo");
        Some(Ok(LaunchSpec {
            command: test_exe(),
            args: stub_args(),
            env: BTreeMap::from([(STUB_MODE.to_owned(), mode.to_owned())]),
            cwd: None,
        }))
    }
}

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

fn context(cancel: CancellationToken) -> StepContext {
    StepContext::new(
        RunId::new(new_id()),
        cancel,
        KernelServices::default(),
        Arc::new(NoLink),
    )
}

fn shell(argv: &[&str]) -> ShellRunInput {
    ShellRunInput {
        argv: argv.iter().map(|part| (*part).to_owned()).collect(),
        reason: "Проверка".to_owned(),
        cwd: None,
        stdin: None,
        background: false,
        notify: false,
        conversation_id: String::new(),
    }
}

fn stub_argv() -> Vec<String> {
    std::iter::once(test_exe()).chain(stub_args()).collect()
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
        Self {
            services,
            provider_id: ProviderId::parse(&provider_id).expect("a provider id"),
        }
    }

    fn options(&self, launcher: Option<Arc<dyn CommandLauncher>>) -> CommandToolOptions {
        CommandToolOptions::new(launcher, self.services.workspace.join("code-tests"))
    }

    fn shell_node(&self, limits: ProcessLimits) -> ShellRunNode {
        ShellRunNode::new(
            &self.services.folders,
            self.services.runs.approvals(),
            CommandToolOptions {
                limits,
                ..self.options(None)
            },
        )
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

    fn bound(&self, folder: &Path) -> ConversationId {
        let conversation = self.conversation();
        self.services
            .chat
            .bind_folder(&BindFolderInput {
                id: conversation,
                path: folder.to_string_lossy().into_owned(),
                grant: Some(FolderGrantScope::Conversation),
            })
            .expect("binds");
        conversation
    }

    fn set_tier(&self, key: &str, tier: ToolTier) {
        let tool = self
            .services
            .tools
            .find_by_key(key)
            .expect("lists")
            .expect("the tool");
        self.services
            .tools
            .set_tier(&tool.id.to_string(), tier, "user")
            .expect("sets the tier");
    }

    fn start(&self, node: Value) -> RunHandleDto {
        self.services
            .runs
            .start(
                zvs_contract::parse(json!({ "kind": "job", "graph": { "nodes": [node] } }))
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

    async fn approval(&self, run: &RunId) -> serde_json::Map<String, Value> {
        for _ in 0..10_000 {
            if let Some(request) = self.approvals(run).first() {
                return request.clone();
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("no approval was requested");
    }

    async fn finish(&self, run: &RunId) -> RunStatus {
        tokio::time::timeout(Duration::from_secs(120), self.services.runs.wait(run))
            .await
            .expect("finishes in time");
        self.services.runs.get(run).expect("a run").status
    }

    fn step(&self, run: &RunId) -> (Value, Option<String>) {
        self.services
            .runs
            .steps(run)
            .expect("steps")
            .into_iter()
            .find(|step| step.node_id == "run")
            .map(|step| (step.output.unwrap_or(Value::Null), step.error))
            .unwrap_or((Value::Null, None))
    }
}

#[test]
fn shell_is_a_destructive_builtin_that_asks_by_default() {
    let host = Host::new();
    let tools = host
        .services
        .tools
        .list(&ToolListFilter::default())
        .expect("lists");
    let tool = tools
        .iter()
        .find(|tool| tool.key == SHELL_RUN)
        .expect("shell is a tool");
    assert!(tool.destructive);
    assert!(!tool.read_only);
    assert_eq!(tool.tier, ToolTier::Ask);
    assert_eq!(tool.description, SHELL_DESCRIPTION);
}

#[test]
fn inputs_are_validated() {
    assert!(shell(&[]).validate().is_err());
    assert!(shell(&[" "]).validate().is_err());
    assert!(shell(&["cargo", "check"]).validate().is_ok());
    let mut served = shell(&["npx", "http-server"]);
    served.background = true;
    assert!(served.validate().is_ok());
    served.stdin = Some("input".to_owned());
    assert!(
        served.validate().is_err(),
        "a background process takes no stdin"
    );
    assert!(
        serde_json::from_value::<ShellRunInput>(json!({ "argv": "npm test", "reason": "x" }))
            .is_err(),
        "a shell string is not an argument vector"
    );
    assert!(
        serde_json::from_value::<ShellRunInput>(json!({ "argv": ["npm", "test"] })).is_err(),
        "a run needs a reason"
    );
    assert!(
        serde_json::from_value::<ShellRunInput>(json!({ "argv": ["npm", "test"], "reason": " " }))
            .expect("parses")
            .validate()
            .is_err(),
        "a blank reason is rejected"
    );
    let mut tests = shell(&["cargo", "test"]);
    tests.notify = true;
    assert!(tests.validate().is_err(), "notify needs background");
    tests.background = true;
    assert!(tests.validate().is_ok());
    for argv in [
        &["npx", "http-server", "-p", "8099"][..],
        &["npm", "run", "dev"],
        &["pnpm", "dev"],
        &["npm", "start"],
        &["python", "-m", "http.server"],
        &["cargo", "watch", "-x", "check"],
        &["C:\\tools\\vite.cmd"],
        &["tsc", "--watch"],
    ] {
        let mut served = shell(argv);
        served.background = true;
        assert!(
            served.validate().is_ok(),
            "{argv:?} may run in the background"
        );
        served.notify = true;
        let error = served.validate().expect_err("a server never finishes");
        assert_eq!(error.message, NOTIFY_SERVER, "{argv:?}");
    }
    for argv in [
        &["cargo", "build", "--profile", "dev"][..],
        &["npm", "test"],
        &["pnpm", "run", "build"],
        &["pytest", "tests"],
    ] {
        let parts: Vec<String> = argv.iter().map(|part| (*part).to_owned()).collect();
        assert!(!server_like(&parts), "{argv:?}");
    }
}

#[tokio::test]
async fn shell_run_defaults_to_the_working_folder_and_refuses_without_one() {
    let host = Host::new();
    let folder = temporary_directory("shell-workspace-");
    let root = canonical(folder.path()).expect("a canonical path");
    let node = host.shell_node(ProcessLimits::default());
    let mut input = ShellRunInput {
        argv: stub_argv(),
        stdin: Some("echo\nстрока".to_owned()),
        ..shell(&[])
    };
    input.conversation_id = host.conversation().to_string();
    let refused = node
        .run(&context(CancellationToken::new()), input.clone())
        .await
        .expect_err("no folder");
    assert_eq!(refused.code, AppErrorCode::ValidationFailed);
    assert_eq!(refused.message, NO_FOLDER);

    input.conversation_id = host.bound(&root).to_string();
    let output = node
        .run(&context(CancellationToken::new()), input)
        .await
        .expect("runs");
    assert_eq!(output.exit_code, Some(0));
    assert!(output.stdout.contains("stdin=строка"));
    assert!(same_path(
        value_of(&output.stdout, "cwd").expect("a cwd"),
        &root
    ));
    assert!(same_path(&output.process.cwd, &root));
    assert_eq!(output.process.argv, stub_argv());
    assert!(output.process.source.is_none());

    let missing = node
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: host.bound(&root).to_string(),
                ..shell(&["zvs-no-such-program-anywhere"])
            },
        )
        .await
        .expect_err("not on PATH");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert_eq!(missing.message, PROGRAM_NOT_FOUND);
}

#[test]
fn programs_resolve_through_the_toolchain_folders_first() {
    let tools = PathBuf::from("tools");
    let system = PathBuf::from("system");
    let present = |path: &Path| {
        let text = path
            .to_string_lossy()
            .to_ascii_lowercase()
            .replace('\\', "/");
        [
            "tools/npm.cmd",
            "tools/npm",
            "system/npm.cmd",
            "system/npm",
            "system/cargo.exe",
            "system/cargo",
            "work/bin/run.bat",
            "work/bin/run",
        ]
        .contains(&text.as_str())
    };
    let dirs = [tools.clone(), system.clone()];
    let cwd = Path::new("work");
    let found = |name: &str| find_program(name, &dirs, cwd, Some(".EXE;.CMD;.BAT"), &present);
    let windows = cfg!(windows);
    assert_eq!(
        found("npm"),
        Some(tools.join(if windows { "npm.cmd" } else { "npm" }))
    );
    assert_eq!(
        found("cargo"),
        Some(system.join(if windows { "cargo.exe" } else { "cargo" }))
    );
    assert_eq!(
        found("bin/run"),
        Some(
            cwd.join("bin")
                .join(if windows { "run.bat" } else { "run" })
        )
    );
    assert_eq!(found("python"), None);
}

#[tokio::test]
async fn shell_asks_even_at_auto_and_off_denies() {
    let host = Host::new();
    let folder = temporary_directory("ask-");
    let root = canonical(folder.path()).expect("a canonical path");
    let conversation = host.bound(&root);
    host.set_tier(SHELL_RUN, ToolTier::Auto);

    let shell_node = json!({
        "id": "run",
        "type": SHELL_RUN,
        "input": {
            "conversationId": conversation.to_string(),
            "argv": stub_argv(),
            "reason": "Проверка",
            "stdin": "echo\nhello"
        }
    });
    let handle = host.start(shell_node.clone());
    let request = host.approval(&handle.id).await;
    assert_eq!(request["kind"], json!("process"));
    assert_eq!(request["subject"], json!(SHELL_RUN));
    assert_eq!(request["tool"], json!(SHELL_RUN));
    assert_eq!(request["action"], json!(SHELL_ACTION));
    assert_eq!(request["conversationId"], json!(conversation.to_string()));
    assert_eq!(request["process"]["argv"], json!(stub_argv()));
    assert_eq!(request["process"]["reason"], json!("Проверка"));
    assert!(same_path(
        request["process"]["cwd"].as_str().expect("a cwd"),
        &root
    ));
    let approval: zvs_contract::ProcessApprovalDto =
        zvs_contract::parse(Value::Object(request.clone())).expect("a process approval");
    assert_eq!(approval.process.argv, stub_argv());
    host.services
        .runs
        .approve(&handle.id, request["id"].as_str().expect("an id"), false)
        .expect("approves");
    assert_eq!(host.finish(&handle.id).await, RunStatus::Succeeded);
    let (output, _) = host.step(&handle.id);
    assert!(
        output["stdout"]
            .as_str()
            .is_some_and(|text| text.contains("stdin=hello"))
    );

    host.set_tier(SHELL_RUN, ToolTier::Off);
    let handle = host.start(shell_node);
    assert_eq!(host.finish(&handle.id).await, RunStatus::Failed);
    assert!(host.approvals(&handle.id).is_empty());
}

#[tokio::test]
async fn a_foreign_cwd_goes_through_folder_access() {
    let host = Host::new();
    let bound = temporary_directory("bound-");
    let other = temporary_directory("other-");
    let other_root = canonical(other.path()).expect("a canonical path");
    let conversation = host.bound(&canonical(bound.path()).expect("a canonical path"));
    let handle = host.start(json!({
        "id": "run",
        "type": SHELL_RUN,
        "input": {
            "conversationId": conversation.to_string(),
            "argv": stub_argv(),
            "reason": "Проверка",
            "cwd": other_root.to_string_lossy(),
            "stdin": "echo"
        }
    }));
    let request = host.approval(&handle.id).await;
    assert_eq!(request["kind"], json!("folder"));
    assert_eq!(request["tool"], json!(SHELL_RUN));
    assert_eq!(
        request["folders"][0]["path"],
        json!(other_root.to_string_lossy())
    );
    host.services
        .folders
        .answer(
            &handle.id,
            request["id"].as_str().expect("an id"),
            zvs_contract::FolderAccessAnswer::Deny,
        )
        .expect("answers");
    assert_eq!(host.finish(&handle.id).await, RunStatus::Failed);
    assert_eq!(host.approvals(&handle.id).len(), 1);
}

#[tokio::test]
async fn an_owned_absolute_cwd_needs_only_process_approval() {
    let host = Host::new();
    let conversation = host.conversation();
    let folder = host
        .services
        .artifacts
        .folders()
        .folder(&conversation.to_string())
        .unwrap()
        .join("attachments/browser");
    std::fs::create_dir_all(&folder).unwrap();
    let cwd = canonical(&folder).unwrap();
    let handle = host.start(json!({ "id": "run", "type": SHELL_RUN, "input": { "conversationId": conversation, "argv": stub_argv(), "reason": "Проверка", "cwd": cwd, "stdin": "echo" } }));
    let request = host.approval(&handle.id).await;
    assert_eq!(request["kind"], json!("process"));
    assert!(same_path(request["process"]["cwd"].as_str().unwrap(), &cwd));
    host.services
        .runs
        .approve(&handle.id, request["id"].as_str().unwrap(), false)
        .unwrap();
    assert_eq!(host.finish(&handle.id).await, RunStatus::Succeeded);
    assert_eq!(host.approvals(&handle.id).len(), 1);
}

#[tokio::test]
async fn output_is_capped_and_fenced_through_the_shared_runner() {
    let host = Host::new();
    let folder = temporary_directory("caps-");
    let root = canonical(folder.path()).expect("a canonical path");
    let limits = ProcessLimits {
        output_bytes: 4096,
        ..ProcessLimits::default()
    };
    let output = host
        .shell_node(limits)
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                argv: stub_argv(),
                stdin: Some("flood".to_owned()),
                conversation_id: host.bound(&root).to_string(),
                ..shell(&[])
            },
        )
        .await
        .expect("runs");
    assert!(output.truncated);
    assert_eq!(output.stdout.len(), 4096);
    assert!(
        output
            .text
            .contains("[output truncated to 4096 bytes per stream]")
    );
    assert!(output.text.starts_with("exit code: 0\n"));
    assert!(output.text.ends_with("</stderr>"));
}

fn hang_spec() -> LaunchSpec {
    LaunchSpec {
        command: test_exe(),
        args: stub_args(),
        env: BTreeMap::from([(STUB_MODE.to_owned(), "hang".to_owned())]),
        cwd: None,
    }
}

#[tokio::test]
async fn the_shared_runner_stops_a_process_past_its_time_limit() {
    let limits = ProcessLimits {
        timeout: Duration::from_secs(2),
        ..ProcessLimits::default()
    };
    let started = Instant::now();
    let run = run_process(
        &hang_spec(),
        None,
        limits,
        &CancellationToken::new(),
        SHELL_NOT_STARTED,
    )
    .await
    .expect("a stopped process still returns its output");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(run.timed_out && run.failed());
    assert_eq!(run.exit_code, None);
    assert!(run.stdout.contains("started"));
    assert!(
        process_text(&run, "process", limits).contains("[the process was stopped after 2 seconds]")
    );
}

#[tokio::test]
async fn cancelling_kills_the_process_tree() {
    let cancel = CancellationToken::new();
    let started = Instant::now();
    let task = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            run_process(
                &hang_spec(),
                None,
                ProcessLimits {
                    timeout: PROCESS_TIMEOUT,
                    output_bytes: 1024,
                },
                &cancel,
                SHELL_NOT_STARTED,
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(1500)).await;
    cancel.cancel();
    let error = task
        .await
        .expect("the task joins")
        .expect_err("a cancelled process fails");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[tokio::test]
async fn a_program_that_cannot_start_reports_the_caller_message() {
    let error = run_process(
        &LaunchSpec {
            command: std::env::temp_dir()
                .join("zvs-missing-program.exe")
                .to_string_lossy()
                .into_owned(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
        },
        None,
        ProcessLimits::default(),
        &CancellationToken::new(),
        SHELL_NOT_STARTED,
    )
    .await
    .expect_err("nothing to start");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert_eq!(error.message, SHELL_NOT_STARTED);
}

#[tokio::test]
async fn a_background_start_is_approved_like_a_foreground_run_and_returns_a_handle() {
    let host = Host::new();
    let folder = temporary_directory("background-");
    let root = canonical(folder.path()).expect("a canonical path");
    let conversation = host.bound(&root);
    let handle = host.start(json!({
        "id": "run",
        "type": SHELL_RUN,
        "input": {
            "conversationId": conversation.to_string(),
            "argv": stub_argv(),
            "reason": "Проверка",
            "background": true
        }
    }));
    let request = host.approval(&handle.id).await;
    assert_eq!(request["kind"], json!("process"));
    assert_eq!(request["process"]["background"], json!(true));
    let approval: zvs_contract::ProcessApprovalDto =
        zvs_contract::parse(Value::Object(request.clone())).expect("a process approval");
    assert!(approval.process.background);
    host.services
        .runs
        .approve(&handle.id, request["id"].as_str().expect("an id"), false)
        .expect("approves");
    assert_eq!(host.finish(&handle.id).await, RunStatus::Succeeded);
    let (output, _) = host.step(&handle.id);
    let id = output["shellId"].as_str().expect("a shell id").to_owned();
    assert!(output["exitCode"].is_null());
    assert!(
        output["text"]
            .as_str()
            .is_some_and(|text| text.starts_with("Started in the background."))
    );
    let listed = host.services.shells.list(&conversation.to_string());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id.to_string(), id);
    host.services
        .shells
        .close_owner(&conversation.to_string())
        .await;
    assert!(
        host.services
            .shells
            .list(&conversation.to_string())
            .is_empty()
    );
}

#[tokio::test]
async fn a_run_without_a_dialog_cannot_go_to_the_background() {
    let host = Host::new();
    let node = host.shell_node(ProcessLimits::default());
    let mut orphan = shell(&["cargo", "check"]);
    orphan.background = true;
    let error = node
        .run(&context(CancellationToken::new()), orphan)
        .await
        .expect_err("refused");
    assert_eq!(error.message, crate::background::NO_OWNER);
}

async fn ended(host: &Host, id: &zvs_contract::BackgroundShellId) -> BackgroundShellDto {
    for _ in 0..2400 {
        let shell = host.services.shells.get(None, id).expect("the shell");
        if !shell.status.running() {
            return shell;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the background process did not end in time");
}

#[tokio::test]
async fn a_background_program_that_finishes_is_reported_to_the_wake_up() {
    let host = Host::new();
    let node = ShellRunNode::new(
        &host.services.folders,
        host.services.runs.approvals(),
        CommandToolOptions {
            shells: Some(host.services.shells.clone()),
            ..host.options(None)
        },
    );
    let folder = temporary_directory("notify-shell-");
    let conversation = host
        .bound(&canonical(folder.path()).expect("a path"))
        .to_string();
    let mut input = ShellRunInput {
        argv: stub_argv(),
        ..shell(&[])
    };
    input.background = true;
    input.notify = true;
    input.conversation_id = conversation.clone();
    let output = node
        .run(&context(CancellationToken::new()), input)
        .await
        .expect("starts");
    let shell = ended(&host, &output.shell_id.expect("a process id")).await;
    assert_eq!(shell.exit_code, Some(0));
    assert!(host.services.shells.take_ended(&conversation, 5).is_empty());
    let notified = host.services.shells.take_notified(&conversation, 5);
    assert_eq!(notified.len(), 1);
    assert_eq!(notified[0].shell.exit_code, Some(0));
    host.services.shells.close_owner(&conversation).await;
}
