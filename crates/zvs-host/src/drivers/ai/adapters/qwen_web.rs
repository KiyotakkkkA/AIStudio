use std::collections::{HashSet, VecDeque};
use std::mem;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Number, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AppErrorCode, AuthMode, ModelCapability, ToolMode};

use super::account_family::{
    AccountAdapterCore, AccountAdapterOptions, VendorObservation, flatten_prompt,
    malformed_payload, malformed_turn, truncated_stream,
};
use super::wire::object;
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::session_expired;
use crate::drivers::ai::ports::{
    DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult, TextDelta,
    TextGenerationDriver, TokenUsage,
};
use crate::drivers::ai::sse::FrameStream;
use crate::drivers::ai::transport::RequestSpec;

pub const QWEN_MODELS_PATH: &str = "/models/";
pub const QWEN_CHATS_NEW_PATH: &str = "/chats/new";
pub const QWEN_COMPLETIONS_PATH: &str = "/chat/completions";

pub const QWEN_CHAT_MODE: &str = "local";
pub const QWEN_CHAT_TYPE: &str = "t2t";
pub const QWEN_API_VERSION: &str = "2.1";
pub const GENERATION_ABORTED_MESSAGE: &str = "Вендор прервал генерацию";

pub const QWEN_MODELS_OBSERVATION: VendorObservation = VendorObservation {
    source: "https://chat.qwen.ai/api/v2/models/",
    observed_at: "2026-09-10",
};

pub const QWEN_GENERATION_OBSERVATION: VendorObservation = VendorObservation {
    source: "https://chat.qwen.ai/api/v2/chats/new then POST /api/v2/chat/completions?chat_id=… (SSE, one vendor chat per generation)",
    observed_at: "2026-09-10",
};

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

#[derive(Deserialize)]
#[serde(untagged)]
enum Flag {
    Bool(bool),
    Number(f64),
}

#[derive(Deserialize)]
struct Flags {
    #[serde(rename = "vision")]
    _vision: Option<Flag>,
    #[serde(rename = "document")]
    _document: Option<Flag>,
    #[serde(rename = "video")]
    _video: Option<Flag>,
    #[serde(rename = "audio")]
    _audio: Option<Flag>,
    thinking: Option<Flag>,
}

#[derive(Deserialize)]
struct ModelMeta {
    max_context_length: Option<Number>,
    max_generation_length: Option<Number>,
    #[serde(rename = "modality")]
    _modality: Option<Vec<String>>,
    capabilities: Option<Flags>,
    abilities: Option<Flags>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: Option<String>,
    is_active: Option<bool>,
    meta: Option<ModelMeta>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    name: Option<String>,
    owned_by: Option<String>,
    info: Option<ModelInfo>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ModelData {
    List(Vec<ModelEntry>),
    Wrapped { data: Vec<ModelEntry> },
}

#[derive(Deserialize)]
struct ModelsPayload {
    success: Option<bool>,
    data: Option<ModelData>,
}

#[derive(Deserialize)]
struct ChatCreatedData {
    id: Option<String>,
}

#[derive(Deserialize)]
struct ChatCreated {
    success: Option<bool>,
    id: Option<String>,
    data: Option<ChatCreatedData>,
}

#[derive(Deserialize)]
struct Summary {
    content: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Extra {
    #[serde(rename = "summary_title")]
    _title: Option<Summary>,
    summary_thought: Option<Summary>,
}

#[derive(Deserialize)]
struct Delta {
    #[serde(rename = "role")]
    _role: Option<String>,
    content: Option<String>,
    phase: Option<String>,
    status: Option<String>,
    extra: Option<Extra>,
}

#[derive(Deserialize)]
struct Choice {
    delta: Option<Delta>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum VendorCode {
    Number(Number),
    Text(String),
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: Option<VendorCode>,
    #[serde(rename = "message")]
    _message: Option<String>,
    status: Option<i64>,
}

enum VendorError {
    Text,
    Detail(ErrorDetail),
}

impl<'de> Deserialize<'de> for VendorError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(_) => Ok(Self::Text),
            detail @ Value::Object(_) => ErrorDetail::deserialize(detail)
                .map(Self::Detail)
                .map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom(
                "a vendor error is a string or an object",
            )),
        }
    }
}

#[derive(Deserialize)]
struct StreamFrame {
    choices: Option<Vec<Choice>>,
    usage: Option<Usage>,
    success: Option<bool>,
    status: Option<i64>,
    code: Option<VendorCode>,
    error: Option<VendorError>,
}

fn positive(value: Option<&Number>) -> Result<Option<i64>, ()> {
    let Some(number) = value else {
        return Ok(None);
    };
    let whole = number
        .as_i64()
        .or_else(|| {
            number
                .as_f64()
                .filter(|float| float.fract() == 0.0)
                .map(|float| float as i64)
        })
        .ok_or(())?;
    if whole > 0 { Ok(Some(whole)) } else { Err(()) }
}

fn enabled(flag: Option<&Flag>) -> bool {
    match flag {
        Some(Flag::Bool(value)) => *value,
        Some(Flag::Number(value)) => *value > 0.0,
        None => false,
    }
}

fn text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_owned)
}

fn to_discovered_model(external_id: String, entry: &ModelEntry) -> Result<DiscoveredModel, ()> {
    let meta = entry.info.as_ref().and_then(|info| info.meta.as_ref());
    let context_window = positive(meta.and_then(|meta| meta.max_context_length.as_ref()))?;
    let max_output = positive(meta.and_then(|meta| meta.max_generation_length.as_ref()))?;
    let thinking = meta
        .and_then(|meta| meta.capabilities.as_ref())
        .and_then(|flags| flags.thinking.as_ref())
        .or_else(|| {
            meta.and_then(|meta| meta.abilities.as_ref())
                .and_then(|flags| flags.thinking.as_ref())
        });
    let mut capabilities = vec![ModelCapability::Streaming];
    if enabled(thinking) {
        capabilities.push(ModelCapability::Reasoning);
    }
    Ok(DiscoveredModel {
        display_name: text(entry.name.as_deref())
            .or_else(|| text(entry.info.as_ref().and_then(|info| info.name.as_deref())))
            .unwrap_or_else(|| external_id.clone()),
        family: text(entry.owned_by.as_deref()),
        context_window,
        max_output,
        capabilities,
        ..DiscoveredModel::named(external_id)
    })
}

pub fn map_qwen_models(payload: &Value) -> Option<Vec<DiscoveredModel>> {
    let parsed = ModelsPayload::deserialize(payload).ok()?;
    if parsed.success == Some(false) {
        return None;
    }
    let entries = match parsed.data? {
        ModelData::List(entries) | ModelData::Wrapped { data: entries } => entries,
    };
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut converted = Vec::with_capacity(entries.len());
    for entry in &entries {
        converted.push(to_discovered_model(entry.id.trim().to_owned(), entry).ok()?);
    }
    for (entry, model) in entries.iter().zip(converted) {
        let external_id = entry.id.trim();
        if external_id.is_empty() || seen.contains(external_id) {
            continue;
        }
        if entry.info.as_ref().and_then(|info| info.is_active) == Some(false) {
            continue;
        }
        seen.insert(external_id.to_owned());
        models.push(model);
    }
    Some(models)
}

fn code_json(code: &VendorCode) -> Value {
    match code {
        VendorCode::Number(number) => Value::Number(number.clone()),
        VendorCode::Text(text) => Value::String(text.clone()),
    }
}

fn is_auth_code(code: Option<&VendorCode>) -> bool {
    match code {
        Some(VendorCode::Number(number)) => {
            matches!(number.as_f64(), Some(value) if value == 401.0 || value == 403.0)
        }
        Some(VendorCode::Text(text)) => {
            if text == "401" || text == "403" {
                return true;
            }
            let lower = text.to_lowercase();
            ["unauth", "forbidden", "token", "login", "expire", "session"]
                .iter()
                .any(|needle| lower.contains(needle))
        }
        None => false,
    }
}

fn vendor_failure(frame: &StreamFrame) -> Option<AppError> {
    let (nested_code, nested_status) = match &frame.error {
        Some(VendorError::Detail(detail)) => (detail.code.as_ref(), detail.status),
        _ => (None, None),
    };
    let code = nested_code.or(frame.code.as_ref());
    let status = nested_status.or(frame.status);
    let mut details = zvs_contract::ErrorDetails::new();
    details.insert("family".to_owned(), json!(FAMILY.as_str()));
    details.insert("endpoint".to_owned(), json!(QWEN_COMPLETIONS_PATH));
    if let Some(code) = code {
        details.insert("vendorCode".to_owned(), code_json(code));
    }
    if let Some(status) = status {
        details.insert("status".to_owned(), json!(status));
    }
    if matches!(status, Some(401 | 403)) || is_auth_code(code) {
        return Some(session_expired(details));
    }
    if frame.success != Some(false)
        && frame.error.is_none()
        && status.is_none_or(|status| status < 400)
    {
        return None;
    }
    Some(AppError::new(AppErrorCode::Unknown, GENERATION_ABORTED_MESSAGE).with_details(details))
}

fn summary_deltas(emitted: &mut Vec<String>, next: &[String]) -> Vec<String> {
    let mut chunks = Vec::new();
    for (index, value) in next.iter().enumerate() {
        let seen = emitted.get(index).cloned().unwrap_or_default();
        if *value == seen {
            continue;
        }
        chunks.push(match value.strip_prefix(seen.as_str()) {
            Some(rest) => rest.to_owned(),
            None => value.clone(),
        });
        if emitted.len() <= index {
            emitted.resize(index + 1, String::new());
        }
        emitted[index].clone_from(value);
    }
    chunks
}

fn to_usage(raw: Option<&Usage>) -> Option<TokenUsage> {
    let raw = raw?;
    let count = |value: Option<u64>| value.and_then(|value| i64::try_from(value).ok());
    Some(TokenUsage {
        prompt_tokens: count(raw.input_tokens),
        completion_tokens: count(raw.output_tokens),
        total_tokens: count(raw.total_tokens),
    })
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
}

impl Shared {
    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let response = self
            .core
            .request(&RequestSpec::get(QWEN_MODELS_PATH), cancel)
            .await?;
        let payload = self.core.decode(&response.text, QWEN_MODELS_PATH)?;
        let models = map_qwen_models(&payload)
            .ok_or_else(|| malformed_payload(FAMILY, QWEN_MODELS_PATH, &self.core.logger))?;
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
        let body = object([
            ("chatId", json!("")),
            ("chat_mode", json!(QWEN_CHAT_MODE)),
            ("chat_type", json!(QWEN_CHAT_TYPE)),
            ("models", json!([model])),
            ("project_id", json!("")),
            ("timestamp", json!(self.core.clock.now())),
        ]);
        let response = self
            .core
            .request(&RequestSpec::post(QWEN_CHATS_NEW_PATH, body), cancel)
            .await?;
        let payload = self.core.decode(&response.text, QWEN_CHATS_NEW_PATH)?;
        let malformed = || malformed_turn(FAMILY, QWEN_CHATS_NEW_PATH, &self.core.logger);
        let created = ChatCreated::deserialize(&payload).map_err(|_| malformed())?;
        if created.success == Some(false) {
            return Err(malformed());
        }
        let chat_id = created
            .data
            .and_then(|data| data.id)
            .or(created.id)
            .unwrap_or_default();
        let chat_id = chat_id.trim();
        if chat_id.is_empty() {
            return Err(malformed());
        }
        Ok(chat_id.to_owned())
    }

    fn turn(&self, chat_id: &str, request: &GenerateRequest, thinking: bool) -> RequestSpec {
        let timestamp = self.core.clock.now().div_euclid(1000);
        let message = object([
            ("id", Value::Null),
            ("fid", json!((self.core.new_id)())),
            ("parentId", Value::Null),
            ("parent_id", Value::Null),
            ("childrenIds", json!([])),
            ("role", json!("user")),
            ("content", json!(flatten_prompt(request))),
            ("user_action", json!("chat")),
            ("files", json!([])),
            ("timestamp", json!(timestamp)),
            ("model", json!("")),
            ("models", json!([request.model])),
            ("chat_type", json!(QWEN_CHAT_TYPE)),
            ("sub_chat_type", json!(QWEN_CHAT_TYPE)),
            (
                "feature_config",
                object([
                    ("thinking_enabled", json!(thinking)),
                    ("thinking_format", json!("summary")),
                    (
                        "thinking_mode",
                        json!(if thinking { "Thinking" } else { "Fast" }),
                    ),
                    ("auto_thinking", json!(false)),
                    ("auto_search", json!(false)),
                    ("output_schema", json!("phase")),
                    ("research_mode", json!("normal")),
                ]),
            ),
            (
                "extra",
                object([("meta", object([("subChatType", json!(QWEN_CHAT_TYPE))]))]),
            ),
        ]);
        let body = object([
            ("chatId", json!(chat_id)),
            ("chat_id", json!(chat_id)),
            ("chat_mode", json!(QWEN_CHAT_MODE)),
            ("chat_type", json!(QWEN_CHAT_TYPE)),
            ("incremental_output", json!(true)),
            ("messages", json!([message])),
            ("model", json!(request.model)),
            ("parentId", Value::Null),
            ("parent_id", Value::Null),
            ("stream", json!(true)),
            ("timestamp", json!(timestamp)),
            ("version", json!(QWEN_API_VERSION)),
        ]);
        RequestSpec::post(QWEN_COMPLETIONS_PATH, body)
            .with_query("chat_id", chat_id)
            .with_accept("text/event-stream")
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
            .frames(self.turn(&chat_id, request, thinking), cancel))
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
    emitted: Vec<String>,
    finished: bool,
    summary: Option<SharedSummary>,
}

impl Run {
    fn fail(&mut self, error: AppError) {
        self.queue.push_back(Err(error));
        self.phase = Phase::Done;
    }

    fn frame(&mut self, payload: &str) {
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        let Ok(frame) = StreamFrame::deserialize(&value) else {
            return;
        };
        if let Some(failure) = vendor_failure(&frame) {
            let failure = self.shared.core.normalise(failure);
            self.fail(failure);
            return;
        }
        if let Some(summary) = &self.summary
            && let Some(usage) = to_usage(frame.usage.as_ref())
        {
            with_summary(summary, |summary| summary.usage = Some(usage));
        }
        let Some(delta) = frame
            .choices
            .and_then(|choices| choices.into_iter().next())
            .and_then(|choice| choice.delta)
        else {
            return;
        };
        if delta.phase.as_deref() == Some("thinking_summary") {
            let thought = delta
                .extra
                .and_then(|extra| extra.summary_thought)
                .and_then(|summary| summary.content)
                .unwrap_or_default();
            for chunk in summary_deltas(&mut self.emitted, &thought) {
                self.queue.push_back(Ok(TextDelta::reasoning(chunk)));
            }
            return;
        }
        if delta.phase.as_deref() != Some("answer") {
            return;
        }
        let content = delta.content.unwrap_or_default();
        if !content.is_empty() {
            self.queue.push_back(Ok(TextDelta::text(content)));
        }
        if delta.status.as_deref() == Some("finished") {
            self.finished = true;
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
            emitted: Vec::new(),
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
