use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_slice;
use zvs_contract::{AppError, AppErrorCode};

use crate::drivers::ai::HttpClient;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const DEFAULT_IDLE: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(180);
pub const HEALTH_INTERVAL: Duration = Duration::from_millis(250);
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
pub const DEFAULT_CONTEXT_SIZE: u32 = 8192;
pub const DEFAULT_BATCH_SIZE: u32 = 2048;
pub const STDERR_TAIL: usize = 4000;
pub const EXITED_AT_START: &str = "Локальный движок завершился при запуске";
pub const NO_ANSWER_AT_START: &str = "Локальный движок не ответил при запуске";
pub const START_CANCELLED: &str = "Запуск движка отменён";
pub const UNEXPECTED_ANSWER: &str = "Движок вернул неожиданный ответ";
pub const EMPTY_VECTOR: &str = "Движок вернул пустой вектор";
pub const UNEXPECTED_SCORE: &str = "Движок вернул неожиданную оценку";
pub const MISSING_SCORES: &str = "Движок оценил не все фрагменты";
pub const EMPTY_ANSWER: &str = "Движок вернул пустой ответ";
pub const UNKNOWN_ANSWER: &str = "Движок вернул ответ неизвестного вида";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlamaServerRole {
    Embedding,
    Vision,
    Rerank,
}

impl LlamaServerRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::Vision => "vision",
            Self::Rerank => "rerank",
        }
    }
}

pub type Launcher = Arc<dyn Fn(&Path, &[String]) -> Command + Send + Sync>;
pub type PortSource = Arc<dyn Fn() -> Result<u16, AppError> + Send + Sync>;

#[derive(Clone)]
pub struct LlamaServerOptions {
    pub executable_path: PathBuf,
    pub model_path: String,
    pub role: LlamaServerRole,
    pub mmproj_path: Option<String>,
    pub gpu_layers: u32,
    pub context_size: Option<u32>,
    pub batch_size: Option<u32>,
    pub threads: Option<u32>,
    pub idle: Duration,
    pub start_timeout: Duration,
    pub health_interval: Duration,
    pub http: HttpClient,
    pub logger: SharedLogger,
    pub launcher: Option<Launcher>,
    pub free_port: Option<PortSource>,
}

impl LlamaServerOptions {
    pub fn new(
        executable_path: PathBuf,
        model_path: impl Into<String>,
        role: LlamaServerRole,
        gpu_layers: u32,
        http: HttpClient,
        logger: SharedLogger,
    ) -> Self {
        Self {
            executable_path,
            model_path: model_path.into(),
            role,
            mmproj_path: None,
            gpu_layers,
            context_size: None,
            batch_size: None,
            threads: None,
            idle: DEFAULT_IDLE,
            start_timeout: DEFAULT_START_TIMEOUT,
            health_interval: HEALTH_INTERVAL,
            http,
            logger,
            launcher: None,
            free_port: None,
        }
    }

    pub fn argv(&self, port: u16, api_key: &str) -> Vec<String> {
        let batch = self.batch_size.unwrap_or(DEFAULT_BATCH_SIZE).to_string();
        let mut argv = vec!["--model".to_owned(), self.model_path.clone()];
        match self.role {
            LlamaServerRole::Embedding => argv.push("--embeddings".to_owned()),
            LlamaServerRole::Vision => {
                argv.push("--mmproj".to_owned());
                argv.push(self.mmproj_path.clone().unwrap_or_default());
            }
            LlamaServerRole::Rerank => argv.push("--reranking".to_owned()),
        }
        argv.extend([
            "--host".to_owned(),
            "127.0.0.1".to_owned(),
            "--port".to_owned(),
            port.to_string(),
            "--api-key".to_owned(),
            api_key.to_owned(),
            "--n-gpu-layers".to_owned(),
            self.gpu_layers.to_string(),
            "--ctx-size".to_owned(),
            self.context_size
                .unwrap_or(DEFAULT_CONTEXT_SIZE)
                .to_string(),
            "--batch-size".to_owned(),
            batch.clone(),
        ]);
        if self.role != LlamaServerRole::Vision {
            argv.push("--ubatch-size".to_owned());
            argv.push(batch);
        }
        if let Some(threads) = self.threads {
            argv.push("--threads".to_owned());
            argv.push(threads.to_string());
        }
        argv
    }
}

pub fn free_loopback_port() -> Result<u16, AppError> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| AppError::new(AppErrorCode::Unknown, "Could not reserve a port"))
}

fn api_key() -> String {
    let mut key = uuid::Uuid::new_v4().simple().to_string();
    key.push_str(&uuid::Uuid::new_v4().simple().to_string());
    key.truncate(48);
    key
}

fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(max)).collect()
}

fn cancelled_start() -> AppError {
    AppError::new(AppErrorCode::RunCancelled, START_CANCELLED)
}

pub fn cancelled_call() -> AppError {
    AppError::new(
        AppErrorCode::RunCancelled,
        crate::kernel::types::CANCELLED_MESSAGE,
    )
}

type Exit = Option<Option<i32>>;
type Ready = Shared<BoxFuture<'static, Result<String, AppError>>>;

struct Child {
    generation: u64,
    kill: CancellationToken,
    exited: watch::Receiver<Exit>,
}

#[derive(Default)]
struct State {
    child: Option<Child>,
    ready: Option<(u64, Ready)>,
    base_url: Option<String>,
    api_key: String,
    stderr: String,
    stopped: bool,
    generation: u64,
    attempt: u64,
    launching: Option<u64>,
    idle: Option<CancellationToken>,
    stopping: Vec<watch::Receiver<Exit>>,
}

struct Inner {
    options: LlamaServerOptions,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct LlamaServer {
    inner: Arc<Inner>,
}

#[async_trait]
pub trait EngineServer: Send + Sync {
    fn running(&self) -> bool;

    fn model_path(&self) -> String;

    fn role(&self) -> LlamaServerRole;

    async fn embed(
        &self,
        texts: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError>;

    async fn rerank(
        &self,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError>;

    async fn describe_image(
        &self,
        media_type: &str,
        bytes: &[u8],
        prompt: &str,
        cancel: &CancellationToken,
    ) -> Result<String, AppError>;

    fn stop(&self);

    async fn stopped(&self);
}

pub type SharedEngineServer = Arc<dyn EngineServer>;

impl LlamaServer {
    pub fn new(options: LlamaServerOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                state: Mutex::new(State::default()),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.inner
            .options
            .logger
            .log(level, "runtimes", message, fields);
    }

    pub fn is_running(&self) -> bool {
        Self::child_running(&self.state())
    }

    fn child_running(state: &State) -> bool {
        state
            .child
            .as_ref()
            .is_some_and(|child| child.exited.borrow().is_none())
    }

    pub fn base_url(&self) -> Option<String> {
        self.state().base_url.clone()
    }

    pub fn stderr(&self) -> String {
        self.state().stderr.clone()
    }

    pub async fn start(&self, cancel: Option<CancellationToken>) -> Result<String, AppError> {
        let ready = {
            let mut state = self.state();
            let joinable = state.ready.as_ref().filter(|(owner, _)| {
                Self::child_running(&state) || state.launching == Some(*owner)
            });
            match joinable {
                Some((_, ready)) => ready.clone(),
                None => {
                    state.stopped = false;
                    state.attempt += 1;
                    let attempt = state.attempt;
                    state.launching = Some(attempt);
                    let server = self.clone();
                    let task = tokio::spawn(async move {
                        let outcome = server.launch(cancel).await;
                        {
                            let mut state = server.state();
                            if state.launching == Some(attempt) {
                                state.launching = None;
                            }
                        }
                        if outcome.is_err() {
                            server.abandon(attempt);
                        }
                        outcome
                    });
                    let ready: Ready = async move {
                        task.await.unwrap_or_else(|error| {
                            Err(AppError::new(AppErrorCode::NativeError, error.to_string()))
                        })
                    }
                    .boxed()
                    .shared();
                    state.ready = Some((attempt, ready.clone()));
                    ready
                }
            }
        };
        ready.await
    }

    fn abandon(&self, attempt: u64) {
        let current = self
            .state()
            .ready
            .as_ref()
            .is_some_and(|(owner, _)| *owner == attempt);
        if current {
            self.state().ready = None;
            self.stop();
        }
    }

    pub fn stop(&self) {
        let mut state = self.state();
        state.stopped = true;
        if let Some(idle) = state.idle.take() {
            idle.cancel();
        }
        state.ready = None;
        state.base_url = None;
        if let Some(child) = state.child.take() {
            child.kill.cancel();
            state.stopping.push(child.exited);
        }
    }

    pub async fn stopped(&self) {
        let stopping = std::mem::take(&mut self.state().stopping);
        for mut exited in stopping {
            let _ = exited.wait_for(Option::is_some).await;
        }
    }

    fn command(&self, port: u16, api_key: &str) -> Command {
        let options = &self.inner.options;
        let argv = options.argv(port, api_key);
        let mut command = match &options.launcher {
            Some(launcher) => launcher(&options.executable_path, &argv),
            None => {
                let mut command = Command::new(&options.executable_path);
                command.args(&argv);
                command
            }
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }

    async fn launch(&self, cancel: Option<CancellationToken>) -> Result<String, AppError> {
        let options = &self.inner.options;
        let port = match &options.free_port {
            Some(source) => source()?,
            None => free_loopback_port()?,
        };
        let key = api_key();
        {
            let mut state = self.state();
            state.api_key.clone_from(&key);
            state.stderr.clear();
        }
        self.log(
            LogLevel::Info,
            "Starting a local engine server",
            &[
                ("role", json!(options.role.as_str())),
                (
                    "executable",
                    json!(options.executable_path.to_string_lossy()),
                ),
                ("model", json!(options.model_path)),
                ("gpuLayers", json!(options.gpu_layers)),
                ("port", json!(port)),
            ],
        );
        let mut child = self.command(port, &key).spawn().map_err(|error| {
            AppError::new(AppErrorCode::Unknown, EXITED_AT_START)
                .with_detail("exitCode", Value::Null)
                .with_detail("stderr", error.to_string())
        })?;
        let (sender, exited) = watch::channel::<Exit>(None);
        let kill = CancellationToken::new();
        let generation = {
            let mut state = self.state();
            state.generation += 1;
            let generation = state.generation;
            state.child = Some(Child {
                generation,
                kill: kill.clone(),
                exited: exited.clone(),
            });
            generation
        };
        if let Some(mut stderr) = child.stderr.take() {
            let server = self.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0_u8; 4096];
                while let Ok(read) = stderr.read(&mut buffer).await {
                    if read == 0 {
                        break;
                    }
                    let mut state = server.state();
                    let joined = format!(
                        "{}{}",
                        state.stderr,
                        String::from_utf8_lossy(&buffer[..read])
                    );
                    state.stderr = tail(&joined, STDERR_TAIL);
                }
            });
        }
        let server = self.clone();
        tokio::spawn(async move {
            let code = tokio::select! {
                status = child.wait() => status.ok().and_then(|status| status.code()),
                () = kill.cancelled() => {
                    let _ = child.start_kill();
                    child.wait().await.ok().and_then(|status| status.code())
                }
            };
            let _ = sender.send(Some(code));
            server.exited(generation, code);
        });

        let base_url = format!("http://127.0.0.1:{port}");
        self.wait_for_health(&base_url, &exited, cancel.as_ref())
            .await?;
        self.state().base_url = Some(base_url.clone());
        self.touch();
        Ok(base_url)
    }

    fn exited(&self, generation: u64, code: Option<i32>) {
        let unexpected = {
            let mut state = self.state();
            let current = state
                .child
                .as_ref()
                .is_some_and(|child| child.generation == generation);
            if current {
                state.child = None;
                state.ready = None;
                state.base_url = None;
            }
            current.then(|| tail(&state.stderr, 1000))
        };
        if let Some(stderr) = unexpected {
            self.log(
                LogLevel::Warn,
                "A local engine server exited",
                &[
                    ("role", json!(self.inner.options.role.as_str())),
                    ("model", json!(self.inner.options.model_path)),
                    ("exitCode", json!(code)),
                    ("stderr", json!(stderr)),
                ],
            );
        }
    }

    async fn wait_for_health(
        &self,
        base_url: &str,
        exited: &watch::Receiver<Exit>,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), AppError> {
        let options = &self.inner.options;
        let deadline = Instant::now() + options.start_timeout;
        let never = CancellationToken::new();
        let cancel = cancel.unwrap_or(&never);
        loop {
            if cancel.is_cancelled() {
                return Err(cancelled_start());
            }
            let (stopped, key, stderr) = {
                let state = self.state();
                (state.stopped, state.api_key.clone(), state.stderr.clone())
            };
            let exit = *exited.borrow();
            if exit.is_some() || stopped {
                return Err(AppError::new(AppErrorCode::Unknown, EXITED_AT_START)
                    .with_detail("exitCode", json!(exit.flatten()))
                    .with_detail("stderr", tail(&stderr, 1000)));
            }
            let healthy = options
                .http
                .client()
                .get(format!("{base_url}/health"))
                .bearer_auth(&key)
                .timeout(HEALTH_TIMEOUT)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success());
            if healthy {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err(
                    AppError::new(AppErrorCode::ProviderUnreachable, NO_ANSWER_AT_START)
                        .with_detail("baseUrl", base_url)
                        .with_detail("stderr", tail(&stderr, 1000)),
                );
            }
            tokio::select! {
                () = tokio::time::sleep(options.health_interval) => {}
                () = cancel.cancelled() => return Err(cancelled_start()),
            }
        }
    }

    fn touch(&self) {
        let idle = self.inner.options.idle;
        if idle.is_zero() {
            return;
        }
        let token = CancellationToken::new();
        if let Some(previous) = self.state().idle.replace(token.clone()) {
            previous.cancel();
        }
        let server = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(idle) => {
                    let base_url = server.base_url();
                    server.log(
                        LogLevel::Info,
                        "Stopping an idle engine server",
                        &[
                            ("role", json!(server.inner.options.role.as_str())),
                            ("model", json!(server.inner.options.model_path)),
                            ("baseUrl", json!(base_url)),
                        ],
                    );
                    server.stop();
                }
                () = token.cancelled() => {}
            }
        });
    }

    async fn post(
        &self,
        route: &str,
        body: &Value,
        what: &str,
        cancel: &CancellationToken,
        start_with_signal: bool,
    ) -> Result<Value, AppError> {
        let base_url = if start_with_signal {
            self.start(Some(cancel.clone())).await?
        } else {
            tokio::select! {
                started = self.start(None) => started?,
                () = cancel.cancelled() => return Err(cancelled_call()),
            }
        };
        self.touch();
        let key = self.state().api_key.clone();
        let request = self
            .inner
            .options
            .http
            .client()
            .post(format!("{base_url}{route}"))
            .bearer_auth(key)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send();
        let response = tokio::select! {
            response = request => response,
            () = cancel.cancelled() => return Err(cancelled_call()),
        };
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                if !cancel.is_cancelled() {
                    self.stop();
                }
                return Err(AppError::new(
                    AppErrorCode::ProviderUnreachable,
                    error.to_string(),
                ));
            }
        };
        let status = response.status().as_u16();
        let read = tokio::select! {
            bytes = response.bytes() => bytes,
            () = cancel.cancelled() => return Err(cancelled_call()),
        };
        if !(200..300).contains(&status) {
            let detail = read
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            return Err(AppError::new(
                AppErrorCode::Unknown,
                format!("Локальный движок не смог {what}"),
            )
            .with_detail("status", status)
            .with_detail("detail", js_slice(&detail, 500).to_owned()));
        }
        let bytes = read
            .map_err(|error| AppError::new(AppErrorCode::ProviderUnreachable, error.to_string()))?;
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, UNEXPECTED_ANSWER))
    }
}

pub fn read_embeddings(payload: &Value) -> Result<Vec<Vec<f32>>, AppError> {
    let Some(data) = payload.get("data").and_then(Value::as_array) else {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            UNEXPECTED_ANSWER,
        ));
    };
    data.iter()
        .map(|entry| {
            entry
                .get("embedding")
                .and_then(Value::as_array)
                .and_then(|values| {
                    values
                        .iter()
                        .map(|value| value.as_f64().map(|number| number as f32))
                        .collect::<Option<Vec<f32>>>()
                })
                .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, EMPTY_VECTOR))
        })
        .collect()
}

pub fn read_rerank_scores(payload: &Value, count: usize) -> Result<Vec<f64>, AppError> {
    let Some(results) = payload.get("results").and_then(Value::as_array) else {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            UNEXPECTED_ANSWER,
        ));
    };
    let mut scores = vec![f64::NAN; count];
    for entry in results {
        let index = entry
            .get("index")
            .and_then(Value::as_f64)
            .filter(|index| index.fract() == 0.0 && *index >= 0.0 && *index < count as f64);
        let score = entry
            .get("relevance_score")
            .and_then(Value::as_f64)
            .filter(|score| score.is_finite());
        let (Some(index), Some(score)) = (index, score) else {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                UNEXPECTED_SCORE,
            ));
        };
        scores[index as usize] = score;
    }
    if scores.iter().any(|score| score.is_nan()) {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            MISSING_SCORES,
        ));
    }
    Ok(scores)
}

pub fn read_chat_text(payload: &Value) -> Result<String, AppError> {
    let Some(first) = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return Err(AppError::new(AppErrorCode::ValidationFailed, EMPTY_ANSWER));
    };
    match first
        .get("message")
        .and_then(|message| message.get("content"))
    {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(parts)) => Ok(parts
            .iter()
            .map(|part| part.get("text").and_then(Value::as_str).unwrap_or(""))
            .collect()),
        _ => Err(AppError::new(
            AppErrorCode::ValidationFailed,
            UNKNOWN_ANSWER,
        )),
    }
}

#[async_trait]
impl EngineServer for LlamaServer {
    fn running(&self) -> bool {
        self.is_running()
    }

    fn model_path(&self) -> String {
        self.inner.options.model_path.clone()
    }

    fn role(&self) -> LlamaServerRole {
        self.inner.options.role
    }

    async fn embed(
        &self,
        texts: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let payload = self
            .post(
                "/v1/embeddings",
                &json!({ "input": texts, "model": self.inner.options.model_path }),
                "посчитать эмбеддинги",
                cancel,
                true,
            )
            .await?;
        read_embeddings(&payload)
    }

    async fn rerank(
        &self,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }
        let payload = self
            .post(
                "/v1/rerank",
                &json!({
                    "model": self.inner.options.model_path,
                    "query": query,
                    "documents": passages,
                    "top_n": passages.len(),
                }),
                "переранжировать результаты",
                cancel,
                false,
            )
            .await?;
        read_rerank_scores(&payload, passages.len())
    }

    async fn describe_image(
        &self,
        media_type: &str,
        bytes: &[u8],
        prompt: &str,
        cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        let data_uri = format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        let payload = self
            .post(
                "/v1/chat/completions",
                &json!({
                    "model": self.inner.options.model_path,
                    "temperature": 0,
                    "max_tokens": 2048,
                    "messages": [{
                        "role": "user",
                        "content": [
                            { "type": "text", "text": prompt },
                            { "type": "image_url", "image_url": { "url": data_uri } },
                        ],
                    }],
                }),
                "распознать страницу",
                cancel,
                true,
            )
            .await?;
        read_chat_text(&payload)
    }

    fn stop(&self) {
        Self::stop(self);
    }

    async fn stopped(&self) {
        Self::stopped(self).await;
    }
}

#[cfg(test)]
mod tests;
