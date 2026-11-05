use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

use super::{BUILD_FAILED, DashboardRunner, FETCH_FAILED};
use crate::artifacts::{DASHBOARDS, library_env};
use crate::commands::nodes::PYTHON_UTF8_ENV;
use crate::skills::SkillLibrary;
use crate::skills::script::{SKILL_DIR_ENV, ScriptRuntime};
use crate::toolchain::process::{ProcessLimits, ProcessRun, run_process};
use crate::toolchain::{CommandLauncher, LaunchSpec};

pub const DASHBOARD_SKILL: &str = "dashboard";
pub const BUILDER: &str = "scripts/build_dashboard.py";
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
const BUILD_TIMEOUT: Duration = Duration::from_secs(300);
const OUTPUT_BYTES: usize = 64 * 1024;
const DETAIL_CHARS: usize = 4_000;

pub struct ProcessDashboardRunner {
    launcher: Arc<dyn CommandLauncher>,
    skills: SkillLibrary,
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn tail(run: &ProcessRun) -> String {
    let text = if run.stderr.trim().is_empty() {
        &run.stdout
    } else {
        &run.stderr
    };
    let count = text.chars().count();
    text.chars()
        .skip(count.saturating_sub(DETAIL_CHARS))
        .collect()
}

impl ProcessDashboardRunner {
    pub fn new(launcher: Arc<dyn CommandLauncher>, skills: SkillLibrary) -> Self {
        Self { launcher, skills }
    }

    fn spec(
        &self,
        file: &Path,
        args: &[String],
        failed: &'static str,
    ) -> Result<LaunchSpec, AppError> {
        let runtime = ScriptRuntime::of(&text(file))
            .ok_or_else(|| AppError::new(AppErrorCode::UnsupportedFormat, failed))?;
        let mut spec = self
            .launcher
            .launch_command(runtime.command(), &runtime.args(&text(file), args))
            .unwrap_or_else(|| {
                Err(AppError::new(AppErrorCode::NotFound, failed)
                    .with_detail("runtime", runtime.command()))
            })?;
        if runtime == ScriptRuntime::Python {
            for (name, value) in PYTHON_UTF8_ENV {
                spec.env
                    .entry((*name).to_owned())
                    .or_insert_with(|| (*value).to_owned());
            }
        }
        Ok(spec)
    }

    async fn execute(
        spec: &LaunchSpec,
        timeout: Duration,
        failed: &'static str,
    ) -> Result<ProcessRun, AppError> {
        let limits = ProcessLimits {
            timeout,
            output_bytes: OUTPUT_BYTES,
        };
        let run = run_process(spec, None, limits, &CancellationToken::new(), failed).await?;
        if run.failed() {
            return Err(AppError::new(AppErrorCode::Unknown, failed)
                .with_detail("exitCode", run.exit_code)
                .with_detail("timedOut", run.timed_out)
                .with_detail("output", tail(&run)));
        }
        Ok(run)
    }
}

#[async_trait]
impl DashboardRunner for ProcessDashboardRunner {
    async fn fetch(&self, script: &Path, output: &Path, cwd: &Path) -> Result<(), AppError> {
        let mut spec = self.spec(script, &[text(output)], FETCH_FAILED)?;
        spec.cwd = Some(cwd.to_path_buf());
        Self::execute(&spec, FETCH_TIMEOUT, FETCH_FAILED)
            .await
            .map(|_| ())
    }

    async fn build(&self, library: &Path, stem: &str) -> Result<(), AppError> {
        let script = self.skills.script(DASHBOARD_SKILL, BUILDER)?;
        let mut spec = self.spec(
            &script.file,
            &["--refresh".to_owned(), stem.to_owned()],
            BUILD_FAILED,
        )?;
        spec.cwd = Some(library.to_path_buf());
        spec.env.insert(SKILL_DIR_ENV.to_owned(), text(&script.dir));
        spec.env.insert(library_env(DASHBOARDS), text(library));
        let run = Self::execute(&spec, BUILD_TIMEOUT, BUILD_FAILED).await?;
        let report: serde_json::Value = serde_json::from_str(run.stdout.trim()).unwrap_or_default();
        if report["ok"] == serde_json::json!(true) {
            Ok(())
        } else {
            Err(AppError::new(AppErrorCode::Unknown, BUILD_FAILED).with_detail("report", report))
        }
    }
}
