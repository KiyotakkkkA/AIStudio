use std::future::Future;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rmcp::RoleClient;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use zvs_contract::validate::js_slice;
use zvs_contract::{AppError, AppErrorCode, McpLogLevel};

use super::transport::{MAX_MESSAGE_BYTES, STOPPED, TransportHooks, decode_text};
pub use crate::toolchain::LaunchSpec;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CLOSE_GRACE: Duration = Duration::from_secs(2);
pub const NOT_RUNNING: &str = "MCP-сервер не запущен";

pub type SpawnServer = Arc<dyn Fn(&LaunchSpec) -> std::io::Result<Child> + Send + Sync>;

#[cfg(windows)]
fn platform_command(spec: &LaunchSpec) -> Command {
    use std::collections::HashMap;

    use super::windows_command::{is_file, windows_launch};

    let mut merged: HashMap<String, String> = std::env::vars().collect();
    for (name, value) in &spec.env {
        merged.retain(|key, _| !key.eq_ignore_ascii_case(name));
        merged.insert(name.clone(), value.clone());
    }
    let cwd = spec
        .cwd
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let launch = windows_launch(&spec.command, &spec.args, &merged, &is_file, &cwd);
    let mut command = Command::new(&launch.file);
    if launch.verbatim {
        for argument in &launch.args {
            command.raw_arg(argument);
        }
    } else {
        command.args(&launch.args);
    }
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn platform_command(spec: &LaunchSpec) -> Command {
    let mut command = Command::new(&spec.command);
    command.args(&spec.args);
    command
}

pub fn default_spawn(spec: &LaunchSpec) -> std::io::Result<Child> {
    let mut command = platform_command(spec);
    command
        .envs(&spec.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    command.spawn()
}

pub fn cannot_start(spec: &LaunchSpec, error: &std::io::Error) -> AppError {
    let reason = match error.kind() {
        std::io::ErrorKind::NotFound => {
            format!(" — команда «{}» не найдена (ENOENT)", spec.command)
        }
        std::io::ErrorKind::PermissionDenied => " — EACCES".to_owned(),
        _ => format!(" — {error}"),
    };
    let line = format!("{} {}", spec.command, spec.args.join(" "));
    AppError::new(
        AppErrorCode::McpUnavailable,
        format!("Не удалось запустить MCP-сервер: {}{reason}", line.trim()),
    )
    .with_detail("command", spec.command.clone())
}

pub async fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut taskkill = Command::new("taskkill");
        taskkill
            .args(["/pid", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        if let Ok(status) = taskkill.status().await
            && status.success()
        {
            return;
        }
    }
    let _ = child.start_kill();
}

fn exit_reason(command: &str, status: Option<std::process::ExitStatus>) -> String {
    let code = status
        .and_then(|status| status.code())
        .map_or_else(|| "null".to_owned(), |code| code.to_string());
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status
            .and_then(|status| status.signal())
            .map_or_else(|| "null".to_owned(), |signal| signal.to_string())
    };
    #[cfg(not(unix))]
    let signal = "null".to_owned();
    format!("{command} завершился (code={code}, signal={signal})")
}

pub struct StdioTransport {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    incoming: mpsc::UnboundedReceiver<RxJsonRpcMessage<RoleClient>>,
    stop: Option<oneshot::Sender<()>>,
    exited: watch::Receiver<bool>,
    closing: Arc<AtomicBool>,
}

impl StdioTransport {
    pub fn open(
        spec: &LaunchSpec,
        spawn: Option<&SpawnServer>,
        hooks: TransportHooks,
    ) -> Result<Self, AppError> {
        let spawned = match spawn {
            Some(spawn) => spawn(spec),
            None => default_spawn(spec),
        };
        let mut child = spawned.map_err(|error| cannot_start(spec, &error))?;
        let (stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(stdin), Some(stdout), stderr) => (stdin, stdout, stderr),
                _ => {
                    let _ = child.start_kill();
                    return Err(cannot_start(
                        spec,
                        &std::io::Error::other("stdio was not piped"),
                    ));
                }
            };
        let (sender, incoming) = mpsc::unbounded_channel();
        let reader_hooks = hooks.clone();
        let (child_tx, child_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if buffer.len() > MAX_MESSAGE_BYTES {
                    reader_hooks.log(
                        McpLogLevel::Error,
                        "Сервер прислал строку сверх допустимого размера",
                    );
                    let _ = child_tx.send(());
                    break;
                }
                let line = String::from_utf8_lossy(&buffer);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(message) = decode_text(line, &reader_hooks)
                    && sender.send(message).is_err()
                {
                    break;
                }
            }
        });
        if let Some(stderr) = stderr {
            let stderr_hooks = hooks.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = line.trim();
                    if !line.is_empty() {
                        stderr_hooks.log(McpLogLevel::Debug, js_slice(line, 1000));
                    }
                }
            });
        }
        let (stop, stop_rx) = oneshot::channel::<()>();
        let (exited_tx, exited) = watch::channel(false);
        let closing = Arc::new(AtomicBool::new(false));
        let watcher_closing = Arc::clone(&closing);
        let command = spec.command.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                status = child.wait() => status.ok(),
                _ = stop_rx => {
                    kill_tree(&mut child).await;
                    child.wait().await.ok()
                }
                _ = child_rx => {
                    kill_tree(&mut child).await;
                    child.wait().await.ok()
                }
            };
            let _ = exited_tx.send(true);
            if watcher_closing.load(Ordering::SeqCst) {
                hooks.close(STOPPED);
            } else {
                hooks.close(&exit_reason(&command, status));
            }
        });
        Ok(Self {
            stdin: Arc::new(Mutex::new(Some(stdin))),
            incoming,
            stop: Some(stop),
            exited,
            closing,
        })
    }
}

impl Transport<RoleClient> for StdioTransport {
    type Error = std::io::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let stdin = Arc::clone(&self.stdin);
        async move {
            let mut line = serde_json::to_vec(&item).map_err(std::io::Error::other)?;
            line.push(b'\n');
            let mut guard = stdin.lock().await;
            let Some(stdin) = guard.as_mut() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    NOT_RUNNING,
                ));
            };
            stdin.write_all(&line).await?;
            stdin.flush().await
        }
    }

    fn receive(&mut self) -> impl Future<Output = Option<RxJsonRpcMessage<RoleClient>>> + Send {
        self.incoming.recv()
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.closing.store(true, Ordering::SeqCst);
        drop(self.stdin.lock().await.take());
        let mut exited = self.exited.clone();
        let finished = tokio::time::timeout(CLOSE_GRACE, async {
            let _ = exited.wait_for(|done| *done).await;
        })
        .await
        .is_ok();
        if !finished && let Some(stop) = self.stop.take() {
            let _ = stop.send(());
            let mut exited = self.exited.clone();
            let _ = exited.wait_for(|done| *done).await.map(|_| ());
        }
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if !*self.exited.borrow()
            && let Some(stop) = self.stop.take()
        {
            self.closing.store(true, Ordering::SeqCst);
            let _ = stop.send(());
        }
    }
}
