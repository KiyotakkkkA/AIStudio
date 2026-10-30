use std::collections::VecDeque;
use std::process::Stdio;
use std::time::Duration;

use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, ProviderUsageWindowDto};

use super::{Event, Events, stream_error};
use crate::drivers::ai::errors::cancelled;
use crate::drivers::ai::openai::failure;
use crate::drivers::ai::ports::{GenerateRequest, TextDelta, TokenUsage, ToolCall};
use crate::drivers::ai::transport::SharedTransport;

fn npm_codex(prefix: &std::path::Path) -> Option<std::path::PathBuf> {
    let root = prefix
        .join("node_modules/@openai/codex")
        .canonicalize()
        .ok()?;
    let (package, target, binary) = if cfg!(all(windows, target_arch = "aarch64")) {
        ("codex-win32-arm64", "aarch64-pc-windows-msvc", "codex.exe")
    } else if cfg!(windows) {
        ("codex-win32-x64", "x86_64-pc-windows-msvc", "codex.exe")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        ("codex-darwin-arm64", "aarch64-apple-darwin", "codex")
    } else if cfg!(target_os = "macos") {
        ("codex-darwin-x64", "x86_64-apple-darwin", "codex")
    } else if cfg!(target_arch = "aarch64") {
        ("codex-linux-arm64", "aarch64-unknown-linux-musl", "codex")
    } else {
        ("codex-linux-x64", "x86_64-unknown-linux-musl", "codex")
    };
    let vendor = format!("vendor/{target}/bin/{binary}");
    [
        root.join(format!("node_modules/@openai/{package}/{vendor}")),
        root.parent()?.join(format!("{package}/{vendor}")),
        root.join(vendor),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn codex_program() -> std::path::PathBuf {
    let name = if cfg!(windows) { "codex.exe" } else { "codex" };
    let paths = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    for path in paths {
        let direct = path.join(name);
        if direct.is_file() {
            return direct;
        }
        if let Some(program) = npm_codex(&path) {
            return program;
        }
    }
    std::path::PathBuf::from(name)
}

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    _directory: tempfile::TempDir,
    next: u64,
    cancel: CancellationToken,
    inbox: VecDeque<Value>,
    last_usage: Option<TokenUsage>,
    allowed_tools: std::collections::HashSet<String>,
}

impl Client {
    async fn start(
        transport: SharedTransport,
        cancel: CancellationToken,
    ) -> Result<Self, AppError> {
        Self::start_command(transport, cancel, Command::new(codex_program())).await
    }

    async fn start_command(
        transport: SharedTransport,
        cancel: CancellationToken,
        mut command: Command,
    ) -> Result<Self, AppError> {
        let token = transport.oauth_token(&cancel).await?;
        let directory = tempfile::Builder::new()
            .prefix("zvs-openai-")
            .tempdir()
            .map_err(|_| failure())?;
        command.args(["app-server", "--listen", "stdio://"]);
        for config in [
            "model_provider=\"openai_chatgpt_plan\"",
            "model_providers.openai_chatgpt_plan.name=\"ChatGPT plan\"",
            "model_providers.openai_chatgpt_plan.base_url=\"https://api.openai.com/v1\"",
            "model_providers.openai_chatgpt_plan.env_key=\"ZVS_OPENAI_ACCESS_TOKEN\"",
            "model_providers.openai_chatgpt_plan.wire_api=\"responses\"",
            "model_providers.openai_chatgpt_plan.requires_openai_auth=false",
            "model_providers.openai_chatgpt_plan.supports_websockets=false",
            "features.shell_tool=false",
            "features.unified_exec=false",
            "features.apps=false",
            "features.plugins=false",
            "features.browser_use=false",
            "features.computer_use=false",
            "features.image_generation=false",
            "features.view_image=false",
            "features.code_mode=false",
            "features.multi_agent=false",
            "features.incremental_tools=false",
            "features.hooks=false",
            "features.skip_host_skill_discovery=true",
            "web_search=\"disabled\"",
        ] {
            command.args(["-c", config]);
        }
        command
            .env("ZVS_OPENAI_ACCESS_TOKEN", token)
            .env("CODEX_HOME", directory.path())
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .env_remove("CODEX_ACCESS_TOKEN")
            .current_dir(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::new(
                    AppErrorCode::NotFound,
                    "Codex CLI не найден. Установите Codex CLI или выберите прямое подключение.",
                )
            } else {
                AppError::new(
                    AppErrorCode::ProviderUnreachable,
                    "Не удалось запустить Codex CLI",
                )
            }
        })?;
        let input = child.stdin.take().ok_or_else(failure)?;
        let output = BufReader::new(child.stdout.take().ok_or_else(failure)?);
        let mut client = Self {
            child,
            input,
            output,
            _directory: directory,
            next: 0,
            cancel,
            inbox: VecDeque::new(),
            last_usage: None,
            allowed_tools: std::collections::HashSet::new(),
        };
        client.rpc("initialize", json!({"clientInfo":{"name":zvs_openai::APP_ID, "title":"ZVS AI Studio", "version":env!("CARGO_PKG_VERSION")}, "capabilities":{"experimentalApi":true}})).await?;
        client.send(json!({"method":"initialized"})).await?;
        Ok(client)
    }

    async fn send(&mut self, value: Value) -> Result<(), AppError> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| failure())?;
        bytes.push(b'\n');
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(cancelled()),
            result = self.input.write_all(&bytes) => result.map_err(|_| failure()),
        }
    }

    async fn read(&mut self) -> Result<Value, AppError> {
        let mut line = String::new();
        let read = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return Err(cancelled()),
            result = tokio::time::timeout(Duration::from_secs(180), self.output.read_line(&mut line)) => result.map_err(|_| failure())?.map_err(|_| failure())?,
        };
        if read == 0 || read > 4 * 1024 * 1024 {
            return Err(failure());
        }
        serde_json::from_str(&line).map_err(|_| failure())
    }

    async fn reject_request(&mut self, message: &Value) -> Result<(), AppError> {
        if message.get("method").is_some()
            && let Some(id) = message.get("id")
        {
            self.send(json!({"id":id, "error":{"code":-32601, "message":"Tool execution is owned by ZVS AI Studio"}})).await?;
        }
        Ok(())
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        self.next += 1;
        let id = self.next;
        self.send(json!({"id":id, "method":method, "params":params}))
            .await?;
        loop {
            let response = self.read().await?;
            if response.get("method").is_none()
                && response.get("id").and_then(Value::as_u64) == Some(id)
            {
                if response.get("error").is_some() {
                    if matches!(
                        response.pointer("/error/code").and_then(Value::as_i64),
                        Some(-32601 | -32602)
                    ) {
                        return Err(AppError::new(
                            AppErrorCode::ProviderUnreachable,
                            "Обновите Codex CLI или выберите прямое подключение OpenAI",
                        ));
                    }
                    return Err(failure());
                }
                return response.get("result").cloned().ok_or_else(failure);
            }
            if response.get("method").and_then(Value::as_str) == Some("item/tool/call") {
                self.inbox.push_back(response);
                continue;
            }
            self.reject_request(&response).await?;
            if response.get("id").is_none() {
                self.inbox.push_back(response);
            }
        }
    }

    async fn turn(&mut self, request: &GenerateRequest) -> Result<(), AppError> {
        self.allowed_tools = request.tools.iter().map(|tool| tool.name.clone()).collect();
        let dynamic = if request.tools.is_empty() {
            Vec::new()
        } else {
            vec![
                json!({"type":"namespace", "name":"zvs", "description":"Tools provided by ZVS AI Studio", "tools": request.tools.iter().map(|tool| json!({"type":"function", "name":tool.name, "description":tool.description, "inputSchema":tool.parameters})).collect::<Vec<_>>()}),
            ]
        };
        let thread = self.rpc("thread/start", json!({"model":request.model, "ephemeral":true, "approvalPolicy":"untrusted", "sandbox":"read-only", "environments":[], "baseInstructions":request.system, "dynamicTools":dynamic})).await?;
        let thread_id = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(failure)?;
        let transcript = request.messages.iter().map(|message| {
            json!({"role":message.role.as_str(), "content":message.content,
                "toolCallId":message.tool_call_id, "toolCalls":message.tool_calls.iter().map(|call| json!({"id":call.id,"name":call.name,"arguments":call.arguments})).collect::<Vec<_>>()})
        }).collect::<Vec<_>>();
        let mut params = json!({"threadId":thread_id,"input":[{"type":"text", "text":serde_json::to_string(&transcript).map_err(|_| failure())?}]});
        if let Some(effort) = super::reasoning_effort(request) {
            params["effort"] = json!(effort);
        }
        self.rpc("turn/start", params).await?;
        Ok(())
    }

    async fn next_event(&mut self) -> Result<Event, AppError> {
        loop {
            if self.cancel.is_cancelled() {
                return Err(cancelled());
            }
            let message = match self.inbox.pop_front() {
                Some(message) => message,
                None => self.read().await?,
            };
            let params = &message["params"];
            match message.get("method").and_then(Value::as_str) {
                Some("item/agentMessage/delta") => {
                    return Ok(Event::Delta(TextDelta::text(
                        params["delta"].as_str().unwrap_or_default(),
                    )));
                }
                Some("item/reasoning/summaryTextDelta" | "item/reasoning/textDelta") => {
                    return Ok(Event::Delta(TextDelta::reasoning(
                        params["delta"].as_str().unwrap_or_default(),
                    )));
                }
                Some("item/tool/call") => {
                    let name = params
                        .get("tool")
                        .and_then(Value::as_str)
                        .ok_or_else(failure)?;
                    if !self.allowed_tools.contains(name)
                        || params.get("namespace").and_then(Value::as_str) != Some("zvs")
                    {
                        return Err(failure());
                    }
                    let call = ToolCall::new(
                        params
                            .get("callId")
                            .and_then(Value::as_str)
                            .ok_or_else(failure)?,
                        name,
                        match params["arguments"].as_str() {
                            Some(arguments) => arguments.to_owned(),
                            None => serde_json::to_string(&params["arguments"])
                                .map_err(|_| failure())?,
                        },
                    );
                    return Ok(Event::Delta(TextDelta::ToolCall(call)));
                }
                Some("turn/completed") => {
                    if params.pointer("/turn/status").and_then(Value::as_str) != Some("completed") {
                        return Err(stream_error(params));
                    }
                    return Ok(Event::Completed(self.last_usage));
                }
                Some("thread/tokenUsage/updated") => {
                    let total = params.pointer("/tokenUsage/last").unwrap_or(&Value::Null);
                    self.last_usage = Some(TokenUsage {
                        prompt_tokens: total.get("inputTokens").and_then(Value::as_i64),
                        completion_tokens: total.get("outputTokens").and_then(Value::as_i64),
                        total_tokens: total.get("totalTokens").and_then(Value::as_i64),
                    });
                }
                Some("error") => return Err(stream_error(params)),
                _ => self.reject_request(&message).await?,
            }
        }
    }
}

pub fn events(
    transport: SharedTransport,
    request: GenerateRequest,
    cancel: CancellationToken,
) -> Events {
    let client = futures::stream::once(async move {
        let mut client = Client::start(transport, cancel).await?;
        client.turn(&request).await?;
        Ok::<_, AppError>(client)
    });
    client
        .flat_map(|client| -> BoxStream<'static, Result<Event, AppError>> {
            match client {
                Err(error) => futures::stream::once(async move { Err(error) }).boxed(),
                Ok(client) => {
                    futures::stream::unfold((client, false), |(mut client, ended)| async move {
                        if ended {
                            return None;
                        }
                        let event = client.next_event().await;
                        let ended = matches!(
                            &event,
                            Ok(Event::Completed(_) | Event::Delta(TextDelta::ToolCall(_))) | Err(_)
                        );
                        if ended {
                            let _ = client.child.kill().await;
                        }
                        Some((event, (client, ended)))
                    })
                    .boxed()
                }
            }
        })
        .boxed()
}

pub fn usage_windows(value: &Value) -> Vec<ProviderUsageWindowDto> {
    let mut windows = Vec::new();
    let buckets = value
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
        .filter(|buckets| !buckets.is_empty())
        .map(|buckets| buckets.values().collect::<Vec<_>>())
        .unwrap_or_else(|| value.get("rateLimits").into_iter().collect());
    for bucket in buckets {
        let name = bucket
            .get("limitName")
            .and_then(Value::as_str)
            .or_else(|| bucket.get("limitId").and_then(Value::as_str))
            .unwrap_or("Codex");
        for (key, label) in [
            ("primary", "Основной лимит"),
            ("secondary", "Дополнительный лимит"),
        ] {
            let window = &bucket[key];
            if let Some(used) = window
                .get("usedPercent")
                .and_then(Value::as_f64)
                .filter(|used| used.is_finite() && (0.0..=100.0).contains(used))
            {
                windows.push(ProviderUsageWindowDto {
                    label: format!("{name} · {label}"),
                    used_percent: used,
                    resets_at: window
                        .get("resetsAt")
                        .and_then(Value::as_i64)
                        .and_then(|seconds| seconds.checked_mul(1000)),
                });
            }
        }
    }
    windows
}

pub async fn usage(
    transport: SharedTransport,
    cancel: &CancellationToken,
) -> Result<Vec<ProviderUsageWindowDto>, AppError> {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut client = Client::start(transport, cancel.clone()).await?;
        let result = client.rpc("account/rateLimits/read", json!({})).await;
        let _ = client.child.kill().await;
        result.map(|value| usage_windows(&value))
    })
    .await
    .map_err(|_| failure())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::transport::{ByteStream, RequestSpec, Transport, TransportResponse};

    struct TestTransport;

    #[async_trait::async_trait]
    impl Transport for TestTransport {
        fn base_url(&self) -> &str {
            zvs_openai::API_BASE
        }
        async fn oauth_token(&self, _cancel: &CancellationToken) -> Result<String, AppError> {
            Ok("fake-oauth-token".to_owned())
        }
        async fn request(
            &self,
            _spec: &RequestSpec,
            _cancel: &CancellationToken,
        ) -> Result<TransportResponse, AppError> {
            Err(failure())
        }
        async fn stream(
            &self,
            _spec: &RequestSpec,
            _cancel: &CancellationToken,
        ) -> Result<ByteStream, AppError> {
            Err(failure())
        }
    }

    async fn fake_client() -> Client {
        let mut command = Command::new("node");
        command.arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/helpers/fakeCodexServer.mjs"),
        );
        Client::start_command(
            std::sync::Arc::new(TestTransport),
            CancellationToken::new(),
            command,
        )
        .await
        .expect("fake server")
    }

    #[tokio::test]
    #[ignore = "requires an installed Codex CLI; performs no inference"]
    async fn installed_codex_handshake() {
        let mut client = tokio::time::timeout(
            Duration::from_secs(20),
            Client::start(std::sync::Arc::new(TestTransport), CancellationToken::new()),
        )
        .await
        .expect("startup deadline")
        .expect("Codex handshake");
        client.rpc("thread/start", json!({"ephemeral":true,"approvalPolicy":"untrusted","sandbox":"read-only","environments":[],"dynamicTools":[{"type":"namespace","name":"zvs","description":"ZVS tools","tools":[{"type":"function","name":"lookup","description":"Lookup","inputSchema":{"type":"object","properties":{}}}]}]})).await.expect("thread configuration");
        client.child.kill().await.expect("stop");
    }

    #[tokio::test]
    async fn rpc_keeps_deltas_before_acknowledgement_and_reads_terminal_usage() {
        let mut client = fake_client().await;
        client
            .turn(&GenerateRequest::new("text", Vec::new()))
            .await
            .expect("turn");
        assert!(
            matches!(client.next_event().await.expect("delta"), Event::Delta(TextDelta::Content { text, .. }) if text == "before acknowledgement")
        );
        assert!(matches!(
            client.next_event().await.expect("completed"),
            Event::Completed(Some(TokenUsage {
                total_tokens: Some(5),
                ..
            }))
        ));
        client.child.kill().await.expect("stop");
    }

    #[tokio::test]
    async fn turns_send_selected_effort_and_omit_the_default_override() {
        for (effort, model) in [
            (None, "effort-default"),
            (Some(zvs_contract::ThinkingEffort::Auto), "effort-default"),
            (Some(zvs_contract::ThinkingEffort::Fast), "effort-low"),
            (Some(zvs_contract::ThinkingEffort::Thinking), "effort-high"),
        ] {
            let mut client = fake_client().await;
            let mut request = GenerateRequest::new(model, Vec::new());
            request.vendor.effort = effort;
            client
                .turn(&request)
                .await
                .expect("effort accepted by server");
            client.child.kill().await.expect("stop");
        }
    }

    #[tokio::test]
    async fn tool_handoff_only_accepts_offered_tools_and_failures_are_errors() {
        for model in ["tool", "unoffered", "failure"] {
            let mut client = fake_client().await;
            let mut request = GenerateRequest::new(model, Vec::new());
            request.tools.push(crate::drivers::ai::ports::ToolSpec {
                name: "lookup".to_owned(),
                description: "Lookup".to_owned(),
                parameters: json!({"type":"object"}),
            });
            client.turn(&request).await.expect("turn");
            client.next_event().await.expect("first delta");
            let result = client.next_event().await;
            if model == "tool" {
                assert!(
                    matches!(result, Ok(Event::Delta(TextDelta::ToolCall(ToolCall { name, .. }))) if name == "lookup")
                );
            } else {
                assert!(result.is_err());
            }
            client.child.kill().await.expect("stop");
        }
    }

    #[tokio::test]
    async fn cancelling_an_app_server_wait_finishes_the_request() {
        let mut client = fake_client().await;
        client.cancel.cancel();
        assert!(client.next_event().await.is_err());
        client.child.kill().await.expect("stop");
    }

    #[test]
    fn limits_are_provider_values_with_millisecond_reset_times() {
        let windows = usage_windows(
            &json!({"rateLimitsByLimitId":{"codex":{"limitId":"codex","primary":{"usedPercent":42,"resetsAt":2000000000},"secondary":null}}}),
        );
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].used_percent, 42.0);
        assert_eq!(windows[0].resets_at, Some(2_000_000_000_000));
        assert!(usage_windows(&json!({"rateLimits":{"primary":{"usedPercent":-1}}})).is_empty());
        assert!(usage_windows(&json!({})).is_empty());
    }
}
