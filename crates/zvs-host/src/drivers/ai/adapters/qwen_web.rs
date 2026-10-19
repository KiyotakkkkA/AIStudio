use std::collections::VecDeque;
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
    CompletionTurn, ModelsDiagnostics, QwenModel, RowProblem, TurnDecoder, TurnEvent,
    VendorFailure, new_chat_body, parse_chat_created, parse_models,
};

use super::account_family::{
    AccountAdapterCore, AccountAdapterOptions, MALFORMED_MODELS_MESSAGE, flatten_prompt,
    malformed_turn, response_diagnostics, truncated_stream,
};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::{html_title, request_failed, session_expired};
use crate::drivers::ai::ports::{
    DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult, TextDelta,
    TextGenerationDriver, TokenUsage,
};
use crate::drivers::ai::sse::FrameStream;
use crate::drivers::ai::transport::{RequestSpec, TransportResponse};
use crate::platform::logger::{LogLevel, SharedLogger};

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

struct Shared {
    core: AccountAdapterCore,
    models: Mutex<Option<Vec<DiscoveredModel>>>,
    /// The client creates one chat at a time; so does the adapter, to stay client-like.
    creating: tokio::sync::Mutex<()>,
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
        *self.models.lock().unwrap_or_else(PoisonError::into_inner) = Some(models.clone());
        Ok(models)
    }

    fn cached_models(&self) -> Option<Vec<DiscoveredModel>> {
        self.models
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
        request: &GenerateRequest,
        thinking: bool,
    ) -> Result<RequestSpec, AppError> {
        let prompt = flatten_prompt(request);
        let fid = (self.core.new_id)();
        let body = CompletionTurn {
            chat_id,
            model: &request.model,
            prompt: &prompt,
            thinking,
            fid: &fid,
            now_ms: self.core.clock.now(),
        }
        .body();
        self.web_request(
            RequestSpec::post(QWEN_COMPLETIONS_PATH, body)
                .with_query("chat_id", chat_id)
                .with_accept("text/event-stream"),
            &chat_referer_path(chat_id),
        )
    }

    async fn open(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<FrameStream, AppError> {
        self.core.drop_unsupported(request);
        let models = match self.cached_models() {
            Some(models) => models,
            None => self.list_models(cancel).await?,
        };
        let thinking = models
            .iter()
            .find(|model| model.external_id == request.model)
            .is_some_and(|model| model.capabilities.contains(&ModelCapability::Reasoning));
        let chat_id = self.create_chat(&request.model, cancel).await?;
        Ok(self
            .core
            .frames(self.turn(&chat_id, request, thinking)?, cancel))
    }
}

enum Phase {
    Open,
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
}

impl Run {
    fn fail(&mut self, error: AppError) {
        self.queue.push_back(Err(error));
        self.phase = Phase::Done;
    }

    fn frame(&mut self, payload: &str) {
        for event in self.decoder.push(payload) {
            match event {
                TurnEvent::Failure(failure) => {
                    let failure = self.shared.core.normalise(vendor_failure(failure));
                    self.fail(failure);
                    return;
                }
                TurnEvent::Usage(usage) => {
                    if let Some(summary) = &self.summary {
                        with_summary(summary, |summary| summary.usage = Some(to_usage(usage)));
                    }
                }
                TurnEvent::Reasoning(chunk) => {
                    self.queue.push_back(Ok(TextDelta::reasoning(chunk)))
                }
                TurnEvent::Text(chunk) => self.queue.push_back(Ok(TextDelta::text(chunk))),
                TurnEvent::Finished => self.finished = true,
            }
        }
    }

    async fn advance(&mut self) {
        match mem::replace(&mut self.phase, Phase::Done) {
            Phase::Done => {}
            Phase::Open => match self.shared.open(&self.request, &self.cancel).await {
                Ok(frames) => self.phase = Phase::Frames(frames),
                Err(error) => self.fail(error),
            },
            Phase::Frames(mut frames) => match frames.next().await {
                None => {
                    if !self.finished {
                        self.fail(truncated_stream(FAMILY, QWEN_COMPLETIONS_PATH));
                    } else if let Some(summary) = &self.summary {
                        with_summary(summary, |summary| {
                            summary.finish_reason = Some(FinishReason::Stop);
                        });
                    }
                }
                Some(Err(error)) => self.fail(error),
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
            }),
        }
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

#[cfg(test)]
mod tests;
