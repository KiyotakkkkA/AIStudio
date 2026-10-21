use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::stream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ChatToolCallStatus, ConversationId, CreateConversationInput,
    HostEventDraft, ModelCapability, ProviderCapability, RunHandleDto, RunListFilter, RunStatus,
    ToolDto, ToolMode, ToolSource, ToolTier, Validate,
};

use super::{Harness, HarnessOptions, OneDriver, eventually, within};
use crate::data::Repositories;
use crate::data::repositories::ModelDraft;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::injected_tools::{INJECTED_CALL_KEY, INJECTED_RESULT_PREFIX};
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::ai::{
    ChatMessage, ChatRole, DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult,
    TextDelta, TextGenerationDriver, ToolCall, ToolSpec,
};
use crate::kernel::types::CANCELLED_MESSAGE;
use crate::kernel::{
    FnNode, NodeRegistry, PermissionRequirement, StepContext, register_core_nodes,
};
use crate::services::chat_tools::{
    CHAT_TOOL_CALLS, CHAT_TOOL_ROUNDS, TOOL_BUDGET_NOTE, TOOL_RESULTS_INSTRUCTION,
};
use crate::testing::{FAKE_CAPABILITIES, TemporaryDatabase, temporary_database};
use crate::tools::{ToolDescriptor, ToolRegistry, ToolRegistryOptions};

type Round = Box<dyn FnOnce(&GenerateRequest) -> Vec<TextDelta> + Send>;
type Fallback = Arc<dyn Fn(usize) -> Vec<TextDelta> + Send + Sync>;

pub(super) struct Scripted {
    requests: Mutex<Vec<GenerateRequest>>,
    rounds: Mutex<VecDeque<Round>>,
    fallback: Mutex<Fallback>,
    pub(super) mode: Mutex<ToolMode>,
}

impl Scripted {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            rounds: Mutex::new(VecDeque::new()),
            fallback: Mutex::new(Arc::new(|_| vec![text("Done.")])),
            mode: Mutex::new(ToolMode::Native),
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
}

impl Loop {
    fn new() -> Self {
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
        let driver = Scripted::new();
        let harness = Harness::with(HarnessOptions {
            database: Some(database.shared()),
            providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
            tools: Some(tools.clone()),
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
        self.harness.create(CreateConversationInput {
            enabled_tools: Some(enabled.iter().map(|key| (*key).to_owned()).collect()),
            ..self.harness.input()
        })
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
        [ToolSpec {
            name: "demo__lookup".to_owned(),
            description: "Look a fact up".to_owned(),
            parameters: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
        }]
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
        HostEventDraft::Step { step } if step.get("domain") == Some(&json!("tool-calls"))
    )));
}

#[tokio::test]
async fn only_tools_picked_for_the_conversation_and_not_off_are_offered() {
    let test = Loop::new();
    test.turn(test.conversation(&[]), "Question").await;
    let requests = test.driver.requests();
    assert!(requests[0].tools.is_empty());
    assert!(
        !requests[0]
            .system
            .clone()
            .unwrap_or_default()
            .contains(TOOL_RESULTS_INSTRUCTION)
    );
    test.set_tier(&test.lookup, ToolTier::Off);
    test.turn(test.conversation(&["demo.lookup"]), "Question")
        .await;
    assert!(test.driver.requests()[1].tools.is_empty());
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
    assert!(test.driver.requests()[2].tools.is_empty());
}

async fn awaiting_approval(test: &Loop, created: ConversationId) -> String {
    let mut approval = None;
    eventually(|| {
        approval = test
            .harness
            .last(created)
            .tool_calls
            .first()
            .filter(|record| record.status == ChatToolCallStatus::AwaitingApproval)
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

#[tokio::test]
async fn the_loop_stops_at_the_round_cap_and_answers_without_running_more_tools() {
    let test = Loop::new();
    test.driver
        .fallback(|count| vec![call("demo__lookup", &json!({ "query": count.to_string() }))]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.turn(created, "Question").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups().len(), CHAT_TOOL_ROUNDS);
    let requests = test.driver.requests();
    assert_eq!(requests.len(), CHAT_TOOL_ROUNDS + 1);
    assert!(
        requests[CHAT_TOOL_ROUNDS]
            .messages
            .iter()
            .any(|message| message.content == TOOL_BUDGET_NOTE)
    );
    let assistant = test.harness.last(created);
    assert_eq!(assistant.tool_calls.len(), CHAT_TOOL_ROUNDS + 1);
    assert_eq!(
        assistant.tool_calls[CHAT_TOOL_ROUNDS].status,
        ChatToolCallStatus::Refused
    );
    assert_eq!(test.tool_steps(&handle).len(), CHAT_TOOL_ROUNDS);
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
async fn an_injected_adapter_calls_the_tool_from_a_json_reply_that_never_reaches_the_user() {
    let test = Loop::new();
    *test
        .driver
        .mode
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = ToolMode::Injected;
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
            "{INJECTED_RESULT_PREFIX} demo__lookup\n```\nfound y\n```"
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
async fn conversation_tool_selection_is_validated_and_persisted() {
    let test = Loop::new();
    let created = test.conversation(&[]);
    assert_eq!(
        test.harness
            .chat
            .update_tools(
                &created,
                &["demo.lookup".to_owned(), "demo.lookup".to_owned()]
            )
            .expect("updates")
            .enabled_tools,
        ["demo.lookup"]
    );
    assert_eq!(
        test.harness
            .chat
            .get(&created)
            .expect("a conversation")
            .conversation
            .enabled_tools,
        ["demo.lookup"]
    );
    assert_eq!(
        test.harness
            .chat
            .update_tools(&created, &["demo.missing".to_owned()])
            .expect_err("unknown")
            .code,
        AppErrorCode::NotFound
    );
    assert_eq!(
        test.harness
            .chat
            .create(&CreateConversationInput {
                enabled_tools: Some(vec!["demo.missing".to_owned()]),
                ..test.harness.input()
            })
            .expect_err("unknown")
            .code,
        AppErrorCode::NotFound
    );
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
            enabled_tools: None,
        })
        .expect("measures");
    assert_eq!(measured.tool_items.len(), 1);
    assert_eq!(measured.tool_items[0].key, "demo.lookup");
    assert!(measured.tool_items[0].tokens > 0);
    assert!(measured.tools > measured.tool_items[0].tokens);

    let none = setup
        .harness
        .chat
        .context(&zvs_contract::ChatContextInput {
            conversation_id: Some(created),
            provider_id: setup.harness.provider_id,
            model_id: setup.harness.model_id.clone(),
            settings: None,
            system_prompt: None,
            enabled_tools: Some(Vec::new()),
        })
        .expect("measures without tools");
    assert_eq!((none.tools, none.tool_items.len()), (0, 0));
}

#[tokio::test]
async fn an_offered_mcp_tool_brings_its_server_instructions_and_resource_reader() {
    let test = Loop::new();
    let server_id = "0199dd11-1111-7111-8111-000000000001";
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
    for key in ["testix.importQuestions", "testix.read_resource"] {
        test.tools
            .register(&ToolDescriptor {
                source_ref: Some(server_id.to_owned()),
                read_only: true,
                ..ToolDescriptor::new(key, ToolSource::Mcp)
            })
            .expect("registers");
    }
    test.driver.fallback(|_| vec![text("Done.")]);
    test.turn(test.conversation(&["testix.importQuestions"]), "Import")
        .await;
    let request = &test.driver.requests()[0];
    let names: Vec<&str> = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(names, ["testix__importQuestions", "testix__read_resource"]);
    let system = request.system.clone().unwrap_or_default();
    assert!(system.contains("Read testix://instructions/questions before importing."));
    assert!(system.contains("«Testix»"));
}
