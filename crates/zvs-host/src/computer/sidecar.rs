use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

use crate::drivers::mcp::{LaunchSpec, default_spawn, kill_tree};
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::skills::BUNDLED_SKILLS;

pub const SKILL_KEY: &str = "computer-use";
pub const SERVER_SCRIPT: &str = "scripts/computer_server.py";
pub const PROTOCOL: u64 = 1;
pub const START_TIMEOUT: Duration = Duration::from_secs(240);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(45);
pub const ACT_TIMEOUT: Duration = Duration::from_secs(180);
const STDERR_KEPT: usize = 20;

pub const SIDECAR_FAILED: &str = "Не удалось запустить управление компьютером. Проверьте, что uv подключён в разделе «Расширения → Компоненты».";
pub const SIDECAR_LOST: &str = "Процесс управления компьютером завершился. Повторите действие.";
pub const SIDECAR_TIMEOUT: &str = "Управление компьютером не ответило вовремя";
pub const SIDECAR_PROTOCOL: &str = "Версия сценария управления компьютером не поддерживается";

#[derive(Debug, Clone, PartialEq)]
pub enum DesktopFault {
    Refused {
        code: String,
        message: String,
        window: Value,
    },
    Unavailable(AppError),
}

impl From<AppError> for DesktopFault {
    fn from(error: AppError) -> Self {
        Self::Unavailable(error)
    }
}

#[async_trait]
pub trait DesktopPort: Send + Sync {
    async fn call(
        &self,
        method: &str,
        params: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, DesktopFault>;

    async fn close(&self);
}

pub type SharedDesktop = Arc<dyn DesktopPort>;

pub struct NoDesktop;

#[async_trait]
impl DesktopPort for NoDesktop {
    async fn call(
        &self,
        _method: &str,
        _params: Value,
        _cancel: &CancellationToken,
    ) -> Result<Value, DesktopFault> {
        Err(failure(SIDECAR_FAILED).into())
    }

    async fn close(&self) {}
}

pub type SidecarLauncher = Arc<dyn Fn(&[String]) -> Result<LaunchSpec, AppError> + Send + Sync>;

struct Process {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

pub struct UvSidecar {
    launcher: SidecarLauncher,
    folder: PathBuf,
    logger: SharedLogger,
    process: tokio::sync::Mutex<Option<Process>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

fn failure(message: &str) -> AppError {
    AppError::new(AppErrorCode::SidecarUnavailable, message)
}

pub fn bundled_scripts() -> Vec<(&'static str, &'static str)> {
    BUNDLED_SKILLS
        .iter()
        .find(|skill| skill.key == SKILL_KEY)
        .map(|skill| {
            skill
                .files
                .iter()
                .filter(|file| file.path.starts_with("scripts/") && file.path.ends_with(".py"))
                .map(|file| (file.path, file.content))
                .collect()
        })
        .unwrap_or_default()
}

pub fn write_scripts(folder: &Path) -> Result<PathBuf, AppError> {
    let scripts = folder.join("scripts");
    std::fs::create_dir_all(&scripts).map_err(|_| failure(SIDECAR_FAILED))?;
    let bundled = bundled_scripts();
    if !bundled.iter().any(|(path, _)| *path == SERVER_SCRIPT) {
        return Err(failure(SIDECAR_FAILED));
    }
    for (path, content) in bundled {
        let target = folder.join(path);
        let current = std::fs::read_to_string(&target).ok();
        if current.as_deref() != Some(content) {
            std::fs::write(&target, content).map_err(|_| failure(SIDECAR_FAILED))?;
        }
    }
    Ok(folder.join(SERVER_SCRIPT))
}

fn timeout_for(method: &str) -> Duration {
    match method {
        "hello" => START_TIMEOUT,
        "act" | "launch" | "inspect" => ACT_TIMEOUT,
        _ => CALL_TIMEOUT,
    }
}

impl UvSidecar {
    pub fn new(launcher: SidecarLauncher, folder: PathBuf, logger: SharedLogger) -> Self {
        Self {
            launcher,
            folder,
            logger,
            process: tokio::sync::Mutex::new(None),
            stderr: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn drain(&self, child: &mut Child) {
        let Some(stderr) = child.stderr.take() else {
            return;
        };
        let kept = Arc::clone(&self.stderr);
        let logger = self.logger.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line: String = line.chars().take(400).collect();
                logger.log(
                    LogLevel::Debug,
                    "computer",
                    "Sidecar output",
                    &[("line", json!(line))],
                );
                let mut kept = kept.lock().unwrap_or_else(PoisonError::into_inner);
                kept.push_back(line);
                while kept.len() > STDERR_KEPT {
                    kept.pop_front();
                }
            }
        });
    }

    async fn spawn(&self) -> Result<Process, AppError> {
        let script = write_scripts(&self.folder)?;
        let mut spec = (self.launcher)(&[
            "run".to_owned(),
            "--quiet".to_owned(),
            "--script".to_owned(),
            script.to_string_lossy().into_owned(),
        ])?;
        spec.env
            .insert("PYTHONDONTWRITEBYTECODE".to_owned(), "1".to_owned());
        spec.env.insert("PYTHONUTF8".to_owned(), "1".to_owned());
        spec.cwd = Some(self.folder.clone());
        let mut child = default_spawn(&spec).map_err(|error| {
            self.logger.log(
                LogLevel::Warn,
                "computer",
                "Could not start the sidecar",
                &[("error", json!(error.to_string()))],
            );
            failure(SIDECAR_FAILED)
        })?;
        self.drain(&mut child);
        let stdin = child.stdin.take().ok_or_else(|| failure(SIDECAR_FAILED))?;
        let stdout = child.stdout.take().ok_or_else(|| failure(SIDECAR_FAILED))?;
        self.logger.log(
            LogLevel::Info,
            "computer",
            "Started the sidecar",
            &[("pid", json!(child.id()))],
        );
        Ok(Process {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next: 0,
        })
    }

    fn last_error(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .back()
            .cloned()
            .unwrap_or_default()
    }

    async fn exchange(
        process: &mut Process,
        method: &str,
        params: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, Exchange> {
        process.next += 1;
        let id = process.next;
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        process
            .stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| Exchange::Lost)?;
        process.stdin.flush().await.map_err(|_| Exchange::Lost)?;
        loop {
            let mut reply = String::new();
            let read = tokio::select! {
                () = cancel.cancelled() => return Err(Exchange::Cancelled),
                read = tokio::time::timeout(timeout_for(method), process.stdout.read_line(&mut reply)) => read,
            };
            match read {
                Err(_) => return Err(Exchange::TimedOut),
                Ok(Err(_) | Ok(0)) => return Err(Exchange::Lost),
                Ok(Ok(_)) => {}
            }
            let Ok(value) = serde_json::from_str::<Value>(reply.trim()) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
        }
    }

    async fn handshake(&self, process: &mut Process) -> Result<(), AppError> {
        let reply = Self::exchange(process, "hello", json!({}), &CancellationToken::new())
            .await
            .map_err(|_| {
                self.logger.log(
                    LogLevel::Warn,
                    "computer",
                    "The sidecar did not answer the handshake",
                    &[("stderr", json!(self.last_error()))],
                );
                failure(SIDECAR_FAILED)
            })?;
        let protocol = reply
            .pointer("/result/protocol")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if protocol != PROTOCOL {
            return Err(failure(SIDECAR_PROTOCOL));
        }
        Ok(())
    }
}

enum Exchange {
    Cancelled,
    TimedOut,
    Lost,
}

pub fn fault_of(reply: &Value) -> Result<Value, DesktopFault> {
    if reply.get("ok") == Some(&Value::Bool(true)) {
        return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
    }
    let error = reply.get("error").cloned().unwrap_or(Value::Null);
    Err(DesktopFault::Refused {
        code: error
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("failed")
            .to_owned(),
        message: error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        window: error.get("window").cloned().unwrap_or(Value::Null),
    })
}

#[async_trait]
impl DesktopPort for UvSidecar {
    async fn call(
        &self,
        method: &str,
        params: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, DesktopFault> {
        let mut slot = self.process.lock().await;
        if slot.is_none() {
            let mut process = self.spawn().await?;
            if let Err(error) = self.handshake(&mut process).await {
                kill_tree(&mut process.child).await;
                return Err(error.into());
            }
            *slot = Some(process);
        }
        let Some(process) = slot.as_mut() else {
            return Err(failure(SIDECAR_FAILED).into());
        };
        match Self::exchange(process, method, params, cancel).await {
            Ok(reply) => fault_of(&reply),
            Err(problem) => {
                if let Some(mut process) = slot.take() {
                    kill_tree(&mut process.child).await;
                }
                let (message, code) = match problem {
                    Exchange::Cancelled => {
                        return Err(crate::kernel::types::cancelled().into());
                    }
                    Exchange::TimedOut => (SIDECAR_TIMEOUT, AppErrorCode::SidecarUnavailable),
                    Exchange::Lost => (SIDECAR_LOST, AppErrorCode::SidecarUnavailable),
                };
                self.logger.log(
                    LogLevel::Warn,
                    "computer",
                    "The sidecar stopped answering",
                    &[
                        ("method", json!(method)),
                        ("stderr", json!(self.last_error())),
                    ],
                );
                Err(AppError::new(code, message).into())
            }
        }
    }

    async fn close(&self) {
        let mut slot = self.process.lock().await;
        if let Some(mut process) = slot.take() {
            drop(process.stdin);
            if tokio::time::timeout(Duration::from_secs(2), process.child.wait())
                .await
                .is_err()
            {
                kill_tree(&mut process.child).await;
            }
            self.logger
                .log(LogLevel::Info, "computer", "Stopped the sidecar", &[]);
        }
    }
}
