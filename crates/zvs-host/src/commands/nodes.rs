use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::{check, invalid};
use zvs_contract::{
    AppError, AppErrorCode, ArtifactDto, CodeLanguage, PROCESS_APPROVAL_KIND, PROCESS_ARG_MAX,
    PROCESS_ARGV_MAX, PROCESS_PACKAGE_MAX, PROCESS_PACKAGES_MAX, PROCESS_SOURCE_MAX,
    ProcessPlanDto, ProcessResultDto, ToolTier, Validate,
};

use crate::artifacts::{Snapshot, library_env, mentioned_paths, produced, snapshot};
use crate::chat_files::{CHAT_DIR_ENV, is_working};
use crate::conversations::is_owner;
use crate::files::roots::{
    PathRoot, ROOTS_UNAVAILABLE, RootKind, SKILL_READ_ONLY, VirtualRoots, path_root,
};
use crate::kernel::{
    ApprovalDecision, DeferredApproval, NodeDef, NodeRegistry, PermissionRequirement, StepContext,
};
use crate::platform::ids::new_id;
use crate::services::approvals::ApprovalService;
use crate::skills::script::{
    SCRIPT_NO_TOOLCHAIN, SCRIPT_NOT_STARTED, SCRIPT_UNSUPPORTED, SKILL_DIR_ENV, ScriptRuntime,
};
use crate::toolchain::CommandLauncher;
use crate::toolchain::LaunchSpec;
use crate::toolchain::process::{
    ProcessLimits, ProcessRun, SCRATCH_FAILED, find_program, joined_path, path_dirs,
    prepare_scratch, process_text, run_process,
};
use crate::toolchain::service::SETTINGS_HINT;
use crate::workspace::{FolderAccessService, FolderTarget};

pub const CODE_RUN: &str = "code.run";
pub const SHELL_RUN: &str = "shell.run";
pub const PROCESS_TOOLS: [&str; 2] = [CODE_RUN, SHELL_RUN];

pub const CODE_DESCRIPTION: &str = "Запуск короткой программы, написанной моделью. language: python — выполняется через uv run; зависимости объявляются в начале source заголовком PEP 723 (# /// script, dependencies = [\"…\"], # ///), uv устанавливает их в собственный кэш приложения, глобальная установка не выполняется. javascript — выполняется через Node.js как ES-модуль (import, не require); пакеты из packages устанавливаются во временную папку через npm install с отключёнными скриптами установки. powershell — выполняется через powershell.exe без профиля. Рабочая папка процесса — временная папка запуска; рабочая папка диалога, если она выбрана, передаётся в переменной окружения ZVS_WORKSPACE_DIR. Разрешения на папки этот инструмент не ограничивают: процесс работает с правами пользователя, и пользователь подтверждает каждый запуск по полному тексту программы. Время выполнения ограничено, интерактивный ввод не поддерживается. Возвращает код завершения, stdout и stderr.";
pub const SHELL_DESCRIPTION: &str = "Запуск программы проекта, например npm test или cargo check, или скрипта навыка. argv — массив: argv[0] — программа, остальные элементы — аргументы. Строка командной оболочки не разбирается, поэтому конвейеры, перенаправления и подстановки недоступны. Программа ищется сначала в папках подключённых сред, затем в PATH. Скрипт навыка указывается как argv[0] = skill:<ключ>/scripts/<файл>: .py выполняется через uv, .js, .mjs и .cjs — через Node.js, папка навыка передаётся в ZVS_SKILL_DIR. Рабочая папка процесса — cwd (путь или chat:); без cwd — рабочая папка диалога, а для скрипта навыка — chat:. Доступ к cwd вне выданных разрешений подтверждает пользователь, но сам процесс работает с правами пользователя. Пользователь подтверждает каждый запуск по полной команде. Время выполнения ограничено; серверы, фоновые и интерактивные процессы не поддерживаются, стандартный ввод передаётся только из stdin. Возвращает код завершения, stdout и stderr.";

pub const CODE_ACTION: &str = "запуск кода";
pub const SHELL_ACTION: &str = "запуск команды";

pub const CODE_NOT_STARTED: &str = "Не удалось запустить программу";
pub const SHELL_NOT_STARTED: &str = "Не удалось запустить команду";
pub const CODE_NO_TOOLCHAIN: &str = "Среда выполнения программ недоступна";
pub const NO_FOLDER: &str = "Команде нужна папка: выберите рабочую папку диалога или укажите cwd";
pub const FOLDER_MISSING: &str = "Папка не найдена";
pub const PROGRAM_NOT_FOUND: &str = "Программа не найдена в папках сред и в PATH";
pub const PROCESS_DENIED: &str = "Запуск отклонён";

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
}

impl CommandToolOptions {
    pub fn new(launcher: Option<Arc<dyn CommandLauncher>>, scratch: PathBuf) -> Self {
        Self {
            launcher,
            scratch,
            limits: ProcessLimits::default(),
            powershell: POWERSHELL.to_owned(),
            roots: VirtualRoots::default(),
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
        }
    }

    fn meta(&self) -> Value {
        let mut meta = json!(ProcessResultDto {
            process: self.process.clone(),
            exit_code: self.exit_code,
            duration_ms: self.duration_ms,
            timed_out: self.timed_out,
            truncated: self.truncated,
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
            ApprovalDecision::Approved => Ok(()),
            ApprovalDecision::Denied => {
                Err(AppError::new(AppErrorCode::ApprovalDenied, PROCESS_DENIED))
            }
        }
    }

    fn working_folder(&self, conversation: &str) -> Result<Option<PathBuf>, AppError> {
        self.folders.working_folder_of(conversation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodeRunInput {
    #[schemars(description = "Язык программы: python, javascript или powershell")]
    pub language: CodeLanguage,
    #[schemars(
        description = "Полный текст программы; для python зависимости указываются в заголовке PEP 723"
    )]
    pub source: String,
    #[serde(default)]
    #[schemars(description = "Аргументы командной строки программы")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Текст, передаваемый программе на стандартный ввод")]
    pub stdin: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Только для javascript: пакеты npm, например lodash или lodash@4.17.21"
    )]
    pub packages: Vec<String>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
}

impl Validate for CodeRunInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("source", &self.source)?;
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
    #[schemars(description = "Текст, передаваемый процессу на стандартный ввод")]
    pub stdin: Option<String>,
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
        let store = tools.options.roots.chats();
        let libraries: Vec<(String, PathBuf, Snapshot)> = store.map_or_else(Vec::new, |store| {
            store
                .libraries()
                .iter()
                .map(|(name, folder)| {
                    let _ = std::fs::create_dir_all(folder);
                    spec.env.insert(library_env(name), path_text(folder));
                    (name.clone(), folder.clone(), snapshot(folder))
                })
                .collect()
        });
        let before = snapshot(&cwd);
        let started = std::time::SystemTime::now();
        let run = run_process(
            &spec,
            input.stdin.as_deref(),
            limits,
            &context.cancel,
            SCRIPT_NOT_STARTED,
        )
        .await?;
        let artifacts = store.map_or_else(Vec::new, |store| {
            let mentioned = mentioned_paths(&input.argv[1..], &cwd);
            let results: Vec<PathBuf> = produced(&cwd, &before, &mentioned, started)
                .into_iter()
                .filter(|path| {
                    !path.starts_with(&script.dir)
                        && path
                            .strip_prefix(&cwd)
                            .map_or(true, |relative| !is_working(relative))
                })
                .collect();
            let run = context.run_id.to_string();
            let owner = if is_owner(&input.conversation_id) {
                input.conversation_id.as_str()
            } else {
                run.as_str()
            };
            let mut kept = store.keep(owner, &run, &results);
            for (name, folder, previous) in &libraries {
                kept.extend(store.adopt(name, &produced(folder, previous, &[], started)));
            }
            kept
        });
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
