//! Copilot CLI JSON-RPC over framed stdio. No command shell or vendor SDK.
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

pub const CLI_MISSING: &str =
    "Не удалось запустить Copilot CLI. Установите GitHub Copilot CLI и повторите вход";
pub const PROTOCOL_ERROR: &str =
    "Copilot CLI вернул неизвестный ответ. Обновите GitHub Copilot CLI";
pub const SESSION_EXPIRED: &str = "Сессия GitHub Copilot недоступна. Выполните вход заново";
pub const REQUEST_FAILED: &str = "Не удалось выполнить запрос GitHub Copilot";

pub fn failure() -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, REQUEST_FAILED)
}

pub fn protocol_error() -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, PROTOCOL_ERROR)
}

pub fn auth_error() -> AppError {
    AppError::new(AppErrorCode::ProviderSessionExpired, SESSION_EXPIRED)
}

fn rpc_error(error: &Value) -> AppError {
    if matches!(
        error.get("code").and_then(Value::as_i64),
        Some(-32601 | -32602)
    ) {
        protocol_error()
    } else {
        failure()
    }
}

fn program() -> Result<PathBuf, AppError> {
    let name = if cfg!(windows) {
        "copilot.exe"
    } else {
        "copilot"
    };
    for directory in std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
    {
        let direct = directory.join(name);
        // VS Code ships a bootstrap shim, not the CLI server.
        if direct.is_file()
            && directory
                .file_name()
                .is_none_or(|part| part != "copilot-shim")
        {
            return Ok(direct);
        }
        for root in [
            directory.join("node_modules/@github/copilot"),
            directory.join("node_modules/@github/copilot/node_modules"),
        ] {
            let platform = if cfg!(windows) {
                "win32"
            } else if cfg!(target_os = "macos") {
                "darwin"
            } else {
                "linux"
            };
            let arch = if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x64"
            };
            for candidate in [
                root.join(name),
                directory.join(format!(
                    "node_modules/@github/copilot-{platform}-{arch}/{name}"
                )),
                root.join(format!(
                    "node_modules/@github/copilot-{platform}-{arch}/{name}"
                )),
                root.join(format!("@github/copilot-{platform}-{arch}/{name}")),
            ] {
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }
    Err(AppError::new(
        AppErrorCode::ProviderUnreachable,
        CLI_MISSING,
    ))
}

async fn frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value, AppError> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = Vec::new();
        let read = (&mut *reader)
            .take(8193)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|_| failure())?;
        header_bytes += read;
        if read == 0 || header_bytes > 8192 {
            return Err(protocol_error());
        }
        if line == b"\r\n" {
            break;
        }
        let line = std::str::from_utf8(&line).map_err(|_| protocol_error())?;
        if let Some((key, value)) = line.split_once(':')
            && key.eq_ignore_ascii_case("Content-Length")
        {
            if length.is_some() {
                return Err(protocol_error());
            }
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| protocol_error())?,
            );
        }
    }
    let length = length
        .filter(|n| *n > 0 && *n <= zvs_copilot::MAX_FRAME)
        .ok_or_else(protocol_error)?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.map_err(|_| failure())?;
    serde_json::from_slice(&body).map_err(|_| protocol_error())
}

pub struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    directory: tempfile::TempDir,
    next: u64,
    cancel: CancellationToken,
    inbox: VecDeque<Value>,
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Client {
    pub async fn start(cancel: &CancellationToken) -> Result<Self, AppError> {
        Self::start_command(Command::new(program()?), cancel).await
    }

    async fn start_command(
        mut command: Command,
        cancel: &CancellationToken,
    ) -> Result<Self, AppError> {
        if cancel.is_cancelled() {
            return Err(super::errors::cancelled());
        }
        let directory = tempfile::Builder::new()
            .prefix("zvs-copilot-")
            .tempdir()
            .map_err(|_| failure())?;
        command
            .args(["--headless", "--stdio", "--no-auto-update"])
            .current_dir(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // Do not silently route a linked account through ambient tokens or BYOK.
        for key in [
            "COPILOT_GITHUB_TOKEN",
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GITHUB_COPILOT_API_TOKEN",
            "COPILOT_API_URL",
            "COPILOT_PROVIDER_BASE_URL",
            "COPILOT_PROVIDER_API_KEY",
            "NODE_OPTIONS",
            "NODE_DEBUG",
            "COPILOT_ALLOW_ALL",
            "COPILOT_ASSISTED_APPROVAL",
            "GITHUB_COPILOT_PROMPT_MODE_EXTENSIONS",
        ] {
            command.env_remove(key);
        }
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .map_err(|_| AppError::new(AppErrorCode::ProviderUnreachable, CLI_MISSING))?;
        let input = child.stdin.take().ok_or_else(failure)?;
        let output = BufReader::new(child.stdout.take().ok_or_else(failure)?);
        let mut client = Self {
            child,
            input,
            output,
            directory,
            next: 0,
            cancel: cancel.clone(),
            inbox: VecDeque::new(),
        };
        let status = client.call("ping", json!({})).await?;
        if status
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .is_none_or(|version| version < 3)
        {
            return Err(protocol_error());
        }
        Ok(client)
    }

    async fn write(&mut self, value: Value) -> Result<(), AppError> {
        let body = serde_json::to_vec(&value).map_err(|_| protocol_error())?;
        if body.len() > zvs_copilot::MAX_FRAME {
            return Err(protocol_error());
        }
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(super::errors::cancelled()),
            result = async {
                self.input.write_all(header.as_bytes()).await?;
                self.input.write_all(&body).await?;
                self.input.flush().await
            } => result.map_err(|_| failure()),
        }
    }

    async fn read(&mut self) -> Result<Value, AppError> {
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(super::errors::cancelled()),
            result = tokio::time::timeout(Duration::from_secs(120), frame(&mut self.output)) => result.map_err(|_| failure())?,
        }
    }

    async fn refuse(&mut self, value: &Value) -> Result<bool, AppError> {
        if value.get("method").is_none() || value.get("id").is_none() {
            return Ok(false);
        }
        let result = match value["method"].as_str() {
            Some("permission.request") => {
                json!({"kind":"denied-no-approval-rule-and-could-not-request-from-user"})
            }
            _ => {
                self.write(json!({"jsonrpc":"2.0", "id":value["id"], "error":{"code":-32601,"message":"Unsupported host request"}})).await?;
                return Ok(true);
            }
        };
        self.write(json!({"jsonrpc":"2.0", "id":value["id"], "result":result}))
            .await?;
        Ok(true)
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        self.next += 1;
        let id = self.next;
        self.write(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}))
            .await?;
        loop {
            let value = self.read().await?;
            if self.refuse(&value).await? {
                continue;
            }
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = value.get("error") {
                    return Err(rpc_error(error));
                }
                return value.get("result").cloned().ok_or_else(protocol_error);
            }
            if self.inbox.len() >= 4096 {
                return Err(protocol_error());
            }
            self.inbox.push_back(value);
        }
    }

    pub async fn auth(
        &mut self,
        expected: Option<&str>,
    ) -> Result<zvs_copilot::AuthStatus, AppError> {
        let status: zvs_copilot::AuthStatus =
            serde_json::from_value(self.call("auth.getStatus", json!({})).await?)
                .map_err(|_| protocol_error())?;
        let identity = status.identity_key().ok_or_else(auth_error)?;
        if expected.is_some_and(|expected| identity != expected) {
            return Err(auth_error());
        }
        Ok(status)
    }

    pub async fn models(&mut self) -> Result<Vec<zvs_copilot::Model>, AppError> {
        let response = self.call("models.list", json!({})).await?;
        serde_json::from_value(response.get("models").cloned().ok_or_else(protocol_error)?)
            .map_err(|_| protocol_error())
    }

    pub async fn session(
        &mut self,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<String, AppError> {
        let mut config =
            zvs_copilot::session_config(model, &self.directory.path().to_string_lossy());
        if let Some(effort) = effort {
            config["reasoningEffort"] = json!(effort);
        }
        let response = self.call("session.create", config).await?;
        let session = response
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(protocol_error)?;
        self.call(
            "session.options.update",
            zvs_copilot::session_options(&session),
        )
        .await?;
        Ok(session)
    }

    pub async fn event(&mut self, session: &str) -> Result<Value, AppError> {
        loop {
            let value = if let Some(value) = self.inbox.pop_front() {
                value
            } else {
                self.read().await?
            };
            if self.refuse(&value).await? {
                continue;
            }
            if value["method"] == "session.event" && value["params"]["sessionId"] == session {
                return Ok(value["params"]["event"].clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_errors_do_not_expose_vendor_messages_or_mislabel_network_failures_as_old_clients() {
        assert_eq!(rpc_error(&json!({"code":-32601})).message, PROTOCOL_ERROR);
        assert_eq!(
            rpc_error(&json!({"code":-32000,"message":"private diagnostic"})).message,
            REQUEST_FAILED
        );
    }

    #[tokio::test]
    #[ignore = "requires ZVS_COPILOT_TEST_CLI pointing to a real Copilot CLI binary"]
    async fn real_cli_accepts_the_isolated_session_configuration_without_login() {
        let home = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::var_os("ZVS_COPILOT_TEST_CLI").expect("CLI path"));
        command
            .arg("--no-auto-login")
            .env("COPILOT_HOME", home.path());
        let cancel = CancellationToken::new();
        let mut client = Client::start_command(command, &cancel).await.unwrap();
        assert_eq!(
            client.auth(None).await.unwrap_err().code,
            AppErrorCode::ProviderSessionExpired
        );
        assert!(!client.session(None, None).await.unwrap().is_empty());
    }

    async fn fake(scenario: &str, cancel: &CancellationToken) -> Result<Client, AppError> {
        let mut command = Command::new("node");
        command
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/helpers/fakeCopilot.mjs"
            ))
            .arg(scenario);
        Client::start_command(command, cancel).await
    }

    #[tokio::test]
    async fn rpc_preserves_early_events_checks_identity_and_refuses_local_permissions() {
        let cancel = CancellationToken::new();
        let mut client = fake("permission", &cancel).await.unwrap();
        client.auth(Some("github.com/octocat")).await.unwrap();
        assert_eq!(
            client
                .auth(Some("github.com/another"))
                .await
                .unwrap_err()
                .code,
            AppErrorCode::ProviderSessionExpired
        );
        let models = client.models().await.unwrap();
        assert_eq!(models[0].id, "model");
        assert_eq!(models[0].effort(Some("low")), Ok(Some("low")));
        let session = client.session(Some("model"), Some("low")).await.unwrap();
        client
            .call(
                "session.send",
                json!({"sessionId":session,"prompt":"hello"}),
            )
            .await
            .unwrap();
        assert_eq!(
            client.event(&session).await.unwrap()["data"]["deltaContent"],
            "Ответ"
        );
        assert_eq!(
            client.event(&session).await.unwrap()["type"],
            "session.idle"
        );
        // The fake exits if the server's permission request was not denied.
        client.call("ping", json!({})).await.unwrap();
        cancel.cancel();
        assert_eq!(
            client.event(&session).await.unwrap_err().code,
            AppErrorCode::RunCancelled
        );
    }

    #[tokio::test]
    async fn old_protocol_signed_out_and_precancelled_requests_fail_closed() {
        let cancel = CancellationToken::new();
        assert!(fake("old", &cancel).await.is_err());
        let mut client = fake("signed-out", &cancel).await.unwrap();
        assert_eq!(
            client.auth(None).await.unwrap_err().code,
            AppErrorCode::ProviderSessionExpired
        );
        cancel.cancel();
        assert!(fake("unused", &cancel).await.is_err());
    }

    #[tokio::test]
    async fn frames_count_utf8_bytes_and_reject_duplicate_and_oversized_lengths() {
        let body = json!({"text":"Ответ"}).to_string();
        let bytes = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        assert_eq!(
            frame(&mut BufReader::new(bytes.as_bytes())).await.unwrap()["text"],
            "Ответ"
        );
        for invalid in [
            "Content-Length: 9\r\nContent-Length: 9\r\n\r\n{}",
            "Content-Length: 8388609\r\n\r\n",
            "Content-Length: -1\r\n\r\n",
            "Content-Length: 2\r\n\r\n{",
        ] {
            assert!(
                frame(&mut BufReader::new(invalid.as_bytes()))
                    .await
                    .is_err()
            );
        }
    }
}
