use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::mem;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AppErrorCode, AuthMode, ModelCapability, ToolMode};
use zvs_zai_web::protocol::{self, chat_referer_path};
use zvs_zai_web::{
    CompletionTurn, NewChat, SignedRequest, TurnDecoder, TurnEvent, TurnMessage, VendorFailure,
    ZaiModel, parse_chat_created, parse_models,
};

use super::account_family::{
    AccountAdapterCore, AccountAdapterOptions, malformed_payload, malformed_turn,
    response_diagnostics, role_label, truncated_stream,
};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::{
    browser_verification, cancelled, is_cancellation, is_session_expired, session_expired,
};
use crate::drivers::ai::ports::{
    ChatMessage, ChatRole, DeltaStream, DiscoveredModel, FinishReason, GenerateRequest,
    GenerateResult, SharedTextDriver, TextDelta, TextGenerationDriver, TokenUsage,
};
use crate::drivers::ai::sse::FrameStream;
use crate::drivers::ai::tool_dialect::{DefaultDialect, with_tool_dialect};
use crate::drivers::ai::transport::RequestSpec;
use crate::platform::logger::LogLevel;

pub use zvs_zai_web::protocol::{
    CHATS_NEW_PATH as ZAI_CHATS_NEW_PATH, COMPLETIONS_PATH as ZAI_COMPLETIONS_PATH,
    DEFAULT_TIMEOUT_SECONDS as ZAI_DEFAULT_TIMEOUT_SECONDS, FE_VERSION as ZAI_FE_VERSION,
    GENERATION_OBSERVATION as ZAI_GENERATION_OBSERVATION, MODELS_PATH as ZAI_MODELS_PATH,
    USER_PATH as ZAI_USER_PATH,
};

pub const ZAI_GENERATION_ABORTED_MESSAGE: &str = "Вендор прервал генерацию";
pub const ZAI_CHAINS_MAX: usize = 256;
pub const ZAI_MODELS_TTL_MS: i64 = 60 * 60 * 1000;
pub const ZAI_DEFAULT_CONTEXT_WINDOW: i64 = 131_072;

pub const ZAI_WEB_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::ZaiWeb,
    auth_modes: &[AuthMode::Account],
    streaming: true,
    live_model_list: true,
    embedding: false,
    image: false,
    honours: Honours::NONE,
    tools: ToolMode::Injected,
};

const FAMILY: AdapterFamily = AdapterFamily::ZaiWeb;

fn to_discovered_model(model: ZaiModel) -> DiscoveredModel {
    let context_window = model.context_window.unwrap_or(ZAI_DEFAULT_CONTEXT_WINDOW);
    let mut capabilities = vec![ModelCapability::Streaming];
    if model.thinking {
        capabilities.push(ModelCapability::Reasoning);
    }
    DiscoveredModel {
        display_name: model.display_name,
        family: Some("glm".to_owned()),
        context_window: Some(context_window),
        max_output: model.max_output.map(|limit| limit.min(context_window)),
        capabilities,
        ..DiscoveredModel::named(model.id)
    }
}

pub fn map_zai_models(payload: &Value) -> Option<Vec<DiscoveredModel>> {
    let list = parse_models(payload)?;
    if list.models.is_empty() && list.skipped > 0 {
        return None;
    }
    Some(
        list.models
            .into_iter()
            .filter(|model| !model.hidden)
            .map(to_discovered_model)
            .collect(),
    )
}

fn vendor_failure(failure: VendorFailure) -> AppError {
    let mut details = zvs_contract::ErrorDetails::new();
    details.insert("family".to_owned(), json!(FAMILY.as_str()));
    details.insert("endpoint".to_owned(), json!(ZAI_COMPLETIONS_PATH));
    if let Some(code) = failure.code {
        details.insert("vendorCode".to_owned(), json!(code));
    }
    if failure.auth {
        return session_expired(details);
    }
    if failure.captcha {
        details.insert("reason".to_owned(), json!("browser-verification"));
        return browser_verification(details);
    }
    AppError::new(AppErrorCode::Unknown, ZAI_GENERATION_ABORTED_MESSAGE).with_details(details)
}

fn to_usage(raw: zvs_zai_web::Usage) -> TokenUsage {
    let count = |value: Option<u64>| value.and_then(|value| i64::try_from(value).ok());
    TokenUsage {
        prompt_tokens: count(raw.input_tokens),
        completion_tokens: count(raw.output_tokens),
        total_tokens: count(raw.total_tokens),
    }
}

const fn role_name(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::Assistant => "assistant",
        ChatRole::User | ChatRole::Tool => "user",
    }
}

fn dialog(request: &GenerateRequest) -> Vec<(&'static str, String)> {
    let mut system = request
        .system
        .as_deref()
        .map(str::trim)
        .filter(|system| !system.is_empty());
    let mut turns: Vec<(&'static str, String)> = request
        .messages
        .iter()
        .map(|message| {
            let content = match system.take_if(|_| message.role == ChatRole::User) {
                Some(system) => format!(
                    "{}: {system}\n\n{}: {}",
                    role_label(ChatRole::System),
                    role_label(ChatRole::User),
                    message.content
                ),
                None => message.content.clone(),
            };
            (role_name(message.role), content)
        })
        .collect();
    if let Some(system) = system {
        turns.insert(
            0,
            (
                "user",
                format!("{}: {system}", role_label(ChatRole::System)),
            ),
        );
    }
    turns
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
    answer_id: String,
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
        while self.order.len() > ZAI_CHAINS_MAX {
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

struct Opened {
    frames: FrameStream,
    chat_id: String,
    answer_id: String,
    chained: bool,
}

struct Shared {
    core: AccountAdapterCore,
    models: Mutex<Option<CachedModels>>,
    user_id: Mutex<Option<String>>,
    creating: tokio::sync::Mutex<()>,
    chains: Mutex<Chains>,
}

impl Shared {
    fn web_request(&self, spec: RequestSpec) -> RequestSpec {
        spec.with_header("x-fe-version", ZAI_FE_VERSION)
            .with_header("x-region", protocol::REGION)
    }

    fn referer(spec: RequestSpec, path: &str) -> RequestSpec {
        spec.with_header("referer", format!("{}{path}", protocol::ORIGIN))
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let response = self
            .core
            .request(
                &Self::referer(self.web_request(RequestSpec::get(ZAI_MODELS_PATH)), "/"),
                cancel,
            )
            .await?;
        let malformed = || {
            malformed_payload(
                FAMILY,
                ZAI_MODELS_PATH,
                &self.core.logger,
                response_diagnostics(&response),
            )
        };
        let payload = serde_json::from_str::<Value>(&response.text).map_err(|_| malformed())?;
        let models = map_zai_models(&payload).ok_or_else(malformed)?;
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
                    now - cached.fetched_at < ZAI_MODELS_TTL_MS,
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

    async fn user_id(&self, cancel: &CancellationToken) -> Result<String, AppError> {
        if let Some(id) = self
            .user_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Ok(id);
        }
        let response = self
            .core
            .request(
                &Self::referer(
                    self.web_request(
                        RequestSpec::get(ZAI_USER_PATH).with_accept("application/json"),
                    ),
                    "/",
                ),
                cancel,
            )
            .await?;
        let payload = self.core.decode(&response, ZAI_USER_PATH)?;
        let id = payload["id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                malformed_turn(
                    FAMILY,
                    ZAI_USER_PATH,
                    &self.core.logger,
                    response_diagnostics(&response),
                )
            })?;
        *self.user_id.lock().unwrap_or_else(PoisonError::into_inner) = Some(id.clone());
        Ok(id)
    }

    fn continuation(&self, request: &GenerateRequest) -> Option<Chain> {
        let last = request
            .messages
            .iter()
            .rposition(|message| message.role == ChatRole::Assistant)?;
        request
            .messages
            .get(last + 1..)
            .filter(|tail| !tail.is_empty())?;
        let key = history_key(
            &request.model,
            request.system.as_deref(),
            &request.messages[..last],
        );
        self.chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .by_key
            .get(&key)
            .cloned()
    }

    fn remember(&self, request: &GenerateRequest, chain: Chain) {
        self.chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                history_key(&request.model, request.system.as_deref(), &request.messages),
                chain,
            );
    }

    fn forget_chats(&self) {
        self.chains
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        *self.user_id.lock().unwrap_or_else(PoisonError::into_inner) = None;
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
            "Z.ai refused a chained turn; opening a new chat",
            &[
                ("family", json!(FAMILY.as_str())),
                ("code", json!(error.code)),
            ],
        );
    }

    async fn create_chat(
        &self,
        model: &str,
        prompt: &str,
        message_id: &str,
        thinking: bool,
        cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        let _one_at_a_time = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(cancelled()),
            guard = self.creating.lock() => guard,
        };
        let body = NewChat {
            model,
            prompt,
            message_id,
            thinking,
            now_ms: self.core.clock.now(),
        }
        .body();
        let response = self
            .core
            .request(
                &Self::referer(
                    self.web_request(
                        RequestSpec::post(ZAI_CHATS_NEW_PATH, body).with_accept("application/json"),
                    ),
                    "/",
                ),
                cancel,
            )
            .await?;
        let payload = self.core.decode(&response, ZAI_CHATS_NEW_PATH)?;
        parse_chat_created(&payload).ok_or_else(|| {
            malformed_turn(
                FAMILY,
                ZAI_CHATS_NEW_PATH,
                &self.core.logger,
                response_diagnostics(&response),
            )
        })
    }

    async fn open(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
        fresh: bool,
    ) -> Result<Opened, AppError> {
        self.core.drop_unsupported(request);
        let models = self.models(cancel).await?;
        let thinking = models
            .iter()
            .find(|model| model.external_id == request.model)
            .is_some_and(|model| model.capabilities.contains(&ModelCapability::Reasoning));
        let user_id = self.user_id(cancel).await?;
        let dialog = dialog(request);
        let messages: Vec<TurnMessage<'_>> = dialog
            .iter()
            .map(|(role, content)| TurnMessage { role, content })
            .collect();
        let message_id = (self.core.new_id)();
        let answer_id = (self.core.new_id)();
        let continued = if fresh {
            None
        } else {
            self.continuation(request)
        };
        let chained = continued.is_some();
        let (chat_id, parent_id) = match continued {
            Some(chain) => (chain.chat_id, Some(chain.answer_id)),
            None => {
                let prompt = CompletionTurn {
                    chat_id: "",
                    model: &request.model,
                    messages: &messages,
                    thinking,
                    reasoning_effort: None,
                    message_id: &message_id,
                    parent_id: None,
                    response_id: &answer_id,
                }
                .signature_prompt()
                .to_owned();
                let chat_id = self
                    .create_chat(&request.model, &prompt, &message_id, thinking, cancel)
                    .await?;
                (chat_id, None)
            }
        };
        let turn = CompletionTurn {
            chat_id: &chat_id,
            model: &request.model,
            messages: &messages,
            thinking,
            reasoning_effort: None,
            message_id: &message_id,
            parent_id: parent_id.as_deref(),
            response_id: &answer_id,
        };
        let request_id = (self.core.new_id)();
        let signed = SignedRequest {
            request_id: &request_id,
            user_id: &user_id,
            timestamp_ms: self.core.clock.now(),
            prompt: turn.signature_prompt(),
        };
        let mut spec = RequestSpec::post(ZAI_COMPLETIONS_PATH, turn.body())
            .with_header("x-signature", signed.signature());
        for (key, value) in signed.query("") {
            spec = spec.with_query(key, value);
        }
        let spec = Self::referer(self.web_request(spec), &chat_referer_path(&chat_id));
        let frames = self.core.frames(spec, cancel);
        Ok(Opened {
            frames,
            chat_id,
            answer_id,
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
    chain: Option<Chain>,
    chained: bool,
    produced: bool,
    fresh: bool,
}

impl Run {
    fn fail(&mut self, error: AppError) {
        if is_session_expired(&error) {
            self.shared.forget_chats();
        }
        self.chain = None;
        self.queue.push_back(Err(error));
        self.phase = Phase::Done;
    }

    fn settle(&mut self, error: AppError) {
        if self.chained && !self.produced && !self.fresh && retryable_on_new_chat(&error) {
            let chat_id = self.chain.take().map(|chain| chain.chat_id);
            self.shared.retire(chat_id.as_deref(), &error);
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
                    () = self.cancel.cancelled() => self.fail(cancelled()),
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
                    self.chain = Some(Chain {
                        chat_id: opened.chat_id,
                        answer_id: opened.answer_id,
                    });
                    self.chained = opened.chained;
                    self.phase = Phase::Frames(opened.frames);
                }
                Err(error) => self.fail(error),
            },
            Phase::Frames(mut frames) => match frames.next().await {
                None => {
                    if !self.finished {
                        self.fail(truncated_stream(FAMILY, ZAI_COMPLETIONS_PATH));
                        return;
                    }
                    if let Some(chain) = self.chain.take() {
                        self.shared.remember(&self.request, chain);
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

pub struct ZaiWebAdapter {
    shared: Arc<Shared>,
}

impl ZaiWebAdapter {
    pub fn new(options: AccountAdapterOptions) -> Self {
        Self {
            shared: Arc::new(Shared {
                core: AccountAdapterCore::new(options, ZAI_WEB_CAPABILITIES),
                models: Mutex::new(None),
                user_id: Mutex::new(None),
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
            chain: None,
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
impl TextGenerationDriver for ZaiWebAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        ZAI_WEB_CAPABILITIES
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

    fn excluded_policy_check_tools(&self) -> &'static [&'static str] {
        zvs_zai_web::EXCLUDED_POLICY_CHECK_TOOLS_NAMES
    }
}

pub fn zai_web_driver(options: AccountAdapterOptions) -> SharedTextDriver {
    with_tool_dialect(
        Arc::new(ZaiWebAdapter::new(options)),
        Arc::new(DefaultDialect),
        None,
    )
}

#[cfg(test)]
mod tests;
