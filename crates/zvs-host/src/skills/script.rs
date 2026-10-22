use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use zvs_contract::validate::{check, invalid};
use zvs_contract::{AppError, AppErrorCode, Validate};

use super::library::SkillLibrary;
use crate::drivers::mcp::{default_spawn, kill_tree};
use crate::kernel::types::cancelled;
use crate::kernel::{NodeDef, PermissionRequirement, StepContext};
use crate::toolchain::CommandLauncher;
use crate::toolchain::service::SETTINGS_HINT;

pub const SKILL_SCRIPT: &str = "skill.script";
pub const SCRIPT_DESCRIPTION: &str = "Запуск скрипта навыка из папки scripts/ по идентификатору навыка и пути из результата skill.load: файлы .py выполняются через uv, файлы .js, .mjs и .cjs — через Node.js. Рабочая папка — временная папка запуска; папка навыка передаётся в переменной окружения ZVS_SKILL_DIR и предназначена только для чтения. Возвращает код завершения, stdout и stderr.";

pub const SCRIPT_UNSUPPORTED: &str =
    "Скрипт навыка должен иметь расширение .py, .js, .mjs или .cjs";
pub const SCRIPT_NO_TOOLCHAIN: &str = "Среда выполнения скриптов недоступна";
pub const SCRIPT_NOT_STARTED: &str = "Не удалось запустить скрипт навыка";
pub const SCRIPT_SCRATCH_FAILED: &str = "Не удалось подготовить рабочую папку скрипта";

pub const SKILL_DIR_ENV: &str = "ZVS_SKILL_DIR";
pub const SCRATCH_DIR_ENV: &str = "ZVS_SCRATCH_DIR";
pub const PYTHON_UTF8_ENV: &[(&str, &str)] = &[("PYTHONUTF8", "1"), ("PYTHONIOENCODING", "utf-8")];
pub const SCRIPT_TIMEOUT: Duration = Duration::from_secs(120);
pub const SCRIPT_OUTPUT_BYTES: usize = 64 * 1024;
pub const SCRATCH_KEEP: usize = 16;
const ARGS_MAX: usize = 64;
const ARG_MAX_CHARS: usize = 4096;
const STDIN_MAX_CHARS: usize = 1_048_576;
const DRAIN_GRACE: Duration = Duration::from_secs(2);

static STREAM_FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(/?)(stdout|stderr)").expect("a valid pattern"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptRuntime {
    Python,
    Node,
}

impl ScriptRuntime {
    pub fn of(path: &str) -> Option<Self> {
        let extension = Path::new(path)
            .extension()?
            .to_string_lossy()
            .to_ascii_lowercase();
        match extension.as_str() {
            "py" => Some(Self::Python),
            "js" | "mjs" | "cjs" => Some(Self::Node),
            _ => None,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Python => "uv",
            Self::Node => "node",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Python => "uv",
            Self::Node => "Node.js",
        }
    }

    pub fn args(self, script: &str, extra: &[String]) -> Vec<String> {
        let mut args = match self {
            Self::Python => vec!["run".to_owned(), script.to_owned()],
            Self::Node => vec![script.to_owned()],
        };
        args.extend(extra.iter().cloned());
        args
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillScriptInput {
    #[schemars(description = "Идентификатор навыка")]
    pub key: String,
    #[schemars(description = "Путь скрипта из результата skill.load, например scripts/extract.py")]
    pub path: String,
    #[serde(default)]
    #[schemars(description = "Аргументы командной строки скрипта")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Текст, передаваемый скрипту на стандартный ввод")]
    pub stdin: Option<String>,
}

impl Validate for SkillScriptInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("key", &self.key)?;
        check::max_len("key", &self.key, 128)?;
        check::non_empty("path", &self.path)?;
        check::max_len("path", &self.path, 512)?;
        if self.args.len() > ARGS_MAX {
            return Err(invalid("args", "too_many"));
        }
        for (index, argument) in self.args.iter().enumerate() {
            check::max_len(&format!("args.{index}"), argument, ARG_MAX_CHARS)?;
        }
        if let Some(stdin) = &self.stdin {
            check::max_len("stdin", stdin, STDIN_MAX_CHARS)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillScriptOutput {
    pub key: String,
    pub path: String,
    pub runtime: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
    pub stdout: String,
    pub stderr: String,
    pub text: String,
    pub is_error: bool,
}

impl Validate for SkillScriptOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct SkillScriptOptions {
    pub launcher: Option<Arc<dyn CommandLauncher>>,
    pub scratch: PathBuf,
    pub timeout: Duration,
    pub output_bytes: usize,
}

impl SkillScriptOptions {
    pub fn new(launcher: Option<Arc<dyn CommandLauncher>>, scratch: PathBuf) -> Self {
        Self {
            launcher,
            scratch,
            timeout: SCRIPT_TIMEOUT,
            output_bytes: SCRIPT_OUTPUT_BYTES,
        }
    }
}

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    truncated: bool,
}

type SharedCapture = Arc<Mutex<Capture>>;

async fn drain(mut stream: impl AsyncRead + Unpin, capture: SharedCapture, limit: usize) {
    let mut buffer = [0_u8; 8192];
    loop {
        let read = match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        let mut capture = capture.lock().unwrap_or_else(PoisonError::into_inner);
        let room = limit.saturating_sub(capture.bytes.len());
        if read > room {
            capture.truncated = true;
        }
        capture.bytes.extend_from_slice(&buffer[..read.min(room)]);
    }
}

fn taken(capture: &SharedCapture) -> (String, bool) {
    let capture = capture.lock().unwrap_or_else(PoisonError::into_inner);
    (
        String::from_utf8_lossy(&capture.bytes).into_owned(),
        capture.truncated,
    )
}

fn fenced(tag: &str, text: &str) -> String {
    let body = STREAM_FENCE.replace_all(text, "<$1 $2");
    format!("<{tag}>\n{body}\n</{tag}>")
}

pub fn script_text(output: &SkillScriptOutput, limit: usize, timeout: Duration) -> String {
    let code = output
        .exit_code
        .map_or_else(|| "none".to_owned(), |code| code.to_string());
    let mut lines = vec![format!("exit code: {code}")];
    if output.timed_out {
        lines.push(format!(
            "[the script was stopped after {} seconds]",
            timeout.as_secs()
        ));
    }
    if output.truncated {
        lines.push(format!("[output truncated to {limit} bytes per stream]"));
    }
    lines.push(fenced("stdout", &output.stdout));
    lines.push(fenced("stderr", &output.stderr));
    lines.join("\n")
}

pub fn prepare_scratch(root: &Path, run: &str) -> Result<PathBuf, AppError> {
    let failed =
        |_: std::io::Error| AppError::new(AppErrorCode::NativeError, SCRIPT_SCRATCH_FAILED);
    let scratch = root.join(run);
    fs::create_dir_all(&scratch).map_err(failed)?;
    let mut others: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(root)
        .map_err(failed)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && *path != scratch)
        .filter_map(|path| {
            let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((modified, path))
        })
        .collect();
    others.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, stale) in others.into_iter().skip(SCRATCH_KEEP.saturating_sub(1)) {
        let _ = fs::remove_dir_all(stale);
    }
    Ok(scratch)
}

enum Ending {
    Exited(Option<i32>),
    TimedOut,
    Cancelled,
}

pub struct SkillScriptNode {
    library: SkillLibrary,
    options: SkillScriptOptions,
}

impl SkillScriptNode {
    pub fn new(library: SkillLibrary, options: SkillScriptOptions) -> Self {
        Self { library, options }
    }
}

#[async_trait]
impl NodeDef for SkillScriptNode {
    type Input = SkillScriptInput;
    type Output = SkillScriptOutput;

    fn node_type(&self) -> &str {
        SKILL_SCRIPT
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SKILL_SCRIPT)
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: SkillScriptInput,
    ) -> Result<SkillScriptOutput, AppError> {
        context.check_cancelled()?;
        let script = self.library.script(&input.key, &input.path)?;
        let runtime = ScriptRuntime::of(&script.path)
            .ok_or_else(|| AppError::new(AppErrorCode::UnsupportedFormat, SCRIPT_UNSUPPORTED))?;
        let missing = || {
            AppError::new(
                AppErrorCode::NotFound,
                format!(
                    "Для запуска скрипта нужен {}. {SETTINGS_HINT}",
                    runtime.display_name()
                ),
            )
            .with_detail("runtime", runtime.command())
        };
        let launcher = self
            .options
            .launcher
            .as_ref()
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, SCRIPT_NO_TOOLCHAIN))?;
        let args = runtime.args(&script.file.to_string_lossy(), &input.args);
        let mut spec = launcher
            .launch_command(runtime.command(), &args)
            .unwrap_or_else(|| Err(missing()))?;
        let scratch = prepare_scratch(&self.options.scratch, &context.run_id.to_string())?;
        spec.env.insert(
            SKILL_DIR_ENV.to_owned(),
            script.dir.to_string_lossy().into_owned(),
        );
        spec.env.insert(
            SCRATCH_DIR_ENV.to_owned(),
            scratch.to_string_lossy().into_owned(),
        );
        if runtime == ScriptRuntime::Python {
            for (name, value) in PYTHON_UTF8_ENV {
                spec.env
                    .entry((*name).to_owned())
                    .or_insert_with(|| (*value).to_owned());
            }
        }
        spec.cwd = Some(scratch);
        context.check_cancelled()?;
        let mut child = default_spawn(&spec).map_err(|error| {
            AppError::new(AppErrorCode::NativeError, SCRIPT_NOT_STARTED)
                .with_detail("reason", error.to_string())
        })?;
        let limit = self.options.output_bytes;
        let stdout = SharedCapture::default();
        let stderr = SharedCapture::default();
        let mut readers = Vec::new();
        if let Some(pipe) = child.stdout.take() {
            readers.push(tokio::spawn(drain(pipe, Arc::clone(&stdout), limit)));
        }
        if let Some(pipe) = child.stderr.take() {
            readers.push(tokio::spawn(drain(pipe, Arc::clone(&stderr), limit)));
        }
        if let Some(mut pipe) = child.stdin.take() {
            let text = input.stdin.clone().unwrap_or_default();
            tokio::spawn(async move {
                if !text.is_empty() {
                    let _ = pipe.write_all(text.as_bytes()).await;
                }
                let _ = pipe.shutdown().await;
            });
        }
        let ending = tokio::select! {
            status = child.wait() => Ending::Exited(status.ok().and_then(|status| status.code())),
            () = tokio::time::sleep(self.options.timeout) => Ending::TimedOut,
            () = context.cancel.cancelled() => Ending::Cancelled,
        };
        if !matches!(ending, Ending::Exited(_)) {
            kill_tree(&mut child).await;
            let _ = child.wait().await;
        }
        for reader in readers {
            let handle = reader.abort_handle();
            if tokio::time::timeout(DRAIN_GRACE, reader).await.is_err() {
                handle.abort();
            }
        }
        let exit_code = match ending {
            Ending::Cancelled => return Err(cancelled()),
            Ending::TimedOut => None,
            Ending::Exited(code) => code,
        };
        let (stdout, stdout_truncated) = taken(&stdout);
        let (stderr, stderr_truncated) = taken(&stderr);
        let timed_out = matches!(ending, Ending::TimedOut);
        let mut output = SkillScriptOutput {
            key: input.key,
            path: script.path,
            runtime: runtime.command().to_owned(),
            exit_code,
            timed_out,
            truncated: stdout_truncated || stderr_truncated,
            stdout,
            stderr,
            text: String::new(),
            is_error: timed_out || exit_code != Some(0),
        };
        output.text = script_text(&output, limit, self.options.timeout);
        Ok(output)
    }
}

#[cfg(test)]
mod tests;
