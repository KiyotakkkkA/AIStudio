use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::stream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ChatSendInput, ChatToolCallStatus, ConversationId, DeltaKind,
    HostEvent, HostEventDraft, ModelCapability, OfferedToolSource, ProviderCapability,
    RunHandleDto, RunListFilter, RunStatus, ThinkingEffort, ToolDto, ToolMode, ToolSource,
    ToolTier, Validate, VendorTurnOptions, WebSourceDto,
};

use super::{Harness, HarnessOptions, OneDriver, eventually, within};
use crate::data::Repositories;
use crate::data::repositories::ModelDraft;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::ai::tool_dialect::tests::counted_ids;
use crate::drivers::ai::tool_dialect::{
    DefaultDialect, INJECTED_CALL_KEY, INJECTED_RESULT_PREFIX, with_tool_dialect,
};
use crate::drivers::ai::{
    ChatMessage, ChatRole, DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult,
    TextDelta, TextGenerationDriver, ToolCall, ToolSpec,
};
use crate::kernel::types::CANCELLED_MESSAGE;
use crate::kernel::{
    FnNode, NodeRegistry, PermissionRequirement, StepContext, register_core_nodes,
};
use crate::services::chat_ask::{ASK_MALFORMED, ASK_ONCE, ASK_SKIPPED, ASK_TOOL};
use crate::services::chat_reply::{TOOL_CALL_DOMAIN, append_round, apply_tool_call};
use crate::services::chat_tools::{CHAT_TOOL_CALLS, TOOL_BUDGET_NOTE, TOOL_RESULTS_INSTRUCTION};
use crate::services::profile::TOOL_CALLS_KEY;
use crate::testing::{FAKE_CAPABILITIES, TemporaryDatabase, temporary_database};
use crate::tools::{ToolDescriptor, ToolRegistry, ToolRegistryOptions};

type Round = Box<dyn FnOnce(&GenerateRequest) -> Vec<TextDelta> + Send>;
type Fallback = Arc<dyn Fn(usize) -> Vec<TextDelta> + Send + Sync>;

pub(super) struct Scripted {
    requests: Mutex<Vec<GenerateRequest>>,
    rounds: Mutex<VecDeque<Round>>,
    fallback: Mutex<Fallback>,
    pub(super) mode: Mutex<ToolMode>,
    pub(super) excluded: Mutex<&'static [&'static str]>,
}

impl Scripted {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            rounds: Mutex::new(VecDeque::new()),
            fallback: Mutex::new(Arc::new(|_| vec![text("Done.")])),
            mode: Mutex::new(ToolMode::Native),
            excluded: Mutex::new(&[]),
        })
    }

    pub(super) fn round(
        &self,
        round: impl FnOnce(&GenerateRequest) -> Vec<TextDelta> + Send + 'static,
    ) {
        self.rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(Box::new(round));
    }

    pub(super) fn fallback(
        &self,
        fallback: impl Fn(usize) -> Vec<TextDelta> + Send + Sync + 'static,
    ) {
        *self.fallback.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(fallback);
    }

    pub(super) fn requests(&self) -> Vec<GenerateRequest> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl TextGenerationDriver for Scripted {
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            tools: *self.mode.lock().unwrap_or_else(PoisonError::into_inner),
            ..FAKE_CAPABILITIES
        }
    }

    async fn generate(
        &self,
        _request: &GenerateRequest,
        _cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        Err(AppError::new(AppErrorCode::Unknown, "not used"))
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        let count = {
            let mut requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
            requests.push(request.clone());
            requests.len()
        };
        let round = self
            .rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front();
        let deltas = match round {
            Some(round) => round(request),
            None => {
                let fallback =
                    Arc::clone(&self.fallback.lock().unwrap_or_else(PoisonError::into_inner));
                fallback(count)
            }
        };
        let cancel = cancel.clone();
        Box::pin(stream::iter(deltas.into_iter().map(move |delta| {
            if cancel.is_cancelled() {
                Err(crate::kernel::types::cancelled())
            } else {
                Ok(delta)
            }
        })))
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }

    fn excluded_policy_check_tools(&self) -> &'static [&'static str] {
        *self.excluded.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) fn text(value: &str) -> TextDelta {
    TextDelta::text(value)
}

pub(super) fn call(name: &str, arguments: &Value) -> TextDelta {
    call_with_id(name, arguments, &format!("call_{name}"))
}

pub(super) fn call_with_id(name: &str, arguments: &Value, id: &str) -> TextDelta {
    TextDelta::ToolCall(ToolCall::new(id, name, arguments.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
struct LookupInput {
    query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
struct TextOutput {
    text: String,
}

impl Validate for LookupInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

impl Validate for TextOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Default)]
struct Calls {
    lookups: Mutex<Vec<String>>,
    drops: Mutex<usize>,
    hang: AtomicBool,
    started: AtomicBool,
    inflight: AtomicUsize,
    peak: AtomicUsize,
}

impl Calls {
    fn lookups(&self) -> Vec<String> {
        self.lookups
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn drops(&self) -> usize {
        *self.drops.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Loop {
    harness: Harness,
    driver: Arc<Scripted>,
    tools: ToolRegistry,
    calls: Arc<Calls>,
    lookup: ToolDto,
    drop: ToolDto,
    _database: TemporaryDatabase,
}

fn register_demo_nodes(registry: &NodeRegistry, calls: &Arc<Calls>) {
    let lookups = Arc::clone(calls);
    registry
        .register(FnNode::new(
            "demo.lookup",
            PermissionRequirement::tool("demo.lookup"),
            false,
            move |context: StepContext, input: LookupInput| {
                let calls = Arc::clone(&lookups);
                async move {
                    calls
                        .lookups
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(input.query.clone());
                    if calls.hang.load(Ordering::SeqCst) {
                        calls.started.store(true, Ordering::SeqCst);
                        context.cancel.cancelled().await;
                        return Err(AppError::new(AppErrorCode::Unknown, "aborted"));
                    }
                    Ok(TextOutput {
                        text: format!("found {}", input.query),
                    })
                }
            },
        ))
        .expect("registers lookup");
    let drops = Arc::clone(calls);
    registry
        .register(FnNode::new(
            "demo.drop",
            PermissionRequirement::tool("demo.drop"),
            true,
            move |_context: StepContext, _input: Value| {
                let calls = Arc::clone(&drops);
                async move {
                    *calls.drops.lock().unwrap_or_else(PoisonError::into_inner) += 1;
                    Ok(TextOutput {
                        text: "dropped".to_owned(),
                    })
                }
            },
        ))
        .expect("registers drop");
    let slow = Arc::clone(calls);
    registry
        .register(FnNode::new(
            "demo.slow",
            PermissionRequirement::tool("demo.slow"),
            false,
            move |_context: StepContext, input: LookupInput| {
                let calls = Arc::clone(&slow);
                async move {
                    let now = calls.inflight.fetch_add(1, Ordering::SeqCst) + 1;
                    calls.peak.fetch_max(now, Ordering::SeqCst);
                    for _ in 0..2_000 {
                        if calls.peak.load(Ordering::SeqCst) >= 2 {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                    if input.query == "first" {
                        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                    }
                    calls.inflight.fetch_sub(1, Ordering::SeqCst);
                    Ok(TextOutput {
                        text: format!("slow {}", input.query),
                    })
                }
            },
        ))
        .expect("registers slow");
}

impl Loop {
    fn new() -> Self {
        Self::build(false)
    }

    fn web() -> Self {
        Self::build(true)
    }

    fn build(web: bool) -> Self {
        let database = temporary_database();
        let registry = Arc::new(NodeRegistry::new());
        register_core_nodes(&registry).expect("registers the core nodes");
        let calls = Arc::new(Calls::default());
        register_demo_nodes(&registry, &calls);
        let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
        let lookup = tools
            .register(&ToolDescriptor {
                description: "Look a fact up".to_owned(),
                input_schema: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
                read_only: true,
                ..ToolDescriptor::new("demo.lookup", ToolSource::Builtin)
            })
            .expect("registers lookup");
        let drop = tools
            .register(&ToolDescriptor {
                destructive: true,
                ..ToolDescriptor::new("demo.drop", ToolSource::Builtin)
            })
            .expect("registers drop");
        tools
            .register(&ToolDescriptor {
                description: "Look a fact up slowly".to_owned(),
                input_schema: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
                read_only: true,
                tier: Some(ToolTier::Auto),
                ..ToolDescriptor::new("demo.slow", ToolSource::Builtin)
            })
            .expect("registers slow");
        let driver = Scripted::new();
        let shared: SharedTextDriver = if web {
            *driver.mode.lock().unwrap_or_else(PoisonError::into_inner) = ToolMode::Injected;
            with_tool_dialect(
                Arc::clone(&driver) as SharedTextDriver,
                Arc::new(DefaultDialect),
                Some(counted_ids()),
            )
        } else {
            Arc::clone(&driver) as SharedTextDriver
        };
        let harness = Harness::with(HarnessOptions {
            database: Some(database.shared()),
            providers: Some(Arc::new(OneDriver(shared))),
            tools: Some(tools.clone()),
            mcp: Some(Arc::new(LiveServers(&[LIVE_SERVER]))),
            registry: Some(registry),
            capabilities: vec![ProviderCapability::Text],
            context_window: 16384,
            max_output: 512,
            ..HarnessOptions::default()
        });
        Self {
            harness,
            driver,
            tools,
            calls,
            lookup,
            drop,
            _database: database,
        }
    }

    fn conversation(&self, enabled: &[&str]) -> ConversationId {
        for tool in self
            .tools
            .list(&zvs_contract::ToolListFilter::default())
            .expect("lists the tools")
        {
            if !enabled.contains(&tool.key.as_str()) && tool.tier != ToolTier::Off {
                self.set_tier(&tool, ToolTier::Off);
            }
        }
        self.harness.create(self.harness.input())
    }

    async fn turn(&self, conversation: ConversationId, message: &str) -> RunHandleDto {
        within(self.harness.turn(conversation, message)).await
    }

    fn tool_steps(&self, handle: &RunHandleDto) -> Vec<zvs_contract::StepDto> {
        self.harness
            .runs
            .steps(&handle.id)
            .expect("steps")
            .into_iter()
            .filter(|step| step.node_id.starts_with("generate."))
            .collect()
    }

    fn set_tier(&self, tool: &ToolDto, tier: ToolTier) {
        self.tools
            .set_tier(&tool.id.to_string(), tier, "test")
            .expect("sets the tier");
    }
}

#[tokio::test]
async fn a_native_call_is_admitted_run_as_its_own_step_and_fed_back_fenced_before_the_answer() {
    let test = Loop::new();
    test.driver.round(|_| {
        vec![
            text("Checking."),
            call("demo__lookup", &json!({ "query": "x" })),
        ]
    });
    test.driver.fallback(|_| vec![text("It is x.")]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups(), ["x"]);
    let requests = test.driver.requests();
    assert_eq!(
        requests[0].tools,
        [
            ToolSpec {
                name: "demo__lookup".to_owned(),
                description: "Look a fact up".to_owned(),
                parameters: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
            },
            super::ask_spec()
        ]
    );
    assert!(
        requests[0]
            .system
            .as_deref()
            .is_some_and(|system| system.contains(TOOL_RESULTS_INSTRUCTION))
    );
    let tail = &requests[1].messages[requests[1].messages.len() - 2..];
    assert_eq!(
        tail,
        [
            ChatMessage::assistant("Checking.").with_tool_calls(vec![ToolCall::new(
                "call_demo__lookup",
                "demo__lookup",
                r#"{"query":"x"}"#
            )]),
            ChatMessage::tool(
                "call_demo__lookup",
                "<tool_result name=\"demo__lookup\" status=\"succeeded\">\nfound x\n</tool_result>"
            ),
        ]
    );
    let assistant = test.harness.last(created);
    assert_eq!(assistant.content, "Checking.\n\nIt is x.");
    assert!(!assistant.partial);
    assert_eq!(assistant.tool_calls.len(), 1);
    let record = &assistant.tool_calls[0];
    assert_eq!(record.key, "demo.lookup");
    assert_eq!(record.name, "demo__lookup");
    assert_eq!(record.status, ChatToolCallStatus::Succeeded);
    assert_eq!(record.arguments, json!({ "query": "x" }));
    assert_eq!(record.result.as_deref(), Some("found x"));
    assert_eq!(record.round, 0);
    let steps = test.tool_steps(&handle);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].node_id, "generate.call1");
    assert_eq!(steps[0].step_type, "demo.lookup");
    assert_eq!(steps[0].status, zvs_contract::StepStatus::Succeeded);
    assert_eq!(
        record.step_id.as_deref(),
        Some(steps[0].id.to_string().as_str())
    );
    assert_eq!(
        test.tools
            .get(&test.lookup.id.to_string())
            .expect("a tool")
            .tool
            .call_count,
        1
    );
    let page = test
        .harness
        .runs
        .list(&RunListFilter::default())
        .expect("lists");
    assert_eq!(page.items[0].progress.done, 3);
    assert_eq!(page.items[0].progress.total, 3);
    assert!(test.harness.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Step { step } if step.get("domain") == Some(&json!(TOOL_CALL_DOMAIN))
            && step.get("index") == Some(&json!(0))
            && step["call"]["id"] == json!(record.id)
    )));
}

#[tokio::test]
async fn only_tools_picked_for_the_conversation_and_not_off_are_offered() {
    let test = Loop::new();
    test.turn(test.conversation(&[]), "Question").await;
    let requests = test.driver.requests();
    assert_eq!(requests[0].tools, [super::ask_spec()]);
    test.set_tier(&test.lookup, ToolTier::Off);
    test.turn(test.conversation(&["demo.lookup"]), "Question")
        .await;
    assert_eq!(test.driver.requests()[1].tools, [super::ask_spec()]);
}

#[tokio::test]
async fn an_off_tool_is_refused_at_admission_even_when_it_was_offered_at_the_start_of_the_turn() {
    let test = Loop::new();
    let tools = test.tools.clone();
    let id = test.lookup.id.to_string();
    test.driver.round(move |_| {
        tools.set_tier(&id, ToolTier::Off, "test").expect("sets");
        vec![call("demo__lookup", &json!({ "query": "x" }))]
    });
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert!(test.calls.lookups().is_empty());
    let assistant = test.harness.last(created);
    assert_eq!(assistant.tool_calls[0].status, ChatToolCallStatus::Refused);
    assert_eq!(assistant.tool_calls[0].result, None);
    assert_eq!(
        test.tool_steps(&handle)[0].status,
        zvs_contract::StepStatus::Failed
    );
    let requests = test.driver.requests();
    assert!(
        requests[1]
            .messages
            .last()
            .is_some_and(|message| message.content.contains("Tool call refused"))
    );
    assert_eq!(assistant.content, "Done.");

    test.turn(created, "Question").await;
    assert_eq!(test.driver.requests()[2].tools, [super::ask_spec()]);
}

async fn awaiting_approval(test: &Loop, created: ConversationId) -> String {
    let mut approval = None;
    eventually(|| {
        approval = test
            .harness
            .last(created)
            .tool_calls
            .iter()
            .find(|record| record.status == ChatToolCallStatus::AwaitingApproval)
            .and_then(|record| record.approval_id.clone());
        approval.is_some()
    })
    .await;
    approval.expect("an approval id")
}

#[tokio::test]
async fn a_destructive_tool_asks_for_approval_even_at_tier_auto_and_runs_only_once_approved() {
    let test = Loop::new();
    test.set_tier(&test.drop, ToolTier::Auto);
    test.driver.round(|_| vec![call("demo__drop", &json!({}))]);
    let created = test.conversation(&["demo.drop"]);
    let handle = test.harness.send(created, "Drop it");

    let approval = within(awaiting_approval(&test, created)).await;
    assert_eq!(test.calls.drops(), 0);
    assert_eq!(test.harness.status(&handle), RunStatus::Blocked);
    let requested = test
        .harness
        .events()
        .into_iter()
        .find_map(|event| match event.body {
            HostEventDraft::Approval { request } => request.get("id").cloned(),
            _ => None,
        });
    assert_eq!(requested, Some(json!(approval)));
    test.harness
        .runs
        .approve(&handle.id, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.calls.drops(), 1);
    let record = &test.harness.last(created).tool_calls[0];
    assert_eq!(record.status, ChatToolCallStatus::Succeeded);
    assert_eq!(record.result.as_deref(), Some("dropped"));
}

#[tokio::test]
async fn a_denied_approval_is_reported_to_the_model_and_the_turn_still_answers() {
    let test = Loop::new();
    test.set_tier(&test.drop, ToolTier::Ask);
    test.driver.round(|_| vec![call("demo__drop", &json!({}))]);
    let created = test.conversation(&["demo.drop"]);
    let handle = test.harness.send(created, "Drop it");
    let approval = within(awaiting_approval(&test, created)).await;
    test.harness
        .runs
        .deny(&handle.id, &approval)
        .expect("denies");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.drops(), 0);
    let assistant = test.harness.last(created);
    assert_eq!(assistant.tool_calls[0].status, ChatToolCallStatus::Denied);
    assert_eq!(assistant.content, "Done.");
    assert!(
        test.driver.requests()[1]
            .messages
            .last()
            .is_some_and(|message| message.content.contains("Tool call denied"))
    );
}

fn assert_stops_after(test: &Loop, handle: &RunHandleDto, created: ConversationId, cap: usize) {
    assert_eq!(test.harness.status(handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups().len(), cap);
    let requests = test.driver.requests();
    assert_eq!(requests.len(), cap + 1);
    assert!(
        requests[cap]
            .messages
            .iter()
            .any(|message| message.content == TOOL_BUDGET_NOTE)
    );
    let assistant = test.harness.last(created);
    assert_eq!(assistant.tool_calls.len(), cap + 1);
    assert_eq!(
        assistant.tool_calls[cap].status,
        ChatToolCallStatus::Refused
    );
    assert_eq!(test.tool_steps(handle).len(), cap);
}

#[tokio::test]
async fn the_loop_stops_at_the_default_cap_and_answers_without_running_more_tools() {
    let test = Loop::new();
    test.driver
        .fallback(|count| vec![call("demo__lookup", &json!({ "query": count.to_string() }))]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_stops_after(&test, &handle, created, CHAT_TOOL_CALLS);
}

#[tokio::test]
async fn the_stored_tool_call_limit_caps_the_loop() {
    let test = Loop::new();
    test.harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection)
                .settings
                .set(TOOL_CALLS_KEY, "3", 2)?;
            Ok(())
        })
        .expect("a setting");
    test.driver
        .fallback(|count| vec![call("demo__lookup", &json!({ "query": count.to_string() }))]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_stops_after(&test, &handle, created, 3);
}

#[tokio::test]
async fn the_loop_stops_at_the_call_cap_across_rounds() {
    let test = Loop::new();
    test.driver.fallback(|count| {
        ["a", "b", "c"]
            .iter()
            .map(|query| {
                call_with_id(
                    "demo__lookup",
                    &json!({ "query": query }),
                    &format!("{query}{}", count - 1),
                )
            })
            .collect()
    });
    let created = test.conversation(&["demo.lookup"]);
    test.turn(created, "Question").await;

    assert_eq!(test.calls.lookups().len(), CHAT_TOOL_CALLS);
    let assistant = test.harness.last(created);
    assert_eq!(
        assistant
            .tool_calls
            .iter()
            .filter(|record| record.status == ChatToolCallStatus::Succeeded)
            .count(),
        CHAT_TOOL_CALLS
    );
    let requests = test.driver.requests();
    assert!(
        requests
            .last()
            .expect("a request")
            .messages
            .iter()
            .any(|message| message.content == TOOL_BUDGET_NOTE)
    );
}

#[tokio::test]
async fn cancelling_during_a_tool_call_leaves_a_consistent_partial_conversation() {
    let test = Loop::new();
    test.calls.hang.store(true, Ordering::SeqCst);
    test.driver.round(|_| {
        vec![
            text("Let me look."),
            call("demo__lookup", &json!({ "query": "x" })),
        ]
    });
    let created = test.conversation(&["demo.lookup"]);
    let handle = test.harness.send(created, "Question");
    eventually(|| test.calls.started.load(Ordering::SeqCst)).await;

    within(test.harness.chat.cancel(&handle.id))
        .await
        .expect("cancels");

    assert_eq!(test.harness.status(&handle), RunStatus::Cancelled);
    let assistant = test.harness.last(created);
    assert_eq!(assistant.role, zvs_contract::MessageRole::Assistant);
    assert!(assistant.partial);
    assert_eq!(assistant.content, "Let me look.");
    assert_eq!(assistant.tool_calls.len(), 1);
    assert_eq!(
        assistant.tool_calls[0].status,
        ChatToolCallStatus::Cancelled
    );
    assert_eq!(
        assistant.tool_calls[0].error.as_deref(),
        Some(CANCELLED_MESSAGE)
    );
    assert_eq!(
        test.tool_steps(&handle)[0].status,
        zvs_contract::StepStatus::Cancelled
    );

    test.calls.hang.store(false, Ordering::SeqCst);
    let next = test.turn(created, "Again").await;
    assert_eq!(test.harness.status(&next), RunStatus::Succeeded);
    let last = test.driver.requests().pop().expect("a request");
    assert!(
        last.messages
            .iter()
            .all(|message| message.role != ChatRole::Tool)
    );
    assert!(
        last.messages
            .iter()
            .all(|message| message.tool_calls.is_empty())
    );
}

#[tokio::test]
async fn a_call_to_a_tool_that_was_not_offered_is_refused_without_a_step() {
    let test = Loop::new();
    test.driver.round(|_| vec![call("demo__drop", &json!({}))]);
    let created = test.conversation(&["demo.lookup"]);
    let handle = test.turn(created, "Question").await;
    assert_eq!(test.calls.drops(), 0);
    let record = &test.harness.last(created).tool_calls[0];
    assert_eq!(record.key, "demo__drop");
    assert_eq!(record.status, ChatToolCallStatus::Refused);
    assert!(test.tool_steps(&handle).is_empty());
}

#[tokio::test]
async fn vendor_options_reach_the_driver_and_web_sources_of_every_round_are_kept() {
    let test = Loop::new();
    let source = |url: &str| WebSourceDto {
        url: url.to_owned(),
        title: url.to_owned(),
        snippet: String::new(),
        date: None,
        icon: None,
    };
    let first = source("https://a.example/");
    let second = source("https://b.example/");
    let found = (first.clone(), second.clone());
    test.driver.round(move |_| {
        vec![
            TextDelta::WebSources(vec![found.0]),
            call("demo__lookup", &json!({ "query": "x" })),
        ]
    });
    test.driver.fallback(move |_| {
        vec![
            TextDelta::WebSources(vec![found.1.clone()]),
            text("Итог [[2]]."),
        ]
    });
    let created = test.conversation(&["demo.lookup"]);
    let vendor = VendorTurnOptions {
        effort: Some(ThinkingEffort::Fast),
        web_search: true,
    };
    let handle = test
        .harness
        .chat
        .send(&ChatSendInput {
            vendor,
            ..ChatSendInput::new(created, "Курс")
        })
        .expect("sends");
    within(test.harness.runs.wait(&handle.id)).await;

    assert!(
        test.driver
            .requests()
            .iter()
            .all(|request| request.vendor == vendor)
    );
    let last = test.harness.last(created);
    assert_eq!(last.content, "Итог [[2]].");
    assert_eq!(last.web_sources, [first, second]);
    let user = &test.harness.messages(created)[0];
    assert!(user.web_sources.is_empty());
}

#[tokio::test]
async fn a_vendor_side_tool_call_is_left_to_the_vendor_instead_of_refused() {
    let test = Loop::new();
    *test
        .driver
        .excluded
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = &["web_search"];
    test.driver.round(|_| {
        vec![
            text("Курс 90,24."),
            call("web_search", &json!({ "queries": ["курс евро"] })),
        ]
    });
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Курс").await;

    let last = test.harness.last(created);
    assert_eq!(last.content, "Курс 90,24.");
    assert!(last.tool_calls.is_empty());
    assert_eq!(test.driver.requests().len(), 1);
    assert!(test.tool_steps(&handle).is_empty());
}

#[tokio::test]
async fn malformed_arguments_fail_the_call_without_running_the_tool() {
    let test = Loop::new();
    test.driver.round(|_| {
        vec![TextDelta::ToolCall(ToolCall::new(
            "bad",
            "demo__lookup",
            "{oops",
        ))]
    });
    let created = test.conversation(&["demo.lookup"]);
    test.turn(created, "Question").await;
    assert!(test.calls.lookups().is_empty());
    let record = &test.harness.last(created).tool_calls[0];
    assert_eq!(record.status, ChatToolCallStatus::Failed);
    assert_eq!(record.arguments, json!("{oops"));
}

#[tokio::test]
async fn a_web_driver_calls_the_tool_from_a_json_reply_that_never_reaches_the_user() {
    let test = Loop::web();
    test.driver.round(|_| {
        vec![
            text(&format!("{{\"{INJECTED_CALL_KEY}\": \"demo__")),
            text(r#"lookup", "arguments": {"query": "y"}}"#),
        ]
    });
    test.driver.fallback(|_| vec![text("It is y.")]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.calls.lookups(), ["y"]);
    let requests = test.driver.requests();
    assert!(requests[0].tools.is_empty());
    assert!(
        requests[0]
            .system
            .as_deref()
            .is_some_and(|system| system.contains("AVAILABLE TOOLS"))
    );
    assert_eq!(
        requests[1].messages.last(),
        Some(&ChatMessage::user(format!(
            "{INJECTED_RESULT_PREFIX} demo__lookup\n```\n<tool_result name=\"demo__lookup\" status=\"succeeded\">\nfound y\n</tool_result>\n```"
        )))
    );
    assert_eq!(test.harness.last(created).content, "It is y.");
    let tokens: String = test
        .harness
        .events()
        .into_iter()
        .filter(|event| event.stream_id == handle.stream_id)
        .filter_map(|event| match event.body {
            HostEventDraft::Token { delta, .. } => Some(delta),
            _ => None,
        })
        .collect();
    assert_eq!(tokens, "It is y.");
}

fn injected_call(name: &str, arguments: &Value) -> String {
    format!("{{\"{INJECTED_CALL_KEY}\": \"{name}\", \"arguments\": {arguments}}}\n")
}

#[tokio::test]
async fn a_web_driver_reads_three_files_in_one_round() {
    let test = Loop::web();
    test.driver.round(|_| {
        vec![
            text(&injected_call("demo__lookup", &json!({ "query": "a.md" }))),
            text(&injected_call("demo__lookup", &json!({ "query": "b.md" }))),
            text(&injected_call("demo__lookup", &json!({ "query": "c.md" }))),
        ]
    });
    test.driver.fallback(|_| vec![text("Three summaries.")]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Summarise a, b and c").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups(), ["a.md", "b.md", "c.md"]);
    let requests = test.driver.requests();
    assert_eq!(requests.len(), 2);
    let results: Vec<&str> = requests[1]
        .messages
        .iter()
        .filter(|message| message.content.starts_with(INJECTED_RESULT_PREFIX))
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(results.len(), 3);
    assert!(results[2].contains("found c.md"));
    let assistant = test.harness.last(created);
    assert_eq!(
        assistant
            .tool_calls
            .iter()
            .map(|record| (record.id.as_str(), record.round))
            .collect::<Vec<_>>(),
        [("call_1", 0), ("call_2", 0), ("call_3", 0)]
    );
    assert_eq!(assistant.content, "Three summaries.");
}

#[tokio::test]
async fn a_web_reply_with_a_read_and_a_write_runs_the_read_first_and_the_write_still_asks() {
    let test = Loop::web();
    test.set_tier(&test.drop, ToolTier::Ask);
    test.driver.round(|_| {
        vec![text(&format!(
            "{}{}",
            injected_call("demo__lookup", &json!({ "query": "x" })),
            injected_call("demo__drop", &json!({}))
        ))]
    });
    test.driver.fallback(|_| vec![text("Done.")]);
    let created = test.conversation(&["demo.lookup", "demo.drop"]);
    let handle = test.harness.send(created, "Look up and drop");

    let approval = within(awaiting_approval(&test, created)).await;
    assert_eq!(test.calls.lookups(), ["x"]);
    assert_eq!(test.calls.drops(), 0);
    test.harness
        .runs
        .approve(&handle.id, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.calls.drops(), 1);
    assert_eq!(test.driver.requests().len(), 2);
    assert_eq!(
        test.harness
            .last(created)
            .tool_calls
            .iter()
            .map(|record| (record.name.clone(), record.status))
            .collect::<Vec<_>>(),
        [
            ("demo__lookup".to_owned(), ChatToolCallStatus::Succeeded),
            ("demo__drop".to_owned(), ChatToolCallStatus::Succeeded)
        ]
    );
}

#[tokio::test]
async fn a_model_the_provider_reports_as_tool_less_is_not_offered_tools() {
    let test = Loop::new();
    let provider = test.harness.provider_id.to_string();
    test.harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).models.replace_for_provider(
                &provider,
                &[
                    ModelDraft {
                        context_window: Some(16384),
                        max_output: Some(512),
                        capabilities: vec![ModelCapability::Streaming],
                        ..ModelDraft::new("fake-model", "Fake", 1)
                    },
                    ModelDraft {
                        context_window: Some(16384),
                        capabilities: vec![ModelCapability::Streaming, ModelCapability::Tools],
                        ..ModelDraft::new("tool-model", "Tools", 1)
                    },
                ],
                1,
            )?;
            Ok(())
        })
        .expect("replaces the models");
    let handle = test
        .turn(test.conversation(&["demo.lookup"]), "Question")
        .await;
    assert!(test.driver.requests()[0].tools.is_empty());
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
}

#[tokio::test]
async fn context_counts_the_offered_tools_one_by_one() {
    let setup = Loop::new();
    let created = setup.conversation(&["demo.lookup"]);
    let measured = setup
        .harness
        .chat
        .context(&zvs_contract::ChatContextInput {
            conversation_id: Some(created),
            provider_id: setup.harness.provider_id,
            model_id: setup.harness.model_id.clone(),
            settings: None,
            system_prompt: None,
            command: None,
        })
        .expect("measures");
    assert_eq!(measured.tool_items.len(), 2);
    assert_eq!(measured.tool_items[0].key, "demo.lookup");
    assert_eq!(measured.tool_items[1].key, ASK_TOOL);
    assert!(measured.tool_items[0].tokens > 0);
    assert!(measured.tools > measured.tool_items[0].tokens);
}

const LIVE_SERVER: &str = "0199dd11-1111-7111-8111-000000000001";
const OFFLINE_SERVER: &str = "0199dd11-1111-7111-8111-000000000002";

struct LiveServers(&'static [&'static str]);

impl crate::services::McpServers for LiveServers {
    fn usable(&self) -> std::collections::HashSet<String> {
        self.0.iter().map(|id| (*id).to_owned()).collect()
    }
}

#[tokio::test]
async fn only_a_connected_mcp_server_offers_its_tools_and_they_bring_its_instructions() {
    let test = Loop::new();
    let server_id = LIVE_SERVER;
    test.harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).mcp_servers.insert(
                &crate::data::schema::McpServerRow {
                    id: server_id.to_owned(),
                    name: "Testix".to_owned(),
                    namespace: "testix".to_owned(),
                    transport: zvs_contract::McpTransport::Http,
                    command: None,
                    args: Vec::new(),
                    env: zvs_contract::McpEnv::new(),
                    url: Some("https://testix.example/mcp".to_owned()),
                    headers: zvs_contract::McpHeaders::new(),
                    enabled: true,
                    startup: zvs_contract::McpStartup::Manual,
                    status: zvs_contract::McpServerStatus::Connected,
                    protocol_version: None,
                    server_name: Some("Testix".to_owned()),
                    server_version: None,
                    instructions: Some(
                        "Read testix://instructions/questions before importing.".to_owned(),
                    ),
                    last_error: None,
                    last_connected_at: None,
                    created_at: 1,
                    updated_at: 1,
                },
            )?;
            Ok(())
        })
        .expect("adds a server");
    for (key, server) in [
        ("testix.importQuestions", server_id),
        ("testix.read_resource", server_id),
        ("offline.ping", OFFLINE_SERVER),
    ] {
        test.tools
            .register(&ToolDescriptor {
                source_ref: Some(server.to_owned()),
                read_only: true,
                ..ToolDescriptor::new(key, ToolSource::Mcp)
            })
            .expect("registers");
    }
    test.driver.fallback(|_| vec![text("Done.")]);
    test.turn(
        test.conversation(&[
            "testix.importQuestions",
            "testix.read_resource",
            "offline.ping",
        ]),
        "Import",
    )
    .await;
    let request = &test.driver.requests()[0];
    let names: Vec<&str> = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "testix__importQuestions",
            "testix__read_resource",
            "ask_user"
        ]
    );
    let system = request.system.clone().unwrap_or_default();
    assert!(system.contains("Read testix://instructions/questions before importing."));
    assert!(system.contains("«Testix»"));
}

struct OneSkill;

#[async_trait]
impl crate::skills::SkillCatalogue for OneSkill {
    async fn catalogue(
        &self,
        query: &str,
        _cancel: &CancellationToken,
    ) -> Vec<crate::skills::SkillCatalogueEntry> {
        assert_eq!(query, "Write the release notes");
        vec![crate::skills::SkillCatalogueEntry {
            key: "release-notes".to_owned(),
            description: "Writes release notes from a change list".to_owned(),
        }]
    }
}

#[tokio::test]
async fn a_turn_lists_the_skills_and_offers_skill_load_even_when_the_conversation_did_not_pick_it()
{
    let database = temporary_database();
    let registry = Arc::new(NodeRegistry::new());
    register_core_nodes(&registry).expect("registers the core nodes");
    registry
        .register(FnNode::new(
            crate::skills::SKILL_LOAD,
            PermissionRequirement::tool(crate::skills::SKILL_LOAD),
            false,
            |_context: StepContext, input: Value| async move {
                Ok(TextOutput {
                    text: format!(
                        "instructions for {}",
                        input["key"].as_str().unwrap_or_default()
                    ),
                })
            },
        ))
        .expect("registers skill.load");
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    for key in crate::skills::SKILL_TOOLS {
        tools
            .register(&ToolDescriptor {
                read_only: true,
                ..ToolDescriptor::new(key, ToolSource::Builtin)
            })
            .expect("registers a skill tool");
    }
    let driver = Scripted::new();
    driver.round(|_| vec![call("skill__load", &json!({ "key": "release-notes" }))]);
    driver.fallback(|_| vec![text("Notes.")]);
    let harness = Harness::with(HarnessOptions {
        database: Some(database.shared()),
        providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
        tools: Some(tools),
        skills: Some(Arc::new(OneSkill)),
        registry: Some(registry),
        capabilities: vec![ProviderCapability::Text],
        context_window: 16384,
        max_output: 512,
        ..HarnessOptions::default()
    });
    let created = harness.create(harness.input());

    let handle = within(harness.turn(created, "Write the release notes")).await;

    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let requests = driver.requests();
    let mut offered: Vec<&str> = requests[0]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    offered.sort_unstable();
    assert_eq!(
        offered,
        [
            "ask_user",
            "fs__apply",
            "fs__list",
            "fs__read",
            "shell__run",
            "skill__load"
        ]
    );
    let system = requests[0].system.clone().unwrap_or_default();
    assert!(system.contains("call skill__load with its key"), "{system}");
    assert!(system.contains(
        "<skills>
- release-notes: Writes release notes from a change list
</skills>"
    ));
    let assistant = harness.last(created);
    assert_eq!(assistant.tool_calls[0].key, crate::skills::SKILL_LOAD);
    assert_eq!(
        assistant.tool_calls[0].status,
        ChatToolCallStatus::Succeeded
    );
    assert_eq!(
        assistant.tool_calls[0].result.as_deref(),
        Some("instructions for release-notes")
    );
}

struct FirstMessageSkill;

#[async_trait]
impl crate::skills::SkillCatalogue for FirstMessageSkill {
    async fn catalogue(
        &self,
        query: &str,
        _cancel: &CancellationToken,
    ) -> Vec<crate::skills::SkillCatalogueEntry> {
        if query == "Write the release notes" {
            vec![crate::skills::SkillCatalogueEntry {
                key: "release-notes".to_owned(),
                description: "Writes release notes from a change list".to_owned(),
            }]
        } else {
            Vec::new()
        }
    }
}

#[tokio::test]
async fn every_turn_records_why_a_tool_was_offered_and_an_unknown_tool_is_answered_with_the_offered_names()
 {
    let database = temporary_database();
    let registry = Arc::new(NodeRegistry::new());
    register_core_nodes(&registry).expect("registers the core nodes");
    registry
        .register(FnNode::new(
            crate::skills::SKILL_LOAD,
            PermissionRequirement::tool(crate::skills::SKILL_LOAD),
            false,
            |_context: StepContext, _input: Value| async move {
                Ok(TextOutput {
                    text: "instructions".to_owned(),
                })
            },
        ))
        .expect("registers skill.load");
    registry
        .register(FnNode::new(
            crate::files::FS_READ,
            PermissionRequirement::tool(crate::files::FS_READ),
            false,
            |_context: StepContext, _input: Value| async move {
                Err::<TextOutput, _>(AppError::new(
                    AppErrorCode::PermissionDenied,
                    crate::files::roots::SKILL_READ_ONLY,
                ))
            },
        ))
        .expect("registers fs.read");
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    for key in crate::skills::SKILL_TOOLS {
        tools
            .register(&ToolDescriptor {
                read_only: true,
                ..ToolDescriptor::new(key, ToolSource::Builtin)
            })
            .expect("registers a skill tool");
    }
    let driver = Scripted::new();
    driver.round(|_| vec![call("skill__load", &json!({ "key": "release-notes" }))]);
    driver.round(|_| vec![text("Notes.")]);
    driver.round(|_| {
        vec![
            call("mail__send", &json!({ "to": "team" })),
            call("fs__read", &json!({ "paths": [{ "path": "C:/notes.md" }] })),
        ]
    });
    driver.fallback(|_| vec![text("Sent.")]);
    let harness = Harness::with(HarnessOptions {
        database: Some(database.shared()),
        providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
        tools: Some(tools),
        skills: Some(Arc::new(FirstMessageSkill)),
        registry: Some(registry),
        capabilities: vec![ProviderCapability::Text],
        context_window: 16384,
        max_output: 512,
        ..HarnessOptions::default()
    });
    let created = harness.create(harness.input());
    let first = within(harness.turn(created, "Write the release notes")).await;
    assert_eq!(harness.status(&first), RunStatus::Succeeded);

    let second = within(harness.turn(created, "Now send them to the team")).await;

    assert_eq!(harness.status(&second), RunStatus::Succeeded);
    let requests = driver.requests();
    let mut offered: Vec<&str> = requests[2]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    offered.sort_unstable();
    assert_eq!(
        offered,
        [
            "ask_user",
            "fs__apply",
            "fs__list",
            "fs__read",
            "shell__run",
            "skill__load"
        ]
    );
    let assistant = harness.last(created);
    let recorded: Vec<(&str, OfferedToolSource)> = assistant
        .offered_tools
        .iter()
        .map(|tool| (tool.key.as_str(), tool.source))
        .collect();
    assert_eq!(
        recorded,
        [
            ("fs.apply", OfferedToolSource::Base),
            ("fs.list", OfferedToolSource::Base),
            ("fs.read", OfferedToolSource::Base),
            ("shell.run", OfferedToolSource::Base),
            ("skill.load", OfferedToolSource::Base),
            (ASK_TOOL, OfferedToolSource::Base),
        ]
    );
    let refused = &assistant.tool_calls[0];
    assert_eq!(refused.status, ChatToolCallStatus::Refused);
    assert_eq!(
        refused.error.as_deref(),
        Some(crate::services::chat::NOT_OFFERED)
    );
    assert_eq!(
        refused.result.as_deref(),
        Some(
            "There is no tool named \"mail__send\" in this turn. The tools available now are: ask_user, fs__apply, fs__list, fs__read, shell__run, skill__load."
        )
    );
    let scoped = &assistant.tool_calls[1];
    assert_eq!(scoped.status, ChatToolCallStatus::Refused);
    assert_eq!(
        scoped.result.as_deref(),
        Some(crate::files::roots::SKILL_READ_ONLY)
    );
}

#[tokio::test]
async fn a_skill_script_waits_for_its_required_reading_and_the_hint_costs_no_budget() {
    let database = temporary_database();
    database
        .with_connection(|connection| {
            Repositories::new(connection)
                .settings
                .set(TOOL_CALLS_KEY, "4", 2)?;
            Ok(())
        })
        .expect("a setting");
    let registry = Arc::new(NodeRegistry::new());
    register_core_nodes(&registry).expect("registers the core nodes");
    registry
        .register(FnNode::new(
            crate::skills::SKILL_LOAD,
            PermissionRequirement::tool(crate::skills::SKILL_LOAD),
            false,
            |_context: StepContext, _input: Value| async move {
                Ok(
                    serde_json::from_value::<crate::skills::nodes::SkillLoadOutput>(json!({
                        "key": "dashboard",
                        "name": "Дашборды",
                        "folder": "skill:dashboard/",
                        "files": [],
                        "inlined": [],
                        "required": ["references/spec.md"],
                        "requires": { "scripts/build.py": ["references/spec.md"] },
                        "text": "Read references/spec.md with fs.read, then build with shell.run."
                    }))
                    .expect("a load output"),
                )
            },
        ))
        .expect("registers skill.load");
    registry
        .register(FnNode::new(
            crate::files::FS_READ,
            PermissionRequirement::tool(crate::files::FS_READ),
            false,
            |_context: StepContext, _input: Value| async move {
                Ok(TextOutput {
                    text: "spec".to_owned(),
                })
            },
        ))
        .expect("registers fs.read");
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    registry
        .register(FnNode::new(
            crate::commands::SHELL_RUN,
            PermissionRequirement::tool(crate::commands::SHELL_RUN),
            false,
            move |_context: StepContext, _input: Value| {
                let counted = Arc::clone(&counted);
                async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    Ok(TextOutput {
                        text: "built".to_owned(),
                    })
                }
            },
        ))
        .expect("registers shell.run");
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    for key in crate::skills::SKILL_TOOLS {
        tools
            .register(&ToolDescriptor {
                read_only: key != crate::commands::SHELL_RUN,
                tier: Some(ToolTier::Auto),
                ..ToolDescriptor::new(key, ToolSource::Builtin)
            })
            .expect("registers a skill tool");
    }
    let build = json!({ "argv": ["skill:dashboard/scripts/build.py", "_spec.json"] });
    let spec = json!({ "paths": [{ "path": "skill:dashboard/references/spec.md" }] });
    let driver = Scripted::new();
    driver.round(|_| vec![call("skill__load", &json!({ "key": "dashboard" }))]);
    let (first, second) = (build.clone(), spec.clone());
    driver.round(move |_| {
        vec![
            call_with_id("shell__run", &first, "early"),
            call_with_id("fs__read", &second, "read"),
            call_with_id("shell__run", &first, "build"),
        ]
    });
    driver.round(move |_| vec![call_with_id("fs__read", &spec, "again")]);
    driver.fallback(|_| vec![text("Built.")]);
    let harness = Harness::with(HarnessOptions {
        database: Some(database.shared()),
        providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
        tools: Some(tools),
        skills: Some(Arc::new(OneSkill)),
        registry: Some(registry),
        capabilities: vec![ProviderCapability::Text],
        context_window: 16384,
        max_output: 512,
        ..HarnessOptions::default()
    });
    let created = harness.create(harness.input());

    let handle = within(harness.turn(created, "Write the release notes")).await;

    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let calls = harness.last(created).tool_calls;
    assert_eq!(
        calls[0].result.as_deref(),
        Some("Read references/spec.md with fs__read, then build with shell__run.")
    );
    assert_eq!(calls[1].status, ChatToolCallStatus::Refused);
    assert_eq!(
        calls[1].error.as_deref(),
        Some(
            format!(
                "{}: skill:dashboard/references/spec.md",
                crate::skills::turn::READ_BEFORE_RUN
            )
            .as_str()
        )
    );
    assert_eq!(
        calls[2..]
            .iter()
            .map(|call| call.status)
            .collect::<Vec<_>>(),
        [ChatToolCallStatus::Succeeded; 3]
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn read_only_calls_of_one_round_overlap_and_return_in_call_order() {
    let test = Loop::new();
    test.driver.round(|_| {
        vec![
            call_with_id("demo__slow", &json!({ "query": "first" }), "call_a"),
            call_with_id("demo__slow", &json!({ "query": "second" }), "call_b"),
        ]
    });
    test.driver.fallback(|_| vec![text("Both found.")]);
    let created = test.conversation(&["demo.slow"]);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.peak.load(Ordering::SeqCst), 2);
    let requests = test.driver.requests();
    let replies: Vec<&str> = requests[1]
        .messages
        .iter()
        .filter(|message| message.role == ChatRole::Tool)
        .filter_map(|message| message.tool_call_id.as_deref())
        .collect();
    assert_eq!(replies, ["call_a", "call_b"]);
    let calls = test.harness.last(created).tool_calls;
    assert_eq!(
        calls
            .iter()
            .map(|call| call.id.as_str())
            .collect::<Vec<_>>(),
        ["call_a", "call_b"]
    );
    assert!(
        calls
            .iter()
            .all(|call| call.status == ChatToolCallStatus::Succeeded)
    );
}

fn format_question() -> Value {
    json!({ "questions": [{
        "title": "Формат?",
        "kind": "single",
        "options": [{ "label": "DOCX", "recommended": true }, { "label": "PDF" }]
    }] })
}

async fn awaiting_answer(test: &Loop, created: ConversationId) -> zvs_contract::QuestionRecordDto {
    let mut record = None;
    eventually(|| {
        record = test
            .harness
            .last(created)
            .tool_calls
            .first()
            .filter(|call| call.status == ChatToolCallStatus::AwaitingAnswer)
            .and_then(|call| call.meta.clone())
            .and_then(|meta| serde_json::from_value(meta).ok());
        record.is_some()
    })
    .await;
    record.expect("a question record")
}

#[tokio::test]
async fn asking_the_user_blocks_the_turn_and_the_answers_return_as_the_call_result() {
    let test = Loop::new();
    test.driver
        .round(|_| vec![call("ask_user", &format_question())]);
    test.driver.fallback(|_| vec![text("PDF it is.")]);
    let created = test.conversation(&[]);
    let handle = test.harness.send(created, "Make a report");

    let record = within(awaiting_answer(&test, created)).await;
    assert_eq!(test.harness.status(&handle), RunStatus::Blocked);
    assert!(record.answers.is_empty() && !record.skipped);
    assert_eq!(record.request.questions[0].options[0].label, "DOCX");
    let emitted = test
        .harness
        .events()
        .into_iter()
        .find_map(|event| match event.body {
            HostEventDraft::Question { request } => Some(request),
            _ => None,
        });
    assert_eq!(emitted.as_ref(), Some(&record.request));
    let questions = test.harness.runs.questions();
    assert_eq!(questions.pending(&handle.id.to_string()).len(), 1);
    questions
        .answer(
            &handle.id.to_string(),
            &record.request.id,
            &[zvs_contract::QuestionAnswerDto {
                question_id: "q1".to_owned(),
                selected: vec!["o2".to_owned()],
                text: None,
            }],
        )
        .expect("answers");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let assistant = test.harness.last(created);
    assert_eq!(assistant.content, "PDF it is.");
    let call = &assistant.tool_calls[0];
    assert_eq!(call.key, ASK_TOOL);
    assert_eq!(call.status, ChatToolCallStatus::Succeeded);
    let answered: zvs_contract::QuestionRecordDto =
        serde_json::from_value(call.meta.clone().expect("meta")).expect("a record");
    assert_eq!(answered.answers[0].selected, ["o2"]);
    assert!(test.tool_steps(&handle).is_empty());
    assert!(
        test.driver.requests()[1]
            .messages
            .last()
            .is_some_and(|message| message
                .content
                .contains("The user answered:\n1. Формат?\n   PDF"))
    );
}

#[tokio::test]
async fn a_skipped_question_lets_the_turn_continue_on_assumptions() {
    let test = Loop::new();
    test.driver
        .round(|_| vec![call("ask_user", &format_question())]);
    let created = test.conversation(&[]);
    let handle = test.harness.send(created, "Make a report");
    let record = within(awaiting_answer(&test, created)).await;
    test.harness
        .runs
        .questions()
        .skip(&handle.id.to_string(), &record.request.id)
        .expect("skips");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let call = &test.harness.last(created).tool_calls[0];
    let skipped: zvs_contract::QuestionRecordDto =
        serde_json::from_value(call.meta.clone().expect("meta")).expect("a record");
    assert!(skipped.skipped);
    assert!(
        test.driver.requests()[1]
            .messages
            .last()
            .is_some_and(|message| message.content.contains(ASK_SKIPPED))
    );
}

#[tokio::test]
async fn a_malformed_question_goes_back_to_the_model_and_a_second_ask_in_the_round_is_refused() {
    let test = Loop::new();
    test.driver.round(|_| {
        vec![
            call_with_id("ask_user", &json!({ "questions": [] }), "first"),
            call_with_id("ask_user", &format_question(), "second"),
            call("demo__lookup", &json!({ "query": "x" })),
        ]
    });
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let calls = test.harness.last(created).tool_calls;
    assert_eq!(calls[0].status, ChatToolCallStatus::Failed);
    assert_eq!(calls[0].error.as_deref(), Some(ASK_MALFORMED));
    assert_eq!(calls[1].status, ChatToolCallStatus::Refused);
    assert_eq!(calls[1].error.as_deref(), Some(ASK_ONCE));
    assert_eq!(calls[2].status, ChatToolCallStatus::Succeeded);
    assert_eq!(test.calls.lookups(), ["x"]);
    let request = &test.driver.requests()[1];
    assert!(request.messages.iter().any(|message| {
        message
            .content
            .contains("Invalid questions: questions breaks the minItems rule")
    }));
}

#[tokio::test]
async fn cancelling_while_a_question_waits_marks_the_call_cancelled() {
    let test = Loop::new();
    test.driver
        .round(|_| vec![call("ask_user", &format_question())]);
    let created = test.conversation(&[]);
    let handle = test.harness.send(created, "Make a report");
    within(awaiting_answer(&test, created)).await;

    within(test.harness.chat.cancel(&handle.id))
        .await
        .expect("cancels");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Cancelled);
    let call = &test.harness.last(created).tool_calls[0];
    assert_eq!(call.status, ChatToolCallStatus::Cancelled);
    assert!(
        test.harness
            .runs
            .questions()
            .pending(&handle.id.to_string())
            .is_empty()
    );
}

#[tokio::test]
async fn a_model_without_tool_calling_is_not_offered_the_question_bridge() {
    let test = Loop::new();
    *test
        .driver
        .mode
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = ToolMode::None;
    test.turn(test.conversation(&[]), "Question").await;
    assert!(test.driver.requests()[0].tools.is_empty());
    let system = test.driver.requests()[0].system.clone().unwrap_or_default();
    assert!(!system.contains(crate::services::chat_ask::ASK_HEADING));
}

fn attached(test: &Loop, created: ConversationId) -> Option<zvs_contract::ChatAttachDto> {
    test.harness.chat.attach(&created).expect("attaches")
}

#[tokio::test]
async fn attaching_without_a_live_turn_returns_nothing_and_a_finished_turn_is_not_live() {
    let test = Loop::new();
    let created = test.conversation(&["demo.lookup"]);
    assert_eq!(attached(&test, created), None);

    test.driver.fallback(|_| vec![text("Done.")]);
    test.turn(created, "Question").await;

    assert_eq!(attached(&test, created), None);
}

#[tokio::test]
async fn attaching_to_a_turn_waiting_on_an_approval_returns_the_text_the_calls_and_the_approval() {
    let test = Loop::new();
    test.set_tier(&test.drop, ToolTier::Ask);
    test.driver.round(|_| {
        vec![
            text("Сначала "),
            text("удаление."),
            call("demo__drop", &json!({})),
        ]
    });
    let created = test.conversation(&["demo.drop"]);
    let handle = test.harness.send(created, "Drop it");
    let approval = within(awaiting_approval(&test, created)).await;

    let mut live = None;
    within(eventually(|| {
        live = attached(&test, created).filter(|live| !live.approvals.is_empty());
        live.is_some()
    }))
    .await;
    let live = live.expect("a live turn");

    assert_eq!(live.run, handle);
    assert_eq!(live.content, "Сначала удаление.");
    assert_eq!(
        live.rounds,
        [zvs_contract::ChatRoundDto {
            text: "Сначала удаление.".to_owned(),
            reasoning: String::new(),
        }]
    );
    assert_eq!(live.approvals[0].get("id"), Some(&json!(approval)));
    assert_eq!(live.tool_calls.len(), 1);
    assert_eq!(
        live.tool_calls[0].approval_id.as_deref(),
        Some(approval.as_str())
    );
    assert!(live.questions.is_empty());
    let last = test
        .harness
        .runs
        .record(&handle.id.to_string())
        .expect("events")
        .last()
        .map(|event| event.seq);
    assert_eq!(live.seq, last);

    test.harness
        .runs
        .approve(&handle.id, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;
    assert_eq!(test.calls.drops(), 1);
    assert_eq!(attached(&test, created), None);
}

#[tokio::test]
async fn attaching_to_a_turn_waiting_on_a_question_returns_the_question() {
    let test = Loop::new();
    test.driver
        .round(|_| vec![call("ask_user", &format_question())]);
    test.driver.fallback(|_| vec![text("PDF it is.")]);
    let created = test.conversation(&[]);
    let handle = test.harness.send(created, "Make a report");
    let record = within(awaiting_answer(&test, created)).await;

    let mut live = None;
    within(eventually(|| {
        live = attached(&test, created).filter(|live| !live.questions.is_empty());
        live.is_some()
    }))
    .await;
    let live = live.expect("a live turn");

    assert_eq!(live.run, handle);
    assert_eq!(live.questions, std::slice::from_ref(&record.request));
    assert!(live.approvals.is_empty());

    test.harness
        .runs
        .questions()
        .skip(&handle.id.to_string(), &record.request.id)
        .expect("skips");
    within(test.harness.runs.wait(&handle.id)).await;
    assert_eq!(attached(&test, created), None);
}

#[derive(Debug, Default, PartialEq)]
struct Replayed {
    content: String,
    reasoning: String,
    rounds: Vec<zvs_contract::ChatRoundDto>,
    tool_calls: Vec<zvs_contract::ChatToolCallDto>,
    steps: Vec<serde_json::Map<String, Value>>,
}

fn replayed(events: &[HostEvent]) -> Replayed {
    let mut state = Replayed::default();
    for event in events {
        match &event.body {
            HostEventDraft::Token { delta, kind } => {
                match kind {
                    Some(DeltaKind::Reasoning) => state.reasoning.push_str(delta),
                    _ => state.content.push_str(delta),
                }
                append_round(&mut state.rounds, &state.tool_calls, *kind, delta);
            }
            HostEventDraft::Step { step }
                if step.get("domain") == Some(&json!(TOOL_CALL_DOMAIN)) =>
            {
                let index = step["index"].as_u64().expect("an index") as usize;
                let call = serde_json::from_value(step["call"].clone()).expect("a call");
                apply_tool_call(&mut state.tool_calls, index, call);
            }
            HostEventDraft::Step { step } if step.contains_key("attempt") => {
                state.steps.push(step.clone());
            }
            _ => {}
        }
    }
    state
}

#[tokio::test]
async fn a_recorded_turn_with_tool_calls_replays_to_the_live_state_from_a_lean_log() {
    let test = Loop::new();
    test.driver.round(|_| {
        let mut deltas: Vec<TextDelta> = (0..40)
            .map(|index| TextDelta::Content {
                text: format!("мысль{index} "),
                kind: DeltaKind::Reasoning,
            })
            .collect();
        deltas.push(text("Поиск."));
        for index in 0..3 {
            deltas.push(call_with_id(
                "demo__lookup",
                &json!({ "query": format!("{index}{}", "x".repeat(1500)) }),
                &format!("call_{index}"),
            ));
        }
        deltas
    });
    test.driver.round(|_| {
        (0..400)
            .map(|index| text(&format!("слово{index} ")))
            .collect()
    });
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    let live: Vec<HostEvent> = test
        .harness
        .events()
        .into_iter()
        .filter(|event| event.stream_id == handle.stream_id)
        .collect();
    let record = test
        .harness
        .runs
        .record(&handle.id.to_string())
        .expect("events");
    let (rows, bytes): (i64, i64) = test
        .harness
        .database
        .with_connection(|connection| {
            Ok(connection.query_row(
                "SELECT COUNT(*), SUM(LENGTH(event)) FROM run_event WHERE run_id = ?1",
                [handle.id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .expect("measures");
    let live_bytes: usize = live
        .iter()
        .map(|event| serde_json::to_string(event).expect("serialises").len())
        .sum();
    eprintln!(
        "run_event: {rows} rows, {bytes} bytes; live: {} events, {live_bytes} bytes",
        live.len()
    );

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let assistant = test.harness.last(created);
    let state = replayed(&live);
    assert_eq!(replayed(&record), state);
    assert_eq!(state.tool_calls, assistant.tool_calls);
    assert_eq!(state.content, assistant.content);
    assert_eq!(state.reasoning, assistant.reasoning);
    assert_eq!(state.rounds, assistant.rounds);
    assert_eq!(assistant.rounds.len(), 2);
    assert_eq!(assistant.rounds[0].text, "Поиск.");
    assert!(assistant.rounds[0].reasoning.starts_with("мысль0 "));
    assert!(assistant.rounds[1].reasoning.is_empty());
    assert_eq!(
        assistant
            .rounds
            .iter()
            .map(|round| round.text.as_str())
            .collect::<String>(),
        assistant.content
    );
    assert!(rows * 10 < live.len() as i64, "{rows} of {}", live.len());
    assert!(
        usize::try_from(bytes).expect("bytes") * 2 < live_bytes,
        "{bytes} of {live_bytes}"
    );
}

fn bulk_tools(test: &Loop) {
    for index in 0..12 {
        test.tools
            .register(&ToolDescriptor {
                description: format!("Bulk tool {index}. {}", "Описание. ".repeat(100)),
                read_only: true,
                ..ToolDescriptor::new(format!("bulk.tool{index}"), ToolSource::Builtin)
            })
            .expect("registers a bulk tool");
    }
}

fn names(request: &GenerateRequest) -> Vec<&str> {
    request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect()
}

#[tokio::test]
async fn a_large_catalogue_is_deferred_and_a_searched_tool_is_callable_from_the_next_step_and_stays()
 {
    let test = Loop::new();
    bulk_tools(&test);
    test.driver
        .round(|_| vec![call("tool_search", &json!({ "names": ["demo__lookup"] }))]);
    test.driver
        .round(|_| vec![call("demo__lookup", &json!({ "query": "x" }))]);
    test.driver.round(|_| vec![text("It is x.")]);
    test.driver.fallback(|_| vec![text("Again x.")]);
    let mut enabled: Vec<String> = (0..12).map(|index| format!("bulk.tool{index}")).collect();
    enabled.push("demo.lookup".to_owned());
    let keys: Vec<&str> = enabled.iter().map(String::as_str).collect();
    let created = test.conversation(&keys);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups(), ["x"]);
    let requests = test.driver.requests();
    assert_eq!(names(&requests[0]), ["ask_user", "tool_search"]);
    let system = requests[0].system.clone().unwrap_or_default();
    assert!(system.contains(crate::services::chat_tools::DEFERRED_HEADING));
    assert!(system.contains("- demo__lookup: "));
    assert!(system.contains("- bulk__tool7: Bulk tool 7."));
    assert_eq!(
        names(&requests[1]),
        ["ask_user", "tool_search", "demo__lookup"]
    );
    let assistant = test.harness.last(created);
    let search = &assistant.tool_calls[0];
    assert_eq!(search.key, crate::services::chat_tools::TOOL_SEARCH);
    assert_eq!(search.status, ChatToolCallStatus::Succeeded);
    assert_eq!(search.meta, Some(json!({ "loaded": ["demo.lookup"] })));
    assert!(
        search
            .result
            .as_deref()
            .is_some_and(|result| result.contains("\"name\":\"demo__lookup\""))
    );
    assert_eq!(
        assistant.tool_calls[1].status,
        ChatToolCallStatus::Succeeded
    );
    assert!(
        assistant
            .offered_tools
            .contains(&zvs_contract::OfferedToolDto {
                key: "demo.lookup".to_owned(),
                source: OfferedToolSource::Loaded,
            })
    );

    test.turn(created, "Once more").await;

    let requests = test.driver.requests();
    assert_eq!(
        names(&requests[3]),
        ["demo__lookup", "ask_user", "tool_search"]
    );
}

#[tokio::test]
async fn a_deferred_tool_called_by_name_runs_without_a_search() {
    let test = Loop::new();
    bulk_tools(&test);
    test.driver
        .round(|_| vec![call("demo__lookup", &json!({ "query": "y" }))]);
    test.driver.fallback(|_| vec![text("It is y.")]);
    let mut enabled: Vec<String> = (0..12).map(|index| format!("bulk.tool{index}")).collect();
    enabled.push("demo.lookup".to_owned());
    let keys: Vec<&str> = enabled.iter().map(String::as_str).collect();
    let created = test.conversation(&keys);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups(), ["y"]);
    assert_eq!(
        names(&test.driver.requests()[1]),
        ["ask_user", "tool_search", "demo__lookup"]
    );
}

struct BrowserSkill;

#[async_trait]
impl crate::skills::SkillCatalogue for BrowserSkill {
    async fn catalogue(
        &self,
        _query: &str,
        _cancel: &CancellationToken,
    ) -> Vec<crate::skills::SkillCatalogueEntry> {
        Vec::new()
    }

    fn command(&self, key: &str) -> Option<String> {
        (key == "browser-use").then(|| "Open the page with browser.navigate first.".to_owned())
    }
}

#[tokio::test]
async fn the_browser_command_brings_the_browser_tools_and_its_skill_for_the_rest_of_the_dialog() {
    let database = temporary_database();
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    for key in crate::services::chat_tools::BROWSER_TOOLS {
        tools
            .register(&ToolDescriptor {
                read_only: true,
                ..ToolDescriptor::new(key, ToolSource::Builtin)
            })
            .expect("registers a browser tool");
    }
    let driver = Scripted::new();
    driver.fallback(|_| vec![text("Done.")]);
    let harness = Harness::with(HarnessOptions {
        database: Some(database.shared()),
        providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
        tools: Some(tools),
        skills: Some(Arc::new(BrowserSkill)),
        capabilities: vec![ProviderCapability::Text],
        context_window: 16384,
        max_output: 512,
        ..HarnessOptions::default()
    });
    assert_eq!(
        harness
            .chat
            .commands()
            .iter()
            .map(|command| command.command)
            .collect::<Vec<_>>(),
        [zvs_contract::ChatCommand::BrowserUse]
    );
    let created = harness.create(harness.input());

    within(harness.turn(created, "Hello")).await;
    within(harness.command_turn(
        created,
        "Open the schedule",
        zvs_contract::ChatCommand::BrowserUse,
    ))
    .await;
    within(harness.turn(created, "And the next page")).await;

    let requests = driver.requests();
    assert_eq!(names(&requests[0]), ["ask_user"]);
    for request in &requests[1..] {
        assert!(names(request).contains(&"browser__navigate"));
        assert!(names(request).contains(&"browser__submit"));
        let system = request.system.clone().unwrap_or_default();
        assert!(system.contains(
            "Instructions of the command /browser-use:\nOpen the page with browser__navigate first."
        ));
    }
    let messages = harness.messages(created);
    assert_eq!(messages[0].command, None);
    assert_eq!(
        messages[2].command,
        Some(zvs_contract::ChatCommand::BrowserUse)
    );
    assert!(
        messages[5]
            .offered_tools
            .contains(&zvs_contract::OfferedToolDto {
                key: "browser.navigate".to_owned(),
                source: OfferedToolSource::Command,
            })
    );
}

#[tokio::test]
async fn vendor_usage_replaces_the_estimate_and_the_window_reaches_the_context_meter() {
    let test = Loop::new();
    test.driver.round(|_| {
        vec![
            call("demo__lookup", &json!({ "query": "x" })),
            TextDelta::Usage(crate::drivers::ai::TurnUsage {
                input_tokens: Some(500),
                output_tokens: Some(40),
                context_window: Some(32_768),
            }),
        ]
    });
    test.driver.fallback(|_| {
        vec![
            text("Готово."),
            TextDelta::Usage(crate::drivers::ai::TurnUsage {
                input_tokens: Some(700),
                output_tokens: Some(20),
                context_window: None,
            }),
        ]
    });
    let created = test.conversation(&["demo.lookup"]);
    let handle = test.turn(created, "Вопрос").await;
    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);

    let last = test.harness.last(created);
    assert_eq!(last.content, "Готово.");
    assert_eq!((last.tokens_in, last.tokens_out), (1200, 60));
    assert!(!last.usage_estimated);

    let measured = test
        .harness
        .chat
        .context(&zvs_contract::ChatContextInput {
            conversation_id: Some(created),
            provider_id: test.harness.provider_id,
            model_id: test.harness.model_id.clone(),
            settings: None,
            system_prompt: None,
            command: None,
        })
        .expect("measures");
    assert_eq!(measured.context_window, 32_768);
}

#[tokio::test]
async fn a_turn_without_vendor_usage_keeps_the_estimate() {
    let test = Loop::new();

    let created = test.conversation(&[]);
    test.turn(created, "Вопрос").await;
    let last = test.harness.last(created);
    assert!(last.usage_estimated);
    assert!(last.tokens_out > 0);
}
