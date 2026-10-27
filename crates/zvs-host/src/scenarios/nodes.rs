use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, PoisonError, RwLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use zvs_contract::{
    AppError, AppErrorCode, HostEventDraft, ToolListFilter, ToolMode, Validate, log_line,
    resolve_tool_mode,
};

use super::catalogue::{
    AGENT_TOOLS, AREA_BOTH, AREA_DIALOG, AREA_GLOBAL, AREA_PARAM, OUTPUT_CHAT_MESSAGE,
    OUTPUT_CHAT_REPLY, OUTPUT_FILE, OUTPUT_NEW_CHAT, OUTPUT_NOTIFY, POLICY_PARAM,
    POLICY_READ_WRITE, STORES_PARAM, TEMPLATE_PARAM, TRIGGER_CHAT, TRIGGER_MAIL, TRIGGER_SCHEDULE,
    TRIGGER_WEBHOOK,
};
use super::compile::FROM_PREFIX;
use super::template::{TemplateScope, civil_date, render, text_of};
use crate::drivers::ai::{ChatMessage, ChatRole, GenerateRequest, ToolCall};
use crate::kernel::knowledge_nodes::KNOWLEDGE_SEARCH;
use crate::kernel::nodes::PROVIDERS_UNAVAILABLE;
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, StepContext, ToolStepRequest};
use crate::services::MemoryService;
use crate::services::chat::GenerateChatNode;
use crate::services::chat_memory::{
    MEMORY_TOOL, MemoryRefs, apply_memory_call, memory_section, memory_tool,
};
use crate::services::chat_tools::{
    KNOWLEDGE_INSTRUCTION, OfferedTool, TOOL_BUDGET_NOTE, TOOL_RESULT_MAX_CHARS,
    TOOL_RESULTS_INSTRUCTION, cap_text, fence_tool_result, offered_tools, parse_tool_arguments,
    tool_parameters, tool_result_text, tool_specs, wire_tool_name,
};
use crate::skills::{presented, result_limit};
use crate::tools::ToolRegistry;

pub const EFFECTS_UNAVAILABLE: &str = "Действия сценария недоступны";
pub const MODEL_MISSING: &str = "К ИИ-агенту не подключена чат-модель";
pub const NO_REPLY_TARGET: &str = "Ответить некуда: сценарий запущен не сообщением в чате";
pub const UNKNOWN_TOOL: &str = "Инструмент не предоставлен агенту";
pub const DEFAULT_ITERATIONS: u64 = 8;
pub const DEFAULT_MEMORY: u64 = 10;
pub const DEFAULT_CHAT_TITLE: &str = "Результат сценария";

pub trait ScenarioEffects: Send + Sync {
    fn post_message(&self, conversation_id: &str, text: &str) -> Result<Value, AppError>;
    fn create_conversation(&self, title: &str, text: &str) -> Result<Value, AppError>;
    fn notify(&self, title: &str, body: &str) -> Result<(), AppError>;
    fn write_file(&self, path: &str, text: &str) -> Result<Value, AppError>;
}

pub type SharedScenarioEffects = Arc<dyn ScenarioEffects>;

#[derive(Clone, Default)]
pub struct EffectsSlot(Arc<RwLock<Option<SharedScenarioEffects>>>);

impl EffectsSlot {
    pub fn attach(&self, effects: SharedScenarioEffects) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = Some(effects);
    }

    fn get(&self) -> Result<SharedScenarioEffects, AppError> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, EFFECTS_UNAVAILABLE))
    }
}

#[derive(Clone, Default)]
pub struct ScenarioNodeOptions {
    pub effects: EffectsSlot,
    pub tools: Option<ToolRegistry>,
    pub memory: Option<Arc<MemoryService>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepInput {
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default)]
    pub event: Value,
    #[serde(default)]
    pub model: Option<Value>,
    #[serde(default)]
    pub memory: Option<Value>,
    #[serde(default)]
    pub embedding: Option<Value>,
    #[serde(default)]
    pub knowledge: Option<Value>,
    #[serde(default)]
    pub facts: Option<Value>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(flatten)]
    pub sources: BTreeMap<String, Value>,
}

impl Validate for StepInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

impl StepInput {
    pub fn upstream(&self) -> Value {
        let sources: Vec<(&str, &Value)> = self
            .sources
            .iter()
            .filter_map(|(key, value)| Some((key.strip_prefix(FROM_PREFIX)?, value)))
            .collect();
        match sources.as_slice() {
            [] => Value::Null,
            [(_, value)] => (*value).clone(),
            many => Value::Object(
                many.iter()
                    .map(|(id, value)| ((*id).to_owned(), (*value).clone()))
                    .collect(),
            ),
        }
    }

    fn param_text(&self, key: &str) -> Option<String> {
        match self.params.get(key)? {
            Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        }
    }

    fn param_u64(&self, key: &str) -> Option<u64> {
        match self.params.get(key)? {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    fn render(&self, template: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            });
        let date = civil_date(now);
        let result = self.upstream();
        render(
            template,
            &TemplateScope {
                result: &result,
                event: &self.event,
                date: &date,
            },
        )
    }

    fn body(&self) -> String {
        match self.param_text(TEMPLATE_PARAM) {
            Some(template) => self.render(&template),
            None => text_of(&self.upstream()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Payload(pub Value);

impl Validate for Payload {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

struct TriggerNode(&'static str);

#[async_trait]
impl NodeDef for TriggerNode {
    type Input = StepInput;
    type Output = Payload;

    fn node_type(&self) -> &str {
        self.0
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(&self, context: &StepContext, input: StepInput) -> Result<Payload, AppError> {
        context.check_cancelled()?;
        let pinned = input.upstream();
        let event = if input.event.is_null() {
            pinned
        } else {
            input.event
        };
        Ok(Payload(if event.is_null() { json!({}) } else { event }))
    }
}

#[derive(Clone, Copy)]
enum Output {
    ChatReply,
    ChatMessage,
    NewChat,
    Notify,
    File,
}

struct OutputNode {
    node_type: &'static str,
    output: Output,
    effects: EffectsSlot,
}

fn required_text(input: &StepInput, key: &str) -> Result<String, AppError> {
    input
        .param_text(key)
        .map(|text| input.render(&text))
        .ok_or_else(|| {
            AppError::new(
                AppErrorCode::ValidationFailed,
                format!("Не задан параметр «{key}»"),
            )
        })
}

fn with_text(value: Value, text: &str) -> Value {
    let mut map = match value {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => Map::from_iter([("value".to_owned(), other)]),
    };
    map.insert("text".to_owned(), Value::String(text.to_owned()));
    Value::Object(map)
}

#[async_trait]
impl NodeDef for OutputNode {
    type Input = StepInput;
    type Output = Payload;

    fn node_type(&self) -> &str {
        self.node_type
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(&self, context: &StepContext, input: StepInput) -> Result<Payload, AppError> {
        context.check_cancelled()?;
        let effects = self.effects.get()?;
        let text = input.body();
        let result = match self.output {
            Output::ChatReply => {
                let conversation = input
                    .event
                    .get("conversationId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_REPLY_TARGET))?;
                effects.post_message(conversation, &text)?
            }
            Output::ChatMessage => {
                effects.post_message(&required_text(&input, "conversationId")?, &text)?
            }
            Output::NewChat => {
                let title = input.param_text("title").map_or_else(
                    || DEFAULT_CHAT_TITLE.to_owned(),
                    |title| input.render(&title),
                );
                effects.create_conversation(&title, &text)?
            }
            Output::Notify => {
                let title = required_text(&input, "title")?;
                effects.notify(&title, &text)?;
                json!({ "title": title })
            }
            Output::File => effects.write_file(&required_text(&input, "path")?, &text)?,
        };
        Ok(Payload(with_text(result, &text)))
    }
}

struct AgentNode {
    nodes: Weak<NodeRegistry>,
    tools: Option<ToolRegistry>,
    memory: Option<Arc<MemoryService>>,
}

struct Recall {
    service: Arc<MemoryService>,
    area: String,
    conversation: String,
    writes: bool,
    refs: MemoryRefs,
}

impl Recall {
    fn section(&mut self, tool: Option<&str>) -> Result<String, AppError> {
        let mut snapshot = self.service.snapshot(&self.conversation)?;
        if self.area == AREA_DIALOG {
            snapshot.account.clear();
        }
        if self.area == AREA_GLOBAL || self.conversation.is_empty() {
            snapshot.conversation.clear();
        }
        let (section, refs) = memory_section(&snapshot, tool);
        self.refs = refs;
        Ok(section)
    }

    fn apply(&mut self, arguments: &Value) -> Result<String, String> {
        let mut arguments = arguments.clone();
        if let Value::Object(fields) = &mut arguments
            && fields.get("action").and_then(Value::as_str) == Some("save")
        {
            match self.area.as_str() {
                AREA_GLOBAL => {
                    fields.insert("scope".to_owned(), json!("account"));
                }
                AREA_DIALOG => {
                    fields.insert("scope".to_owned(), json!("conversation"));
                }
                _ => {}
            }
        }
        apply_memory_call(
            &self.service,
            &arguments,
            &self.conversation,
            &mut self.refs,
        )
    }
}

struct Session<'a> {
    by_name: HashMap<&'a str, &'a OfferedTool>,
    stores: Vec<String>,
    recall: Option<Recall>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentOutput {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: u64,
    pub rounds: u64,
}

impl Validate for AgentOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn slot_params(slot: Option<&Value>) -> Map<String, Value> {
    slot.and_then(|slot| slot.get("params"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn history(event: &Value, size: u64) -> Vec<ChatMessage> {
    let entries: Vec<ChatMessage> = event
        .get("history")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let content = entry.get("content")?.as_str()?;
            let role = match entry.get("role")?.as_str()? {
                "user" => ChatRole::User,
                "assistant" => ChatRole::Assistant,
                _ => return None,
            };
            Some(ChatMessage::new(role, content))
        })
        .collect();
    let keep = usize::try_from(size).unwrap_or(usize::MAX);
    entries[entries.len().saturating_sub(keep)..].to_vec()
}

fn store_ids(slot: Option<&Value>) -> Vec<String> {
    slot_params(slot)
        .get(STORES_PARAM)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

impl AgentNode {
    fn recall(&self, input: &StepInput) -> Result<Option<Recall>, AppError> {
        let (Some(service), Some(_)) = (&self.memory, &input.facts) else {
            return Ok(None);
        };
        let switches = service.switches()?;
        if !switches.enabled {
            return Ok(None);
        }
        let params = slot_params(input.facts.as_ref());
        let area = params
            .get(AREA_PARAM)
            .and_then(Value::as_str)
            .filter(|area| [AREA_GLOBAL, AREA_DIALOG, AREA_BOTH].contains(area))
            .unwrap_or(AREA_GLOBAL);
        Ok(Some(Recall {
            service: Arc::clone(service),
            area: area.to_owned(),
            conversation: input
                .event
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            writes: switches.model
                && params.get(POLICY_PARAM).and_then(Value::as_str) == Some(POLICY_READ_WRITE),
            refs: MemoryRefs::default(),
        }))
    }

    fn offered(&self, keys: &[String]) -> Result<Vec<OfferedTool>, AppError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(tools) = &self.tools {
            let registered = tools.list(&ToolListFilter::default())?;
            return Ok(offered_tools(keys, &registered));
        }
        let Some(nodes) = self.nodes.upgrade() else {
            return Ok(Vec::new());
        };
        let mut sorted = keys.to_vec();
        sorted.sort();
        let mut taken = HashSet::new();
        Ok(sorted
            .iter()
            .filter_map(|key| {
                let node = nodes.for_tool(key)?;
                let name = wire_tool_name(key, &taken);
                taken.insert(name.clone());
                Some(OfferedTool {
                    key: key.clone(),
                    name,
                    description: key.clone(),
                    parameters: tool_parameters(node.input_schema()),
                    destructive: false,
                    read_only: node.side_effect_free(),
                    source: zvs_contract::OfferedToolSource::Base,
                    server: None,
                })
            })
            .collect())
    }
}

#[async_trait]
impl NodeDef for AgentNode {
    type Input = StepInput;
    type Output = AgentOutput;

    fn node_type(&self) -> &str {
        AGENT_TOOLS
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("llm.generate")
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(&self, context: &StepContext, input: StepInput) -> Result<AgentOutput, AppError> {
        let model = slot_params(input.model.as_ref());
        let provider_id = model
            .get("providerId")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, MODEL_MISSING))?;
        let model_id = model
            .get("modelId")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, MODEL_MISSING))?;
        let Some(providers) = &context.services.providers else {
            return Err(AppError::new(AppErrorCode::Conflict, PROVIDERS_UNAVAILABLE));
        };
        let driver = providers.text(provider_id)?;
        let mut offered = self.offered(&input.tools)?;
        let mut recall = self.recall(&input)?;
        let writes = recall.as_ref().is_some_and(|recall| recall.writes);
        let mode = if offered.is_empty() && !writes {
            ToolMode::None
        } else {
            resolve_tool_mode(driver.capabilities().tools, None)
        };
        if mode == ToolMode::None {
            offered.clear();
        } else if writes {
            offered.push(memory_tool(&offered));
        }
        let memory = match recall.as_mut() {
            Some(recall) => recall.section(
                offered
                    .iter()
                    .find(|tool| tool.key == MEMORY_TOOL)
                    .map(|tool| tool.name.as_str()),
            )?,
            None => String::new(),
        };
        let stores = store_ids(input.knowledge.as_ref());
        let searches = offered.iter().any(|tool| tool.key == KNOWLEDGE_SEARCH);
        let specs = tool_specs(&offered);
        let mut session = Session {
            by_name: offered
                .iter()
                .map(|tool| (tool.name.as_str(), tool))
                .collect(),
            stores,
            recall,
        };
        let system = [
            input.param_text("systemMessage").unwrap_or_default(),
            memory,
            if searches && !session.stores.is_empty() {
                KNOWLEDGE_INSTRUCTION.to_owned()
            } else {
                String::new()
            },
            if specs.is_empty() {
                String::new()
            } else {
                TOOL_RESULTS_INSTRUCTION.to_owned()
            },
        ]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
        let mut messages = Vec::new();
        if input.memory.is_some() {
            let size = slot_params(input.memory.as_ref())
                .get("size")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_MEMORY);
            messages.extend(history(&input.event, size));
        }
        messages.push(ChatMessage::user(text_of(&input.upstream())));
        let template = GenerateRequest {
            system: (!system.is_empty()).then_some(system),
            temperature: model.get("temperature").and_then(Value::as_f64),
            ..GenerateRequest::new(model_id, Vec::new())
        };
        let iterations = input
            .param_u64("maxIterations")
            .unwrap_or(DEFAULT_ITERATIONS)
            .clamp(1, 20);
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut calls: u64 = 0;
        let mut rounds: u64 = 0;
        loop {
            rounds += 1;
            let last = rounds >= iterations;
            if last && !specs.is_empty() && rounds > 1 {
                messages.push(ChatMessage::user(TOOL_BUDGET_NOTE));
            }
            let output = GenerateChatNode::generate_round(
                context,
                &driver,
                &GenerateRequest {
                    messages: messages.clone(),
                    tools: if last { Vec::new() } else { specs.clone() },
                    ..template.clone()
                },
                &text,
            )
            .await?;
            text.push_str(&output.text);
            reasoning.push_str(&output.reasoning);
            if output.tool_calls.is_empty() || last {
                break;
            }
            messages.push(
                ChatMessage::assistant(output.text.clone())
                    .with_tool_calls(output.tool_calls.clone()),
            );
            for call in output.tool_calls {
                calls += 1;
                let content = self.call(context, &mut session, &call, calls).await?;
                messages.push(ChatMessage::tool(call.id.clone(), content));
            }
        }
        Ok(AgentOutput {
            text,
            reasoning,
            tool_calls: calls,
            rounds,
        })
    }
}

impl AgentNode {
    async fn call(
        &self,
        context: &StepContext,
        session: &mut Session<'_>,
        call: &ToolCall,
        index: u64,
    ) -> Result<String, AppError> {
        let shape = |status: &str, text: String| fence_tool_result(&call.name, status, &text);
        let Some(tool) = session.by_name.get(call.name.as_str()).copied() else {
            return Ok(shape("refused", UNKNOWN_TOOL.to_owned()));
        };
        let mut arguments = parse_tool_arguments(&call.arguments).unwrap_or_default();
        if tool.key == MEMORY_TOOL {
            let Some(recall) = session.recall.as_mut() else {
                return Ok(shape("refused", UNKNOWN_TOOL.to_owned()));
            };
            return Ok(match recall.apply(&Value::Object(arguments)) {
                Ok(text) => shape("succeeded", text),
                Err(text) => shape("failed", text),
            });
        }
        if tool.key == KNOWLEDGE_SEARCH {
            arguments.insert("scope".to_owned(), json!(session.stores));
        }
        let Some(invocation) = context.invoke_tool(ToolStepRequest {
            key: tool.key.clone(),
            input: Value::Object(arguments),
            label: format!("call{index}"),
            minimum_tier: None,
            on_approval: None,
        }) else {
            return Ok(shape("refused", UNKNOWN_TOOL.to_owned()));
        };
        match invocation.await {
            Ok(result) => {
                let rendered = tool_result_text(&result.output);
                let shown = presented(&tool.key, rendered.text, session.by_name.values().copied());
                let capped = cap_text(&shown, result_limit(&tool.key, TOOL_RESULT_MAX_CHARS));
                Ok(shape(
                    if rendered.is_error {
                        "failed"
                    } else {
                        "succeeded"
                    },
                    capped.text,
                ))
            }
            Err(error) if error.code == AppErrorCode::RunCancelled => Err(error),
            Err(error) => {
                context.emit(HostEventDraft::Log {
                    line: log_line(&[
                        ("level", json!("warn")),
                        ("message", json!(error.message)),
                        ("tool", json!(tool.key)),
                    ]),
                })?;
                Ok(shape("failed", error.message))
            }
        }
    }
}

pub fn register_scenario_nodes(
    registry: &Arc<NodeRegistry>,
    options: &ScenarioNodeOptions,
) -> Result<(), AppError> {
    for trigger in [
        TRIGGER_CHAT,
        TRIGGER_MAIL,
        TRIGGER_SCHEDULE,
        TRIGGER_WEBHOOK,
    ] {
        registry.register(TriggerNode(trigger))?;
    }
    registry.register(AgentNode {
        nodes: Arc::downgrade(registry),
        tools: options.tools.clone(),
        memory: options.memory.clone(),
    })?;
    for (node_type, output) in [
        (OUTPUT_CHAT_REPLY, Output::ChatReply),
        (OUTPUT_CHAT_MESSAGE, Output::ChatMessage),
        (OUTPUT_NEW_CHAT, Output::NewChat),
        (OUTPUT_NOTIFY, Output::Notify),
        (OUTPUT_FILE, Output::File),
    ] {
        registry.register(OutputNode {
            node_type,
            output,
            effects: options.effects.clone(),
        })?;
    }
    Ok(())
}
