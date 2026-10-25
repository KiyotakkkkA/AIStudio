use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::mem;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AdapterFamily, AppError, AppErrorCode, AuthMode, ModelCapability, ToolMode};
use zvs_qwen_web::protocol::{self, chat_referer_path};
use zvs_qwen_web::{
    CompletionTurn, ModelsDiagnostics, QwenModel, RowProblem, ThinkingMode, TurnDecoder, TurnEvent,
    VendorFailure, new_chat_body, parse_chat_created, parse_models,
};

use dialect::QwenDialect;

use super::account_family::{
    AccountAdapterCore, AccountAdapterOptions, MALFORMED_MODELS_MESSAGE, flatten_prompt,
    malformed_turn, response_diagnostics, truncated_stream,
};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::{
    html_title, is_cancellation, is_session_expired, request_failed, session_expired,
};
use crate::drivers::ai::ports::{
    ChatMessage, ChatRole, DeltaStream, DiscoveredModel, FinishReason, GenerateRequest,
    GenerateResult, SharedTextDriver, TextDelta, TextGenerationDriver, TokenUsage,
};
use crate::drivers::ai::sse::FrameStream;
use crate::drivers::ai::tool_dialect::with_tool_dialect;
use crate::drivers::ai::transport::{RequestSpec, TransportResponse};
use crate::platform::logger::{LogLevel, SharedLogger};

pub mod dialect;

pub use zvs_qwen_web::protocol::{
    API_VERSION as QWEN_API_VERSION, CHAT_MODE as QWEN_CHAT_MODE, CHAT_TYPE as QWEN_CHAT_TYPE,
    CHATS_NEW_PATH as QWEN_CHATS_NEW_PATH, COMPLETIONS_PATH as QWEN_COMPLETIONS_PATH,
    DEFAULT_TIMEOUT_SECONDS as QWEN_DEFAULT_TIMEOUT_SECONDS,
    GENERATION_OBSERVATION as QWEN_GENERATION_OBSERVATION,
    MODELS_OBSERVATION as QWEN_MODELS_OBSERVATION, MODELS_PATH as QWEN_MODELS_PATH,
    WEB_VERSION as QWEN_WEB_VERSION,
};

pub const GENERATION_ABORTED_MESSAGE: &str = "Вендор прервал генерацию";
pub const MODELS_REFUSED_MESSAGE: &str = "Qwen отклонил запрос списка моделей";
pub const CHAINS_MAX: usize = 256;
pub const MODELS_TTL_MS: i64 = 60 * 60 * 1000;

pub const QWEN_WEB_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::QwenWeb,
    auth_modes: &[AuthMode::Account],
    streaming: true,
    live_model_list: true,
    embedding: false,
    image: false,
    honours: Honours::NONE,
    tools: ToolMode::Injected,
};

const FAMILY: AdapterFamily = AdapterFamily::QwenWeb;

fn to_discovered_model(model: QwenModel) -> DiscoveredModel {
    let mut capabilities = vec![ModelCapability::Streaming];
    if model.thinking {
        capabilities.push(ModelCapability::Reasoning);
    }
    DiscoveredModel {
        display_name: model.display_name,
        family: model.owned_by,
        context_window: model.context_window,
        max_output: model.max_output,
        capabilities,
        ..DiscoveredModel::named(model.id)
    }
}

/// `None` also when every row was malformed, as discovery treats that list as unusable.
pub fn map_qwen_models(payload: &Value) -> Option<Vec<DiscoveredModel>> {
    let list = parse_models(payload)?;
    if list.models.is_empty() && list.skipped > 0 {
        return None;
    }
    Some(list.models.into_iter().map(to_discovered_model).collect())
}

fn vendor_failure(failure: VendorFailure) -> AppError {
    let mut details = zvs_contract::ErrorDetails::new();
    details.insert("family".to_owned(), json!(FAMILY.as_str()));
    details.insert("endpoint".to_owned(), json!(QWEN_COMPLETIONS_PATH));
    if let Some(code) = failure.code {
        details.insert("vendorCode".to_owned(), code);
    }
    if let Some(status) = failure.status {
        details.insert("status".to_owned(), json!(status));
    }
    if failure.auth {
        return session_expired(details);
    }
    AppError::new(AppErrorCode::Unknown, GENERATION_ABORTED_MESSAGE).with_details(details)
}

fn row_problems(problems: &[RowProblem]) -> Value {
    problems
        .iter()
        .map(|problem| {
            json!({
                "row": problem.index,
                "field": problem.field,
                "expected": problem.expected,
                "found": problem.found,
            })
        })
        .collect()
}

fn malformed_models_response(response: &TransportResponse, logger: &SharedLogger) -> AppError {
    let shape = ModelsDiagnostics::inspect(&response.text);
    let content_type = response
        .headers
        .get("content-type")
        .cloned()
        .unwrap_or_default();
    // An explicit refusal with a code is the vendor's answer, not an unknown format.
    let (code, message) = if shape.outer_success == "false" && shape.vendor_code.is_some() {
        (AppErrorCode::ProviderUnreachable, MODELS_REFUSED_MESSAGE)
    } else {
        (AppErrorCode::Unknown, MALFORMED_MODELS_MESSAGE)
    };
    logger.log(
        LogLevel::Warn,
        "ai",
        message,
        &[
            ("family", json!(FAMILY.as_str())),
            ("endpoint", json!(QWEN_MODELS_PATH)),
            ("status", json!(response.status)),
            ("contentType", json!(content_type)),
            ("payloadShape", json!(shape.payload_shape)),
            ("rootFields", json!(shape.root_fields)),
            ("outerSuccess", json!(shape.outer_success)),
            ("dataShape", json!(shape.data_shape)),
            ("dataFields", json!(shape.data_fields)),
            ("modelCount", json!(shape.model_count)),
            ("sampleModelFields", json!(shape.sample_model_fields)),
            ("sampleInfoFields", json!(shape.sample_info_fields)),
            ("sampleMetaFields", json!(shape.sample_meta_fields)),
            ("rowProblems", row_problems(&shape.row_problems)),
            ("htmlTitle", json!(html_title(&response.text))),
            ("dataFieldKinds", json!(shape.data_field_kinds)),
            ("vendorCode", json!(shape.vendor_code)),
        ],
    );
    AppError::new(code, message)
        .with_detail("family", FAMILY.as_str())
        .with_detail("endpoint", QWEN_MODELS_PATH)
        .with_detail("status", response.status)
        .with_detail("contentType", content_type)
        .with_detail("payloadShape", shape.payload_shape)
        .with_detail("rootFields", json!(shape.root_fields))
        .with_detail("outerSuccess", shape.outer_success)
        .with_detail("dataShape", shape.data_shape)
        .with_detail("modelCount", shape.model_count)
        .with_detail("rowProblems", row_problems(&shape.row_problems))
        .with_detail("vendorCode", json!(shape.vendor_code))
}

fn to_usage(raw: zvs_qwen_web::Usage) -> TokenUsage {
    let count = |value: Option<u64>| value.and_then(|value| i64::try_from(value).ok());
    TokenUsage {
        prompt_tokens: count(raw.input_tokens),
        completion_tokens: count(raw.output_tokens),
        total_tokens: count(raw.total_tokens),
    }
}

#[derive(Debug, Default)]
struct TurnSummary {
    usage: Option<TokenUsage>,
    finish_reason: Option<FinishReason>,
}

type SharedSummary = Arc<Mutex<TurnSummary>>;

fn with_summary(summary: &SharedSummary, update: impl FnOnce(&mut TurnSummary)) {
    update(&mut summary.lock().unwrap_or_else(PoisonError::into_inner));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Chain {
    chat_id: String,
    response_id: String,
}

#[derive(Debug, Default)]
struct Chains {
    order: VecDeque<u64>,
    by_key: HashMap<u64, Chain>,
}

impl Chains {
    fn insert(&mut self, key: u64, chain: Chain) {
        if self.by_key.insert(key, chain).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > CHAINS_MAX {
            if let Some(oldest) = self.order.pop_front() {
                self.by_key.remove(&oldest);
            }
        }
    }

    fn forget_chat(&mut self, chat_id: &str) {
        self.by_key.retain(|_, chain| chain.chat_id != chat_id);
        let by_key = &self.by_key;
        self.order.retain(|key| by_key.contains_key(key));
    }

    fn clear(&mut self) {
        self.order.clear();
        self.by_key.clear();
    }
}

fn history_key(model: &str, system: Option<&str>, messages: &[ChatMessage]) -> u64 {
    let mut hasher = DefaultHasher::new();
    model.hash(&mut hasher);
    system.map(str::trim).unwrap_or_default().hash(&mut hasher);
    for message in messages {
        message.role.hash(&mut hasher);
        message.content.hash(&mut hasher);
        message.tool_call_id.hash(&mut hasher);
        format!("{:?}", message.tool_calls).hash(&mut hasher);
    }
    hasher.finish()
}

fn request_key(request: &GenerateRequest) -> u64 {
    history_key(&request.model, request.system.as_deref(), &request.messages)
}

struct Opened {
    frames: FrameStream,
    chat_id: String,
    chained: bool,
}

fn retryable_on_new_chat(error: &AppError) -> bool {
    !is_cancellation(error)
        && !is_session_expired(error)
        && !matches!(
            error.code,
            AppErrorCode::ProviderUnreachable | AppErrorCode::RateLimited
        )
}

struct CachedModels {
    models: Vec<DiscoveredModel>,
    fetched_at: i64,
}

struct Shared {
    core: AccountAdapterCore,
    models: Mutex<Option<CachedModels>>,
    creating: tokio::sync::Mutex<()>,
    chains: Mutex<Chains>,
}

impl Shared {
    fn web_request(&self, spec: RequestSpec, referer_path: &str) -> Result<RequestSpec, AppError> {
        let mut referer =
            Url::parse(self.core.transport.base_url()).map_err(|_| request_failed())?;
        referer.set_path(referer_path);
        referer.set_query(None);
        referer.set_fragment(None);
        Ok(spec
            .with_header("source", protocol::SOURCE)
            .with_header("Version", QWEN_WEB_VERSION)
            .with_header("x-request-id", (self.core.new_id)())
            .with_header("referer", referer.to_string()))
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let response = self
            .core
            .request(
                &self.web_request(
                    RequestSpec::get(QWEN_MODELS_PATH),
                    protocol::MODELS_REFERER_PATH,
                )?,
                cancel,
            )
            .await?;
        let payload = serde_json::from_str::<Value>(&response.text)
            .map_err(|_| malformed_models_response(&response, &self.core.logger))?;
        let list = parse_models(&payload)
            .ok_or_else(|| malformed_models_response(&response, &self.core.logger))?;
        if list.models.is_empty() && list.skipped > 0 {
            return Err(malformed_models_response(&response, &self.core.logger));
        }
        if list.skipped > 0 {
            self.core.logger.log(
                LogLevel::Warn,
                "ai",
                "Qwen returned model entries with an unsupported schema",
                &[
                    ("family", json!(FAMILY.as_str())),
                    ("endpoint", json!(QWEN_MODELS_PATH)),
                    ("status", json!(response.status)),
                    ("modelCount", json!(list.models.len())),
                    ("skippedCount", json!(list.skipped)),
                    ("rowProblems", row_problems(&list.problems)),
                ],
            );
        }
        let models = list
            .models
            .into_iter()
            .map(to_discovered_model)
            .collect::<Vec<_>>();
        *self.models.lock().unwrap_or_else(PoisonError::into_inner) = Some(CachedModels {
            models: models.clone(),
            fetched_at: self.core.clock.now(),
        });
        Ok(models)
    }

    async fn models(&self, cancel: &CancellationToken) -> Result<Vec<DiscoveredModel>, AppError> {
        let now = self.core.clock.now();
        let cached = self
            .models
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|cached| {
                (
                    cached.models.clone(),
                    now - cached.fetched_at < MODELS_TTL_MS,
                )
            });
        match cached {
            Some((models, true)) => Ok(models),
            Some((stale, false)) => match self.list_models(cancel).await {
                Ok(models) => Ok(models),
                Err(error) if is_cancellation(&error) || is_session_expired(&error) => Err(error),
                Err(_) => Ok(stale),
            },
            None => self.list_models(cancel).await,
        }
    }

    fn continuation(&self, request: &GenerateRequest) -> Option<(Chain, String)> {
        let last = request
            .messages
            .iter()
            .rposition(|message| message.role == ChatRole::Assistant)?;
        let tail = request
            .messages
            .get(last + 1..)
            .filter(|tail| !tail.is_empty())?;
        let key = history_key(
            &request.model,
            request.system.as_deref(),
            &request.messages[..last],
        );
        let chain = self
            .chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .by_key
            .get(&key)
            .cloned()?;
        let prompt = flatten_prompt(&GenerateRequest::new(request.model.clone(), tail.to_vec()));
        Some((chain, prompt))
    }

    fn remember(&self, request: &GenerateRequest, chat_id: String, response_id: String) {
        self.chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                request_key(request),
                Chain {
                    chat_id,
                    response_id,
                },
            );
    }

    fn forget_chats(&self) {
        self.chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    fn chained_chats(&self) -> usize {
        self.chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .by_key
            .len()
    }

    fn retire(&self, chat_id: Option<&str>, error: &AppError) {
        if let Some(chat_id) = chat_id {
            self.chains
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .forget_chat(chat_id);
        }
        self.core.logger.log(
            LogLevel::Info,
            "ai",
            "Qwen refused a chained turn; opening a new chat",
            &[
                ("family", json!(FAMILY.as_str())),
                ("code", json!(error.code)),
            ],
        );
    }

    async fn create_chat(
        &self,
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        let _one_at_a_time = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(crate::drivers::ai::errors::cancelled()),
            guard = self.creating.lock() => guard,
        };
        let response = self
            .core
            .request(
                &self.web_request(
                    RequestSpec::post(
                        QWEN_CHATS_NEW_PATH,
                        new_chat_body(model, self.core.clock.now()),
                    ),
                    protocol::NEW_CHAT_REFERER_PATH,
                )?,
                cancel,
            )
            .await?;
        let payload = self.core.decode(&response, QWEN_CHATS_NEW_PATH)?;
        parse_chat_created(&payload).ok_or_else(|| {
            malformed_turn(
                FAMILY,
                QWEN_CHATS_NEW_PATH,
                &self.core.logger,
                response_diagnostics(&response),
            )
        })
    }

    fn turn(
        &self,
        chat_id: &str,
        model: &str,
        prompt: &str,
        thinking: ThinkingMode,
        parent_id: Option<&str>,
    ) -> Result<RequestSpec, AppError> {
        let fid = (self.core.new_id)();
        let answer_fid = (self.core.new_id)();
        let body = CompletionTurn {
            chat_id,
            model,
            prompt,
            thinking,
            parent_id,
            fid: &fid,
            answer_fid: &answer_fid,
            now_ms: self.core.clock.now(),
        }
        .body();
        self.web_request(
            RequestSpec::post(QWEN_COMPLETIONS_PATH, body)
                .with_query("chat_id", chat_id)
                .with_accept("application/json")
                .with_header("x-accel-buffering", "no"),
            &chat_referer_path(chat_id),
        )
    }

    async fn open(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
        fresh: bool,
    ) -> Result<Opened, AppError> {
        self.core.drop_unsupported(request);
        let models = self.models(cancel).await?;
        let thinking = if models
            .iter()
            .find(|model| model.external_id == request.model)
            .is_some_and(|model| model.capabilities.contains(&ModelCapability::Reasoning))
        {
            ThinkingMode::Auto
        } else {
            ThinkingMode::Fast
        };
        let continued = if fresh {
            None
        } else {
            self.continuation(request)
        };
        let (chat_id, prompt, parent_id) = match continued {
            Some((chain, prompt)) => (chain.chat_id, prompt, Some(chain.response_id)),
            None => (
                self.create_chat(&request.model, cancel).await?,
                flatten_prompt(request),
                None,
            ),
        };
        let chained = parent_id.is_some();
        let frames = self.core.frames(
            self.turn(
                &chat_id,
                &request.model,
                &prompt,
                thinking,
                parent_id.as_deref(),
            )?,
            cancel,
        );
        Ok(Opened {
            frames,
            chat_id,
            chained,
        })
    }
}

enum Phase {
    Open,
    Retry,
    Frames(FrameStream),
    Done,
}

struct Run {
    shared: Arc<Shared>,
    request: GenerateRequest,
    cancel: CancellationToken,
    phase: Phase,
    queue: VecDeque<Result<TextDelta, AppError>>,
    decoder: TurnDecoder,
    finished: bool,
    summary: Option<SharedSummary>,
    chat_id: Option<String>,
    response_id: Option<String>,
    chained: bool,
    produced: bool,
    fresh: bool,
}

impl Run {
    fn fail(&mut self, error: AppError) {
        if is_session_expired(&error) {
            self.shared.forget_chats();
        }
        self.chat_id = None;
        self.queue.push_back(Err(error));
        self.phase = Phase::Done;
    }

    fn settle(&mut self, error: AppError) {
        if self.chained && !self.produced && !self.fresh && retryable_on_new_chat(&error) {
            self.shared.retire(self.chat_id.take().as_deref(), &error);
            self.response_id = None;
            self.decoder = TurnDecoder::new();
            self.finished = false;
            self.fresh = true;
            self.phase = Phase::Retry;
            return;
        }
        self.fail(error);
    }

    fn frame(&mut self, payload: &str) {
        for event in self.decoder.push(payload) {
            match event {
                TurnEvent::Failure(failure) => {
                    let failure = self.shared.core.normalise(vendor_failure(failure));
                    self.settle(failure);
                    return;
                }
                TurnEvent::Started(response_id) => self.response_id = Some(response_id),
                TurnEvent::Usage(usage) => {
                    if let Some(summary) = &self.summary {
                        with_summary(summary, |summary| summary.usage = Some(to_usage(usage)));
                    }
                }
                TurnEvent::Reasoning(chunk) => {
                    self.produced = true;
                    self.queue.push_back(Ok(TextDelta::reasoning(chunk)));
                }
                TurnEvent::Text(chunk) => {
                    self.produced = true;
                    self.queue.push_back(Ok(TextDelta::text(chunk)));
                }
                TurnEvent::Finished => self.finished = true,
            }
        }
    }

    async fn advance(&mut self) {
        match mem::replace(&mut self.phase, Phase::Done) {
            Phase::Done => {}
            Phase::Retry => {
                tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => {
                        self.fail(crate::drivers::ai::errors::cancelled());
                    }
                    () = tokio::time::sleep(self.shared.core.retry_delay) => {
                        self.phase = Phase::Open;
                    }
                }
            }
            Phase::Open => match self
                .shared
                .open(&self.request, &self.cancel, self.fresh)
                .await
            {
                Ok(opened) => {
                    self.chat_id = Some(opened.chat_id);
                    self.chained = opened.chained;
                    self.phase = Phase::Frames(opened.frames);
                }
                Err(error) => self.fail(error),
            },
            Phase::Frames(mut frames) => match frames.next().await {
                None => {
                    if !self.finished {
                        self.fail(truncated_stream(FAMILY, QWEN_COMPLETIONS_PATH));
                        return;
                    }
                    if let (Some(chat_id), Some(response_id)) =
                        (self.chat_id.take(), self.response_id.take())
                    {
                        self.shared.remember(&self.request, chat_id, response_id);
                    }
                    if let Some(summary) = &self.summary {
                        with_summary(summary, |summary| {
                            summary.finish_reason = Some(FinishReason::Stop);
                        });
                    }
                }
                Some(Err(error)) => self.settle(error),
                Some(Ok(payload)) => {
                    self.phase = Phase::Frames(frames);
                    self.frame(&payload);
                }
            },
        }
    }
}

pub struct QwenWebAdapter {
    shared: Arc<Shared>,
}

impl QwenWebAdapter {
    pub fn new(options: AccountAdapterOptions) -> Self {
        Self {
            shared: Arc::new(Shared {
                core: AccountAdapterCore::new(options, QWEN_WEB_CAPABILITIES),
                models: Mutex::new(None),
                creating: tokio::sync::Mutex::new(()),
                chains: Mutex::new(Chains::default()),
            }),
        }
    }

    pub fn chained_chats(&self) -> usize {
        self.shared.chained_chats()
    }

    fn run(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
        summary: Option<SharedSummary>,
    ) -> DeltaStream {
        let run = Run {
            shared: Arc::clone(&self.shared),
            request: request.clone(),
            cancel: cancel.clone(),
            phase: Phase::Open,
            queue: VecDeque::new(),
            decoder: TurnDecoder::new(),
            finished: false,
            summary,
            chat_id: None,
            response_id: None,
            chained: false,
            produced: false,
            fresh: false,
        };
        futures::stream::unfold(run, |mut run| async move {
            loop {
                if let Some(item) = run.queue.pop_front() {
                    if item.is_err() {
                        run.queue.clear();
                        run.phase = Phase::Done;
                    }
                    return Some((item, run));
                }
                if matches!(run.phase, Phase::Done) {
                    return None;
                }
                run.advance().await;
            }
        })
        .boxed()
    }
}

#[async_trait]
impl TextGenerationDriver for QwenWebAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        QWEN_WEB_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        let summary = SharedSummary::default();
        let mut deltas = self.run(request, cancel, Some(Arc::clone(&summary)));
        let mut text = String::new();
        let mut reasoning = String::new();
        while let Some(delta) = deltas.next().await {
            if let TextDelta::Content { text: chunk, kind } = delta? {
                match kind {
                    zvs_contract::DeltaKind::Reasoning => reasoning.push_str(&chunk),
                    zvs_contract::DeltaKind::Text => text.push_str(&chunk),
                }
            }
        }
        let summary = mem::take(&mut *summary.lock().unwrap_or_else(PoisonError::into_inner));
        Ok(GenerateResult {
            model: request.model.clone(),
            text,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            finish_reason: summary.finish_reason.unwrap_or(FinishReason::Unknown),
            usage: summary.usage,
            tool_calls: Vec::new(),
        })
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        self.run(request, cancel, None)
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        self.shared.list_models(cancel).await
    }
}

pub fn qwen_web_driver(options: AccountAdapterOptions) -> SharedTextDriver {
    with_tool_dialect(
        Arc::new(QwenWebAdapter::new(options)),
        Arc::new(QwenDialect),
        None,
    )
}

#[cfg(test)]
mod tests;
