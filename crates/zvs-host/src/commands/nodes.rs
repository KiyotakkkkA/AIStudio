use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::{check, invalid};
use zvs_contract::{
    AppError, AppErrorCode, ArtifactDto, BackgroundShellDto, BackgroundShellId, CodeLanguage,
    PROCESS_APPROVAL_KIND, PROCESS_ARG_MAX, PROCESS_ARGV_MAX, PROCESS_PACKAGE_MAX,
    PROCESS_PACKAGES_MAX, PROCESS_SOURCE_MAX, ProcessPlanDto, ProcessResultDto,
    ToolAuthorisationDto, ToolAuthorisationKind, ToolTier, Validate,
};

use crate::artifacts::{ArtifactStore, Snapshot, library_env, mentioned_paths, produced, snapshot};
use crate::background::{BackgroundShells, NO_OWNER, ShellStart, status_line};
use crate::chat_files::{CHAT_DIR_ENV, is_working};
use crate::conversations::is_owner;
use crate::files::roots::{
    PathRoot, ROOTS_UNAVAILABLE, RootKind, SKILL_READ_ONLY, VirtualRoots, path_root,
};
use crate::kernel::{
    ApprovalDecision, DeferredApproval, NodeDef, NodeRegistry, PermissionRequirement, StepContext,
    review_action,
};
use crate::platform::ids::new_id;
use crate::policy::{ReviewVerdict, ReviewedAction, rule_denied};
use crate::services::approvals::ApprovalService;
use crate::skills::script::{
    SCRIPT_NO_TOOLCHAIN, SCRIPT_NOT_STARTED, SCRIPT_UNSUPPORTED, SKILL_DIR_ENV, ScriptRuntime,
};
use crate::toolchain::CommandLauncher;
use crate::toolchain::LaunchSpec;
use crate::toolchain::process::{
    ProcessLimits, ProcessRun, SCRATCH_FAILED, find_program, joined_path, path_dirs, pin_scratch,
    prepare_scratch, process_text, run_process,
};
use crate::toolchain::service::SETTINGS_HINT;
use crate::workspace::{FolderAccessService, FolderTarget};

pub const CODE_RUN: &str = "code.run";
pub const SHELL_RUN: &str = "shell.run";
pub const PROCESS_TOOLS: [&str; 2] = [CODE_RUN, SHELL_RUN];

pub const CODE_DESCRIPTION: &str = "Runs a short program written by the model. language: python — executed via uv run; dependencies are declared at the top of source as a PEP 723 header (# /// script, dependencies = [\"…\"], # ///); uv installs them into the app's own cache, nothing is installed globally. javascript — executed via Node.js as an ES module (import, not require); packages from packages are installed into a temporary folder via npm install with install scripts disabled. powershell — executed via powershell.exe without a profile. The process working directory is a temporary run folder; the conversation working folder, when set, is passed as the ZVS_WORKSPACE_DIR environment variable. Folder permissions do not sandbox this tool: the process runs with the user's rights, and the user confirms every run from the full program text. Execution time is limited; interactive input is not supported. For a long job pass background: true to get a process id at once (read it with shell.output); add notify: true and the conversation is continued with the result when the program ends. Returns exit code, stdout, and stderr.";
pub const SHELL_DESCRIPTION: &str = "Runs a project program, e.g. npm test or cargo check, or a skill script. argv is an array: argv[0] is the program, the rest are arguments. No shell command line is parsed, so pipes, redirections, and substitutions are unavailable. The program is looked up first in the folders of connected environments, then in PATH. A skill script is referenced as argv[0] = skill:<key>/scripts/<file>: .py runs via uv; .js, .mjs, and .cjs run via Node.js; the skill folder is passed as ZVS_SKILL_DIR. The process working directory is cwd (a path or chat:); without cwd it is the conversation working folder, or chat: for a skill script. Access to a cwd outside granted permissions is confirmed by the user, but the process itself runs with the user's rights. The user confirms every run from the full command. Execution time is limited and interactive input is not supported; standard input comes only from stdin. For a server, watcher or long build pass background: true to get a process id at once. Add notify: true for work that finishes, such as a build, tests or a job: the conversation is continued with the result when the process ends; never for a server or watcher. Returns exit code, stdout, and stderr.";

pub const CODE_ACTION: &str = "запуск кода";
pub const SHELL_ACTION: &str = "запуск команды";

pub const CODE_NOT_STARTED: &str = "Не удалось запустить программу";
pub const SHELL_NOT_STARTED: &str = "Не удалось запустить команду";
pub const CODE_NO_TOOLCHAIN: &str = "Среда выполнения программ недоступна";
pub const NO_FOLDER: &str = "Команде нужна папка: выберите рабочую папку диалога или укажите cwd";
pub const FOLDER_MISSING: &str = "Папка не найдена";
pub const PROGRAM_NOT_FOUND: &str = "Программа не найдена в папках сред и в PATH";
pub const PROCESS_DENIED: &str = "Запуск отклонён";
pub const BACKGROUND_UNAVAILABLE: &str = "Фоновые процессы недоступны";
pub const NOTIFY_SERVER: &str =
    "Продолжение диалога по завершении недоступно для сервера или наблюдателя";
pub const NOTIFY_TEXT: &str = "The conversation will be continued automatically with the result when this process ends; do not poll it with shell.output.";
pub const BACKGROUND_TEXT: &str = "Read its output with shell.output and stop it with shell.kill.";

pub const WORKSPACE_DIR_ENV: &str = "ZVS_WORKSPACE_DIR";
pub const SCRATCH_DIR_ENV: &str = "ZVS_SCRATCH_DIR";
pub const PYTHON_UTF8_ENV: &[(&str, &str)] = &[("PYTHONUTF8", "1"), ("PYTHONIOENCODING", "utf-8")];
pub const POWERSHELL: &str = "powershell.exe";
pub const POWERSHELL_FLAGS: [&str; 5] = [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
];
pub const NPM_INSTALL: [&str; 4] = ["install", "--ignore-scripts", "--no-audit", "--no-fund"];
pub const POWERSHELL_PRELUDE: &str = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\r\n";
pub const PACKAGE_MANIFEST: &str = "{\n  \"private\": true\n}\n";
const STDIN_MAX_CHARS: usize = 1_048_576;
const PATH_MAX_CHARS: usize = 32_767;
const UTF8_BOM: &str = "\u{feff}";

const SERVER_PROGRAMS: [&str; 7] = [
    "nodemon",
    "vite",
    "http-server",
    "live-server",
    "serve",
    "webpack-dev-server",
    "cargo-watch",
];
const SERVER_ARGS: [&str; 5] = ["serve", "runserver", "watch", "--watch", "http-server"];
const SERVER_SCRIPTS: [&str; 5] = ["dev", "start", "serve", "watch", "preview"];
const SCRIPT_RUNNERS: [&str; 4] = ["npm", "pnpm", "yarn", "bun"];

pub fn server_like(argv: &[String]) -> bool {
    let Some(program) = argv.first() else {
        return false;
    };
    let stem = Path::new(&program.to_ascii_lowercase())
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    if SERVER_PROGRAMS.contains(&stem.as_str()) {
        return true;
    }
    let args: Vec<String> = argv[1..]
        .iter()
        .map(|arg| arg.to_ascii_lowercase())
        .collect();
    if SCRIPT_RUNNERS.contains(&stem.as_str())
        && args
            .first()
            .is_some_and(|first| SERVER_SCRIPTS.contains(&first.as_str()))
    {
        return true;
    }
    args.iter().enumerate().any(|(index, arg)| {
        SERVER_ARGS.contains(&arg.as_str())
            || arg.ends_with(".server")
            || arg.starts_with("--watch=")
            || (arg == "run"
                && args
                    .get(index + 1)
                    .is_some_and(|next| SERVER_SCRIPTS.contains(&next.as_str())))
    })
}

fn check_notify(background: bool, notify: bool) -> Result<(), AppError> {
    if notify && !background {
        return Err(invalid("notify", "background"));
    }
    Ok(())
}

static PACKAGE_SPEC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(@[a-z0-9][a-z0-9._~-]*/)?[a-z0-9][a-z0-9._~-]*(@[A-Za-z0-9._~^<>=*+| -]+)?$")
        .expect("a valid pattern")
});

fn check_args(field: &str, args: &[String]) -> Result<(), AppError> {
    if args.len() > PROCESS_ARGV_MAX {
        return Err(invalid(field, "maxItems"));
    }
    for (index, argument) in args.iter().enumerate() {
        check::max_len(&format!("{field}.{index}"), argument, PROCESS_ARG_MAX)?;
    }
    Ok(())
}

fn check_stdin(stdin: Option<&str>) -> Result<(), AppError> {
    stdin.map_or(Ok(()), |stdin| {
        check::max_len("stdin", stdin, STDIN_MAX_CHARS)
    })
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn language_name(language: CodeLanguage) -> &'static str {
    match language {
        CodeLanguage::Python => "Python",
        CodeLanguage::Javascript => "Node.js",
        CodeLanguage::Powershell => "PowerShell",
    }
}

pub fn source_file(language: CodeLanguage) -> &'static str {
    match language {
        CodeLanguage::Python => "main.py",
        CodeLanguage::Javascript => "main.mjs",
        CodeLanguage::Powershell => "main.ps1",
    }
}

pub fn source_text(language: CodeLanguage, source: &str) -> String {
    match language {
        CodeLanguage::Powershell => format!("{UTF8_BOM}{POWERSHELL_PRELUDE}{source}"),
        CodeLanguage::Python | CodeLanguage::Javascript => source.to_owned(),
    }
}

#[derive(Clone)]
pub struct CommandToolOptions {
    pub launcher: Option<Arc<dyn CommandLauncher>>,
    pub scratch: PathBuf,
    pub limits: ProcessLimits,
    pub powershell: String,
    pub roots: VirtualRoots,
    pub shells: Option<BackgroundShells>,
}

impl CommandToolOptions {
    pub fn new(launcher: Option<Arc<dyn CommandLauncher>>, scratch: PathBuf) -> Self {
        Self {
            launcher,
            scratch,
            limits: ProcessLimits::default(),
            powershell: POWERSHELL.to_owned(),
            roots: VirtualRoots::default(),
            shells: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessToolOutput {
    pub text: String,
    pub is_error: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub process: ProcessPlanDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_id: Option<BackgroundShellId>,
}

impl Validate for ProcessToolOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

impl ProcessToolOutput {
    fn of(run: &ProcessRun, text: String, process: ProcessPlanDto) -> Self {
        Self {
            text,
            is_error: run.failed(),
            exit_code: run.exit_code,
            timed_out: run.timed_out,
            truncated: run.truncated,
            duration_ms: run.duration_ms(),
            stdout: run.stdout.clone(),
            stderr: run.stderr.clone(),
            process,
            artifacts: Vec::new(),
            shell_id: None,
        }
    }

    fn background(shell: &BackgroundShellDto, process: ProcessPlanDto, notify: bool) -> Self {
        let next = if notify {
            format!("{NOTIFY_TEXT}\n{BACKGROUND_TEXT}")
        } else {
            BACKGROUND_TEXT.to_owned()
        };
        Self {
            text: format!("Started in the background.\n{}\n{next}", status_line(shell)),
            is_error: false,
            exit_code: None,
            timed_out: false,
            truncated: false,
            duration_ms: 0,
            stdout: String::new(),
            stderr: String::new(),
            process,
            artifacts: Vec::new(),
            shell_id: Some(shell.id),
        }
    }

    fn meta(&self) -> Value {
        let mut meta = json!(ProcessResultDto {
            process: self.process.clone(),
            exit_code: self.exit_code,
            duration_ms: self.duration_ms,
            timed_out: self.timed_out,
            truncated: self.truncated,
            shell_id: self.shell_id,
        });
        if !self.artifacts.is_empty()
            && let Some(fields) = meta.as_object_mut()
        {
            fields.insert("artifacts".to_owned(), json!(self.artifacts));
        }
        meta
    }
}

#[derive(Clone)]
struct Tools {
    folders: FolderAccessService,
    approvals: Arc<ApprovalService>,
    options: CommandToolOptions,
}

impl Tools {
    fn new(
        folders: &FolderAccessService,
        approvals: Arc<ApprovalService>,
        options: CommandToolOptions,
    ) -> Self {
        Self {
            folders: folders.clone(),
            approvals,
            options,
        }
    }

    async fn approve(
        &self,
        context: &StepContext,
        conversation: &str,
        tool: &str,
        action: &str,
        approval: Option<DeferredApproval>,
        process: &ProcessPlanDto,
    ) -> Result<(), AppError> {
        let Some(approval) = approval else {
            return Ok(());
        };
        let reviewed = ReviewedAction::Process {
            tool: tool.to_owned(),
            process: process.clone(),
        };
        let reason = match review_action(context, reviewed).await? {
            ReviewVerdict::Run(authorisation) => return context.authorise(authorisation),
            ReviewVerdict::Deny { rule } => return Err(rule_denied(&rule)),
            ReviewVerdict::Ask { reason } => reason,
        };
        let mut request = self.approvals.create(
            &context.run_id.to_string(),
            &approval.subject,
            &approval.scope,
        )?;
        request.insert("kind".to_owned(), json!(PROCESS_APPROVAL_KIND));
        if !conversation.is_empty() {
            request.insert("conversationId".to_owned(), json!(conversation));
        }
        request.insert("tool".to_owned(), json!(tool));
        request.insert("action".to_owned(), json!(action));
        request.insert("process".to_owned(), json!(process));
        match context.request_approval(request).await? {
            ApprovalDecision::Approved => context.authorise(
                ToolAuthorisationDto::of(ToolAuthorisationKind::Approved).because(reason),
            ),
            ApprovalDecision::Denied => {
                Err(AppError::new(AppErrorCode::ApprovalDenied, PROCESS_DENIED))
            }
        }
    }

    fn working_folder(&self, conversation: &str) -> Result<Option<PathBuf>, AppError> {
        self.folders.working_folder_of(conversation)
    }

    fn shells(
        &self,
        background: bool,
        conversation: &str,
    ) -> Result<Option<BackgroundShells>, AppError> {
        if !background {
            return Ok(None);
        }
        if conversation.is_empty() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NO_OWNER));
        }
        self.options
            .shells
            .clone()
            .map(Some)
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, BACKGROUND_UNAVAILABLE))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodeRunInput {
    #[schemars(description = "Program language: python, javascript, or powershell")]
    pub language: CodeLanguage,
    #[schemars(
        description = "Полный текст программы; для python зависимости указываются в заголовке PEP 723"
    )]
    pub source: String,
    #[serde(default)]
    #[schemars(description = "Command-line arguments of the program")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Text passed to the program on standard input")]
    pub stdin: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Только для javascript: пакеты npm, например lodash или lodash@4.17.21"
    )]
    pub packages: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(description = "Keep running after the call returns; read with shell.output")]
    pub background: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(description = "With background: continue the conversation when the program ends")]
    pub notify: bool,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
}

impl Validate for CodeRunInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("source", &self.source)?;
        if self.background && self.stdin.is_some() {
            return Err(invalid("stdin", "background"));
        }
        check_notify(self.background, self.notify)?;
        check::max_len("source", &self.source, PROCESS_SOURCE_MAX)?;
        check_args("args", &self.args)?;
        check_stdin(self.stdin.as_deref())?;
        if self.packages.len() > PROCESS_PACKAGES_MAX {
            return Err(invalid("packages", "maxItems"));
        }
        if !self.packages.is_empty() && self.language != CodeLanguage::Javascript {
            return Err(invalid("packages", "language"));
        }
        for (index, package) in self.packages.iter().enumerate() {
            let field = format!("packages.{index}");
            check::max_len(&field, package, PROCESS_PACKAGE_MAX)?;
            if !PACKAGE_SPEC.is_match(package) {
                return Err(invalid(&field, "pattern"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePlan {
    pub install: Option<LaunchSpec>,
    pub run: LaunchSpec,
}

fn launched(
    launcher: Option<&dyn CommandLauncher>,
    command: &str,
    args: &[String],
    language: CodeLanguage,
) -> Result<LaunchSpec, AppError> {
    let launcher =
        launcher.ok_or_else(|| AppError::new(AppErrorCode::NotFound, CODE_NO_TOOLCHAIN))?;
    launcher.launch_command(command, args).unwrap_or_else(|| {
        Err(AppError::new(
            AppErrorCode::NotFound,
            format!(
                "Для запуска нужен {}. {SETTINGS_HINT}",
                language_name(language)
            ),
        )
        .with_detail("runtime", command))
    })
}

pub fn code_plan(
    input: &CodeRunInput,
    folder: &Path,
    workspace: Option<&Path>,
    options: &CommandToolOptions,
) -> Result<CodePlan, AppError> {
    let launcher = options.launcher.as_deref();
    let file = path_text(&folder.join(source_file(input.language)));
    let mut install = None;
    let mut run = match input.language {
        CodeLanguage::Python => {
            let mut args = vec!["run".to_owned(), file];
            args.extend(input.args.iter().cloned());
            let mut spec = launched(launcher, "uv", &args, input.language)?;
            for (name, value) in PYTHON_UTF8_ENV {
                spec.env
                    .entry((*name).to_owned())
                    .or_insert_with(|| (*value).to_owned());
            }
            spec
        }
        CodeLanguage::Javascript => {
            let mut args = vec![file];
            args.extend(input.args.iter().cloned());
            let spec = launched(launcher, "node", &args, input.language)?;
            if !input.packages.is_empty() {
                let mut args: Vec<String> = NPM_INSTALL.map(str::to_owned).to_vec();
                args.extend(input.packages.iter().cloned());
                install = Some(launched(launcher, "npm", &args, input.language)?);
            }
            spec
        }
        CodeLanguage::Powershell => {
            let mut args: Vec<String> = POWERSHELL_FLAGS.map(str::to_owned).to_vec();
            args.push(file);
            args.extend(input.args.iter().cloned());
            LaunchSpec {
                command: options.powershell.clone(),
                args,
                env: std::collections::BTreeMap::new(),
                cwd: None,
            }
        }
    };
    for spec in std::iter::once(&mut run).chain(install.as_mut()) {
        spec.cwd = Some(folder.to_path_buf());
        spec.env
            .insert(SCRATCH_DIR_ENV.to_owned(), path_text(folder));
        if let Some(workspace) = workspace {
            spec.env
                .insert(WORKSPACE_DIR_ENV.to_owned(), path_text(workspace));
        }
    }
    Ok(CodePlan { install, run })
}

fn write_sources(folder: &Path, input: &CodeRunInput) -> Result<(), AppError> {
    let failed = |_: std::io::Error| AppError::new(AppErrorCode::NativeError, SCRATCH_FAILED);
    std::fs::create_dir_all(folder).map_err(failed)?;
    if !input.packages.is_empty() {
        std::fs::write(folder.join("package.json"), PACKAGE_MANIFEST).map_err(failed)?;
    }
    std::fs::write(
        folder.join(source_file(input.language)),
        source_text(input.language, &input.source),
    )
    .map_err(failed)
}

pub struct CodeRunNode(Tools);

impl CodeRunNode {
    pub fn new(
        folders: &FolderAccessService,
        approvals: Arc<ApprovalService>,
        options: CommandToolOptions,
    ) -> Self {
        Self(Tools::new(folders, approvals, options))
    }
}

#[async_trait]
impl NodeDef for CodeRunNode {
    type Input = CodeRunInput;
    type Output = ProcessToolOutput;

    fn node_type(&self) -> &str {
        CODE_RUN
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(CODE_RUN)
    }

    fn permission_for(&self, _input: &Value) -> PermissionRequirement {
        PermissionRequirement::tool_at(CODE_RUN, ToolTier::Ask)
    }

    fn defers_approval(&self) -> bool {
        true
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: CodeRunInput,
    ) -> Result<ProcessToolOutput, AppError> {
        let approval = context.take_deferred_approval();
        context.check_cancelled()?;
        let tools = &self.0;
        let shells = tools.shells(input.background, &input.conversation_id)?;
        let workspace = tools.working_folder(&input.conversation_id)?;
        let run_id = context.run_id.to_string();
        let folder = tools
            .options
            .scratch
            .join(&run_id)
            .join(new_id().simple().to_string());
        let plan = code_plan(&input, &folder, workspace.as_deref(), &tools.options)?;
        let process = ProcessPlanDto {
            program: plan.run.command.clone(),
            language: Some(input.language),
            argv: input.args.clone(),
            cwd: path_text(&folder),
            source: Some(input.source.clone()),
            packages: input.packages.clone(),
            background: input.background,
        };
        tools
            .approve(
                context,
                &input.conversation_id,
                CODE_RUN,
                CODE_ACTION,
                approval,
                &process,
            )
            .await?;
        context.check_cancelled()?;
        prepare_scratch(&tools.options.scratch, &run_id)?;
        write_sources(&folder, &input)?;
        let limits = tools.options.limits;
        if let Some(install) = &plan.install {
            let installed =
                run_process(install, None, limits, &context.cancel, CODE_NOT_STARTED).await?;
            if installed.failed() {
                let text = format!(
                    "npm install failed; the program did not run.\n{}",
                    process_text(&installed, "package installation", limits)
                );
                return Ok(ProcessToolOutput::of(&installed, text, process));
            }
        }
        if let Some(shells) = shells {
            let pin = pin_scratch(&tools.options.scratch.join(&run_id));
            let mut argv = vec![
                input.language.as_str().to_owned(),
                source_file(input.language).to_owned(),
            ];
            argv.extend(input.args.iter().cloned());
            let launch = ShellStart::notify(input.notify).finishing(Box::new(move || {
                drop(pin);
                Vec::new()
            }));
            let shell = shells.start(
                &input.conversation_id,
                &plan.run,
                &argv,
                CODE_NOT_STARTED,
                launch,
            )?;
            return Ok(ProcessToolOutput::background(&shell, process, input.notify));
        }
        let run = run_process(
            &plan.run,
            input.stdin.as_deref(),
            limits,
            &context.cancel,
            CODE_NOT_STARTED,
        )
        .await?;
        let text = process_text(&run, "program", limits);
        Ok(ProcessToolOutput::of(&run, text, process))
    }

    fn tool_meta(&self, _input: &Value, output: &ProcessToolOutput) -> Option<Value> {
        Some(output.meta())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellRunInput {
    #[schemars(
        description = "Программа и аргументы по отдельности, например [\"npm\", \"test\"] или [\"skill:dashboard/scripts/build_dashboard.py\", \"_spec.json\"]"
    )]
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Рабочая папка процесса: абсолютный путь, путь относительно рабочей папки диалога или chat:"
    )]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Text passed to the process on standard input")]
    pub stdin: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(
        description = "Keep running after the call returns; read with shell.output, stop with shell.kill"
    )]
    pub background: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(
        description = "With background: continue the conversation when the process ends; not for servers or watchers"
    )]
    pub notify: bool,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
}

impl Validate for ShellRunInput {
    fn validate(&self) -> Result<(), AppError> {
        let Some(program) = self.argv.first() else {
            return Err(invalid("argv", "minItems"));
        };
        check::non_empty("argv.0", program.trim())?;
        check_args("argv", &self.argv)?;
        if let Some(cwd) = &self.cwd {
            check::non_empty("cwd", cwd)?;
            check::max_len("cwd", cwd, PATH_MAX_CHARS)?;
        }
        if self.background && self.stdin.is_some() {
            return Err(invalid("stdin", "background"));
        }
        check_notify(self.background, self.notify)?;
        if self.notify && server_like(&self.argv) {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NOTIFY_SERVER)
                .with_detail("field", "notify"));
        }
        check_stdin(self.stdin.as_deref())
    }
}

pub fn shell_spec(argv: &[String], cwd: &Path, dirs: &[PathBuf]) -> Result<LaunchSpec, AppError> {
    let name = argv.first().map(String::as_str).unwrap_or_default();
    let pathext = std::env::var("PATHEXT").ok();
    let program = find_program(name, dirs, cwd, pathext.as_deref(), &|path| path.is_file())
        .ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, PROGRAM_NOT_FOUND).with_detail("program", name)
        })?;
    Ok(LaunchSpec {
        command: path_text(&program),
        args: argv[1..].to_vec(),
        env: std::collections::BTreeMap::from([("PATH".to_owned(), joined_path(dirs))]),
        cwd: Some(cwd.to_path_buf()),
    })
}

pub struct SkillScript {
    pub spec: LaunchSpec,
    pub dir: PathBuf,
}

pub fn skill_script(
    options: &CommandToolOptions,
    key: &str,
    path: &str,
    args: &[String],
) -> Result<SkillScript, AppError> {
    let library = options
        .roots
        .skills()
        .ok_or_else(|| AppError::new(AppErrorCode::Conflict, ROOTS_UNAVAILABLE))?;
    let script = library.script(key, path)?;
    let runtime = ScriptRuntime::of(&script.path)
        .ok_or_else(|| AppError::new(AppErrorCode::UnsupportedFormat, SCRIPT_UNSUPPORTED))?;
    let launcher = options
        .launcher
        .as_ref()
        .ok_or_else(|| AppError::new(AppErrorCode::NotFound, SCRIPT_NO_TOOLCHAIN))?;
    let mut spec = launcher
        .launch_command(
            runtime.command(),
            &runtime.args(&path_text(&script.file), args),
        )
        .unwrap_or_else(|| {
            Err(AppError::new(
                AppErrorCode::NotFound,
                format!(
                    "Для запуска скрипта нужен {}. {SETTINGS_HINT}",
                    runtime.display_name()
                ),
            )
            .with_detail("runtime", runtime.command()))
        })?;
    spec.env
        .insert(SKILL_DIR_ENV.to_owned(), path_text(&script.dir));
    if runtime == ScriptRuntime::Python {
        for (name, value) in PYTHON_UTF8_ENV {
            spec.env
                .entry((*name).to_owned())
                .or_insert_with(|| (*value).to_owned());
        }
    }
    Ok(SkillScript {
        spec,
        dir: script.dir,
    })
}

struct ScriptOutputs {
    store: ArtifactStore,
    owner: String,
    run: String,
    mentioned: Vec<PathBuf>,
    cwd: PathBuf,
    script_dir: PathBuf,
    before: Snapshot,
    libraries: Vec<(String, PathBuf, Snapshot)>,
    started: std::time::SystemTime,
}

impl ScriptOutputs {
    fn watch(
        store: Option<&ArtifactStore>,
        spec: &mut LaunchSpec,
        context: &StepContext,
        input: &ShellRunInput,
        cwd: &Path,
        script_dir: &Path,
    ) -> Option<Self> {
        let store = store?;
        let libraries = store
            .libraries()
            .iter()
            .map(|(name, folder)| {
                let _ = std::fs::create_dir_all(folder);
                spec.env.insert(library_env(name), path_text(folder));
                (name.clone(), folder.clone(), snapshot(folder))
            })
            .collect();
        let run = context.run_id.to_string();
        let owner = if is_owner(&input.conversation_id) {
            input.conversation_id.clone()
        } else {
            run.clone()
        };
        Some(Self {
            store: store.clone(),
            owner,
            run,
            mentioned: mentioned_paths(&input.argv[1..], cwd),
            cwd: cwd.to_path_buf(),
            script_dir: script_dir.to_path_buf(),
            before: snapshot(cwd),
            libraries,
            started: std::time::SystemTime::now(),
        })
    }

    fn collect(self) -> Vec<ArtifactDto> {
        let results: Vec<PathBuf> =
            produced(&self.cwd, &self.before, &self.mentioned, self.started)
                .into_iter()
                .filter(|path| {
                    !path.starts_with(&self.script_dir)
                        && path
                            .strip_prefix(&self.cwd)
                            .map_or(true, |relative| !is_working(relative))
                })
                .collect();
        let mut kept = self.store.keep(&self.owner, &self.run, &results);
        for (name, folder, previous) in &self.libraries {
            kept.extend(
                self.store
                    .adopt(name, &produced(folder, previous, &[], self.started)),
            );
        }
        kept
    }
}

pub struct ShellRunNode(Tools);

impl ShellRunNode {
    pub fn new(
        folders: &FolderAccessService,
        approvals: Arc<ApprovalService>,
        options: CommandToolOptions,
    ) -> Self {
        Self(Tools::new(folders, approvals, options))
    }

    fn chat_folder(&self, conversation: &str) -> Option<PathBuf> {
        let roots = &self.0.options.roots;
        roots.chats()?;
        roots.chat_folder(conversation).ok()
    }

    async fn cwd(
        &self,
        context: &StepContext,
        input: &ShellRunInput,
        script: bool,
    ) -> Result<PathBuf, AppError> {
        let tools = &self.0;
        let located = input
            .cwd
            .as_deref()
            .map(|cwd| tools.options.roots.locate(&input.conversation_id, cwd))
            .transpose()?
            .flatten();
        if let Some(located) = located {
            if located.kind == RootKind::Skill {
                return Err(AppError::new(
                    AppErrorCode::PermissionDenied,
                    SKILL_READ_ONLY,
                ));
            }
            return if located.path.is_dir() {
                Ok(located.path)
            } else {
                Err(AppError::new(AppErrorCode::NotFound, FOLDER_MISSING)
                    .with_detail("path", path_text(&located.path)))
            };
        }
        if script && input.cwd.is_none() {
            return match self.chat_folder(&input.conversation_id) {
                Some(folder) => Ok(folder),
                None => prepare_scratch(&tools.options.scratch, &context.run_id.to_string()),
            };
        }
        let folder = match &input.cwd {
            Some(cwd) => {
                let call = tools.folders.call(&input.conversation_id, SHELL_RUN);
                tools
                    .folders
                    .ensure_access(context, &call, SHELL_ACTION, &[FolderTarget::folder(cwd)])
                    .await?
                    .into_iter()
                    .next()
                    .map(|target| target.path)
                    .ok_or_else(|| AppError::new(AppErrorCode::NotFound, FOLDER_MISSING))?
            }
            None => tools
                .working_folder(&input.conversation_id)?
                .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NO_FOLDER))?,
        };
        if folder.is_dir() {
            Ok(folder)
        } else {
            Err(AppError::new(AppErrorCode::NotFound, FOLDER_MISSING)
                .with_detail("path", path_text(&folder)))
        }
    }
}

#[async_trait]
impl NodeDef for ShellRunNode {
    type Input = ShellRunInput;
    type Output = ProcessToolOutput;

    fn node_type(&self) -> &str {
        SHELL_RUN
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SHELL_RUN)
    }

    fn permission_for(&self, _input: &Value) -> PermissionRequirement {
        PermissionRequirement::tool_at(SHELL_RUN, ToolTier::Ask)
    }

    fn defers_approval(&self) -> bool {
        true
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: ShellRunInput,
    ) -> Result<ProcessToolOutput, AppError> {
        let approval = context.take_deferred_approval();
        context.check_cancelled()?;
        let tools = &self.0;
        let program = input.argv.first().map(String::as_str).unwrap_or_default();
        let skill = match path_root(program) {
            PathRoot::Skill { key, rest } => {
                Some(skill_script(&tools.options, key, rest, &input.argv[1..])?)
            }
            _ => None,
        };
        let shells = tools.shells(input.background, &input.conversation_id)?;
        let cwd = self.cwd(context, &input, skill.is_some()).await?;
        let chat = skill
            .as_ref()
            .and_then(|_| self.chat_folder(&input.conversation_id));
        let spec = match &skill {
            Some(script) => {
                let mut spec = script.spec.clone();
                spec.cwd = Some(cwd.clone());
                spec.env.insert(SCRATCH_DIR_ENV.to_owned(), path_text(&cwd));
                if let Some(folder) = &chat {
                    spec.env.insert(CHAT_DIR_ENV.to_owned(), path_text(folder));
                }
                spec
            }
            None => {
                let dirs = tools.options.launcher.as_ref().map_or_else(
                    || path_dirs(std::env::var_os("PATH").as_ref()),
                    |launcher| launcher.program_dirs(),
                );
                shell_spec(&input.argv, &cwd, &dirs)?
            }
        };
        let process = ProcessPlanDto {
            program: spec.command.clone(),
            language: None,
            argv: input.argv.clone(),
            cwd: path_text(&cwd),
            source: None,
            packages: Vec::new(),
            background: input.background,
        };
        tools
            .approve(
                context,
                &input.conversation_id,
                SHELL_RUN,
                SHELL_ACTION,
                approval,
                &process,
            )
            .await?;
        context.check_cancelled()?;
        let limits = tools.options.limits;
        if let Some(shells) = shells {
            let mut spec = spec;
            let mut launch = ShellStart::notify(input.notify);
            let mut not_started = SHELL_NOT_STARTED;
            if let Some(script) = &skill {
                not_started = SCRIPT_NOT_STARTED;
                let pin = cwd
                    .starts_with(&tools.options.scratch)
                    .then(|| pin_scratch(&cwd));
                let outputs = ScriptOutputs::watch(
                    tools.options.roots.chats(),
                    &mut spec,
                    context,
                    &input,
                    &cwd,
                    &script.dir,
                );
                launch = launch.finishing(Box::new(move || {
                    let kept = outputs.map(ScriptOutputs::collect).unwrap_or_default();
                    drop(pin);
                    kept
                }));
            }
            let shell = shells.start(
                &input.conversation_id,
                &spec,
                &input.argv,
                not_started,
                launch,
            )?;
            return Ok(ProcessToolOutput::background(&shell, process, input.notify));
        }
        let Some(script) = skill else {
            let run = run_process(
                &spec,
                input.stdin.as_deref(),
                limits,
                &context.cancel,
                SHELL_NOT_STARTED,
            )
            .await?;
            let text = process_text(&run, "process", limits);
            return Ok(ProcessToolOutput::of(&run, text, process));
        };
        let mut spec = spec;
        let outputs = ScriptOutputs::watch(
            tools.options.roots.chats(),
            &mut spec,
            context,
            &input,
            &cwd,
            &script.dir,
        );
        let run = run_process(
            &spec,
            input.stdin.as_deref(),
            limits,
            &context.cancel,
            SCRIPT_NOT_STARTED,
        )
        .await?;
        let artifacts = outputs.map(ScriptOutputs::collect).unwrap_or_default();
        let mut text = process_text(&run, "script", limits);
        if !artifacts.is_empty() {
            let names: Vec<&str> = artifacts
                .iter()
                .map(|artifact| artifact.name.as_str())
                .collect();
            text.push_str(&format!(
                "\n\nFiles shown to the user with download buttons: {}",
                names.join(", ")
            ));
        }
        Ok(ProcessToolOutput {
            artifacts,
            ..ProcessToolOutput::of(&run, text, process)
        })
    }

    fn tool_meta(&self, _input: &Value, output: &ProcessToolOutput) -> Option<Value> {
        Some(output.meta())
    }
}

pub fn register_command_nodes(
    nodes: &NodeRegistry,
    folders: &FolderAccessService,
    approvals: Arc<ApprovalService>,
    options: CommandToolOptions,
) -> Result<(), AppError> {
    let tools = Tools::new(folders, approvals, options);
    nodes.register(CodeRunNode(tools.clone()))?;
    nodes.register(ShellRunNode(tools))
}
