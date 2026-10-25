use zvs_contract::{CreateSkillInput, SkillFileInput};

use super::*;
use crate::artifacts::{ArtifactStore, DASHBOARDS};
use crate::files::VirtualRoots;
use crate::files::roots::{ROOTS_ONLY, SKILL_READ_ONLY};
use crate::skills::library::{SKILL_FILE_NOT_FOUND, SKILL_SCRIPT_OUTSIDE};
use crate::skills::script::{SCRIPT_NO_TOOLCHAIN, SCRIPT_UNSUPPORTED};

const KEY: &str = "extract";

impl Host {
    fn with_skill(self) -> Self {
        self.services
            .skills
            .create(&CreateSkillInput {
                key: KEY.to_owned(),
                name: "Извлечение".to_owned(),
                description: "Извлекает таблицы из документов".to_owned(),
                instructions: "Запустите scripts/extract.py.".to_owned(),
                allowed_tools: None,
                model_override: None,
                enabled: None,
                files: Some(
                    [
                        ("scripts/extract.py", "print('ok')"),
                        ("scripts/build.mjs", "console.log('ok')"),
                        ("scripts/notes.txt", "текст"),
                        ("references/guide.md", "Руководство"),
                    ]
                    .map(|(path, content)| SkillFileInput {
                        path: path.to_owned(),
                        content: content.to_owned(),
                        encoding: None,
                    })
                    .to_vec(),
                ),
            })
            .expect("creates the skill");
        self
    }

    fn store(&self) -> ArtifactStore {
        ArtifactStore::new(self.services.workspace.join("script-artifacts"))
            .with_library(DASHBOARDS, self.services.workspace.join("dashboards"))
    }

    fn script_node(
        &self,
        launcher: Option<Arc<dyn CommandLauncher>>,
        store: Option<ArtifactStore>,
        limits: ProcessLimits,
    ) -> ShellRunNode {
        ShellRunNode::new(
            &self.services.folders,
            self.services.runs.approvals(),
            CommandToolOptions {
                limits,
                roots: VirtualRoots::new(store, Some(self.services.skills.library().clone())),
                ..self.options(launcher)
            },
        )
    }
}

fn script(path: &str) -> ShellRunInput {
    ShellRunInput {
        argv: [&format!("skill:{KEY}/{path}"), "--table", "1"]
            .map(|part| (*part).to_owned())
            .to_vec(),
        ..shell(&[])
    }
}

#[tokio::test]
async fn only_runnable_files_of_the_scripts_folder_start() {
    let host = Host::new().with_skill();
    let stub = Stub::shared(&[]);
    let node = host.script_node(Some(stub.clone()), None, ProcessLimits::default());
    let outside = node
        .run(
            &context(CancellationToken::new()),
            script("references/guide.md"),
        )
        .await
        .expect_err("a reference is not a script");
    assert_eq!(outside.code, AppErrorCode::PermissionDenied);
    assert_eq!(outside.message, SKILL_SCRIPT_OUTSIDE);
    for path in ["scripts/../SKILL.md", "scripts/none.py"] {
        let error = node
            .run(&context(CancellationToken::new()), script(path))
            .await
            .expect_err(path);
        assert_eq!(error.message, SKILL_FILE_NOT_FOUND, "{path}");
    }
    let unsupported = node
        .run(
            &context(CancellationToken::new()),
            script("scripts/notes.txt"),
        )
        .await
        .expect_err("a text file is not runnable");
    assert_eq!(unsupported.message, SCRIPT_UNSUPPORTED);
    assert!(stub.calls().is_empty());
}

#[tokio::test]
async fn a_missing_runtime_refuses_the_script_before_anything_starts() {
    let host = Host::new().with_skill();
    let toolchain: Arc<dyn CommandLauncher> = Arc::new(host.services.toolchain.clone());
    let node = host.script_node(Some(toolchain), None, ProcessLimits::default());
    for path in ["scripts/extract.py", "scripts/build.mjs"] {
        let error = node
            .run(&context(CancellationToken::new()), script(path))
            .await
            .expect_err(path);
        assert_eq!(error.code, AppErrorCode::NotFound, "{path}");
        assert!(error.message.ends_with(SETTINGS_HINT), "{path}");
    }
    let none = host
        .script_node(None, None, ProcessLimits::default())
        .run(
            &context(CancellationToken::new()),
            script("scripts/extract.py"),
        )
        .await
        .expect_err("no toolchain");
    assert_eq!(none.message, SCRIPT_NO_TOOLCHAIN);
}

#[tokio::test]
async fn outside_a_chat_a_script_runs_in_a_scratch_folder_with_its_skill_dir() {
    let host = Host::new().with_skill();
    let stub = Stub::shared(&[]);
    let node = host.script_node(Some(stub.clone()), None, ProcessLimits::default());
    let run = context(CancellationToken::new());
    let output = node
        .run(
            &run,
            ShellRunInput {
                stdin: Some("строка".to_owned()),
                ..script("scripts/extract.py")
            },
        )
        .await
        .expect("runs");
    let calls = stub.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "uv");
    assert_eq!(calls[0].1[0], "run");
    assert!(calls[0].1[1].ends_with("extract.py"));
    assert_eq!(&calls[0].1[2..], ["--table", "1"]);
    assert_eq!(output.exit_code, Some(0));
    assert!(output.stdout.contains("stdin=строка"));
    assert_eq!(value_of(&output.stdout, "PYTHONIOENCODING"), Some("utf-8"));
    let scratch = host
        .services
        .workspace
        .join("code-tests")
        .join(run.run_id.to_string());
    assert!(same_path(
        value_of(&output.stdout, "cwd").expect("a cwd"),
        &scratch
    ));
    let skill = value_of(&output.stdout, "ZVS_SKILL_DIR").expect("the skill folder");
    assert!(
        Path::new(skill)
            .join("scripts")
            .join("extract.py")
            .is_file()
    );
    assert_eq!(value_of(&output.stdout, "ZVS_CHAT_DIR"), Some(""));
    assert_eq!(
        output.process.argv[0],
        format!("skill:{KEY}/scripts/extract.py")
    );
    assert!(output.text.starts_with("exit code: 0\n<stdout>\n"));
    assert!(output.artifacts.is_empty());

    let failing = host
        .script_node(
            Some(Stub::shared(&[("node", "fail")])),
            None,
            ProcessLimits::default(),
        )
        .run(
            &context(CancellationToken::new()),
            script("scripts/build.mjs"),
        )
        .await
        .expect("the run completes");
    assert_eq!(failing.exit_code, Some(3));
    assert!(failing.is_error);
}

#[tokio::test]
async fn in_a_chat_a_script_runs_in_the_chat_folder_and_only_its_results_are_shown() {
    let host = Host::new().with_skill();
    let store = host.store();
    let node = host.script_node(
        Some(Stub::shared(&[("uv", "chat")])),
        Some(store.clone()),
        ProcessLimits::default(),
    );
    let conversation = host.conversation().to_string();
    let output = node
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: conversation.clone(),
                ..script("scripts/extract.py")
            },
        )
        .await
        .expect("runs");
    let folder = crate::chat_files::chat_dir(&store, &conversation).expect("the chat folder");
    assert!(folder.join("_spec.json").is_file());
    assert!(folder.join("_work").join("table.csv").is_file());
    assert_eq!(
        output
            .artifacts
            .iter()
            .map(|artifact| artifact.name.as_str())
            .collect::<Vec<_>>(),
        ["result.csv"]
    );
    assert!(
        output
            .text
            .ends_with("Files shown to the user with download buttons: result.csv")
    );
    let meta = node
        .tool_meta(&Value::Null, &output)
        .expect("the process meta");
    assert_eq!(meta["artifacts"][0]["name"], json!("result.csv"));
    assert!(serde_json::from_value::<ProcessResultDto>(meta).is_ok());

    let echoed = host
        .script_node(
            Some(Stub::shared(&[])),
            Some(store.clone()),
            ProcessLimits::default(),
        )
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: conversation,
                ..script("scripts/extract.py")
            },
        )
        .await
        .expect("runs");
    assert!(same_path(
        value_of(&echoed.stdout, "cwd").expect("a cwd"),
        &folder
    ));
    assert!(same_path(
        value_of(&echoed.stdout, "ZVS_CHAT_DIR").expect("the chat folder"),
        &folder
    ));
}

#[tokio::test]
async fn documents_and_dashboards_a_script_writes_are_kept_as_artifacts() {
    let host = Host::new().with_skill();
    let store = host.store();
    let run = context(CancellationToken::new());
    let output = host
        .script_node(
            Some(Stub::shared(&[("uv", "write")])),
            Some(store.clone()),
            ProcessLimits::default(),
        )
        .run(&run, script("scripts/extract.py"))
        .await
        .expect("runs");
    assert_eq!(output.artifacts.len(), 1);
    let artifact = &output.artifacts[0];
    assert_eq!(artifact.name, "Отчёт.docx");
    assert_eq!(artifact.id, format!("{}/Отчёт.docx", run.run_id));
    assert!(store.resolve(&artifact.id).is_ok());

    let dashboard = host
        .script_node(
            Some(Stub::shared(&[("uv", "dashboard")])),
            Some(store.clone()),
            ProcessLimits::default(),
        )
        .run(
            &context(CancellationToken::new()),
            script("scripts/extract.py"),
        )
        .await
        .expect("runs");
    assert_eq!(
        dashboard
            .artifacts
            .iter()
            .map(|artifact| artifact.id.as_str())
            .collect::<Vec<_>>(),
        ["dashboards/Продажи.html"]
    );
}

#[tokio::test]
async fn a_script_is_capped_and_stopped_by_the_shared_limits() {
    let host = Host::new().with_skill();
    let capped = host
        .script_node(
            Some(Stub::shared(&[("uv", "flood")])),
            None,
            ProcessLimits {
                output_bytes: 4096,
                ..ProcessLimits::default()
            },
        )
        .run(
            &context(CancellationToken::new()),
            script("scripts/extract.py"),
        )
        .await
        .expect("runs");
    assert!(capped.truncated);
    assert_eq!(capped.stdout.len(), 4096);

    let started = Instant::now();
    let stopped = host
        .script_node(
            Some(Stub::shared(&[("uv", "hang")])),
            None,
            ProcessLimits {
                timeout: Duration::from_secs(2),
                ..ProcessLimits::default()
            },
        )
        .run(
            &context(CancellationToken::new()),
            script("scripts/extract.py"),
        )
        .await
        .expect("a stopped script still returns its output");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(stopped.timed_out && stopped.is_error);
    assert_eq!(stopped.exit_code, None);
}

#[tokio::test]
async fn roots_only_runs_skill_scripts_and_nothing_else() {
    let host = Host::new().with_skill();
    let folder = temporary_directory("shell-roots-only-");
    let root = canonical(folder.path()).expect("a canonical path");
    let conversation = host.bound(&root).to_string();
    let node = host.script_node(
        Some(Stub::shared(&[])),
        Some(host.store()),
        ProcessLimits::default(),
    );
    let program = node
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: conversation.clone(),
                roots_only: true,
                ..ShellRunInput {
                    argv: stub_argv(),
                    ..shell(&[])
                }
            },
        )
        .await
        .expect_err("a project program");
    assert_eq!(program.message, ROOTS_ONLY);
    let foreign = node
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: conversation.clone(),
                roots_only: true,
                cwd: Some(root.to_string_lossy().into_owned()),
                ..script("scripts/extract.py")
            },
        )
        .await
        .expect_err("a real cwd");
    assert_eq!(foreign.message, ROOTS_ONLY);
    let skill_cwd = node
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: conversation.clone(),
                cwd: Some(format!("skill:{KEY}/scripts")),
                ..script("scripts/extract.py")
            },
        )
        .await
        .expect_err("a skill folder as cwd");
    assert_eq!(skill_cwd.message, SKILL_READ_ONLY);
    let allowed = node
        .run(
            &context(CancellationToken::new()),
            ShellRunInput {
                conversation_id: conversation,
                roots_only: true,
                cwd: Some("chat:".to_owned()),
                ..script("scripts/extract.py")
            },
        )
        .await
        .expect("runs in the chat folder");
    assert_eq!(allowed.exit_code, Some(0));
}
