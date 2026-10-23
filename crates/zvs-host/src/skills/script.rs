use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zvs_contract::validate::{check, invalid};
use zvs_contract::{AppError, AppErrorCode, ArtifactDto, Validate};

use super::library::SkillLibrary;
use crate::artifacts::{ArtifactStore, mentioned_paths, produced, snapshot};
use crate::kernel::{NodeDef, PermissionRequirement, StepContext};
use crate::toolchain::CommandLauncher;
use crate::toolchain::process::{
    PROCESS_OUTPUT_BYTES, PROCESS_TIMEOUT, ProcessLimits, SCRATCH_FAILED, process_text, run_process,
};
pub use crate::toolchain::process::{SCRATCH_KEEP, prepare_scratch};
use crate::toolchain::service::SETTINGS_HINT;

pub const SKILL_SCRIPT: &str = "skill.script";
pub const SCRIPT_DESCRIPTION: &str = "Запуск скрипта навыка из папки scripts/ по идентификатору навыка и пути из результата skill.load: файлы .py выполняются через uv, файлы .js, .mjs и .cjs — через Node.js. Рабочая папка — временная папка запуска; папка навыка передаётся в переменной окружения ZVS_SKILL_DIR и предназначена только для чтения. Возвращает код завершения, stdout и stderr.";

pub const SCRIPT_UNSUPPORTED: &str =
    "Скрипт навыка должен иметь расширение .py, .js, .mjs или .cjs";
pub const SCRIPT_NO_TOOLCHAIN: &str = "Среда выполнения скриптов недоступна";
pub const SCRIPT_NOT_STARTED: &str = "Не удалось запустить скрипт навыка";
pub const SCRIPT_SCRATCH_FAILED: &str = SCRATCH_FAILED;

pub const SKILL_DIR_ENV: &str = "ZVS_SKILL_DIR";
pub const SCRATCH_DIR_ENV: &str = "ZVS_SCRATCH_DIR";
pub const PYTHON_UTF8_ENV: &[(&str, &str)] = &[("PYTHONUTF8", "1"), ("PYTHONIOENCODING", "utf-8")];
pub const SCRIPT_TIMEOUT: Duration = PROCESS_TIMEOUT;
pub const SCRIPT_OUTPUT_BYTES: usize = PROCESS_OUTPUT_BYTES;
const ARGS_MAX: usize = 64;
const ARG_MAX_CHARS: usize = 4096;
const STDIN_MAX_CHARS: usize = 1_048_576;

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDto>,
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
    pub artifacts: Option<ArtifactStore>,
}

impl SkillScriptOptions {
    pub fn new(launcher: Option<Arc<dyn CommandLauncher>>, scratch: PathBuf) -> Self {
        Self {
            launcher,
            scratch,
            timeout: SCRIPT_TIMEOUT,
            output_bytes: SCRIPT_OUTPUT_BYTES,
            artifacts: None,
        }
    }
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
        spec.cwd = Some(scratch.clone());
        let before = snapshot(&scratch);
        let started = std::time::SystemTime::now();
        context.check_cancelled()?;
        let limits = ProcessLimits {
            timeout: self.options.timeout,
            output_bytes: self.options.output_bytes,
        };
        let run = run_process(
            &spec,
            input.stdin.as_deref(),
            limits,
            &context.cancel,
            SCRIPT_NOT_STARTED,
        )
        .await?;
        let artifacts = self
            .options
            .artifacts
            .as_ref()
            .map_or_else(Vec::new, |store| {
                let mentioned = mentioned_paths(&input.args, &scratch);
                store.keep(
                    &context.run_id.to_string(),
                    &produced(&scratch, &before, &mentioned, started),
                )
            });
        let mut text = process_text(&run, "script", limits);
        if !artifacts.is_empty() {
            let names: Vec<&str> = artifacts
                .iter()
                .map(|artifact| artifact.name.as_str())
                .collect();
            text.push_str(&format!(
                "

Files shown to the user with download buttons: {}",
                names.join(", ")
            ));
        }
        Ok(SkillScriptOutput {
            key: input.key,
            path: script.path,
            runtime: runtime.command().to_owned(),
            exit_code: run.exit_code,
            timed_out: run.timed_out,
            truncated: run.truncated,
            text,
            is_error: run.failed(),
            stdout: run.stdout,
            stderr: run.stderr,
            artifacts,
        })
    }

    fn tool_meta(
        &self,
        _input: &serde_json::Value,
        output: &SkillScriptOutput,
    ) -> Option<serde_json::Value> {
        (!output.artifacts.is_empty()).then(|| serde_json::json!({ "artifacts": output.artifacts }))
    }
}

#[cfg(test)]
mod tests;
