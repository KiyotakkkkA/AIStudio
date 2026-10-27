use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value, json};
use tokio::sync::Notify;
use zvs_contract::{
    AppError, AppErrorCode, CreateScenarioInput, GraphBinding, HostEventDraft, RunId, RunStatus,
    ScenarioAttachmentDto, ScenarioDetailDto, ScenarioEdgeDto, ScenarioGraphDto, ScenarioNodeDto,
    ScenarioNodeSettingsDto, ScenarioOnError, ScenarioOverlap, ScenarioPositionDto,
    ScenarioRetryStepInput, ScenarioRunInput, ScenarioRunNodeInput, ScenarioSlot,
    ScenarioTriggerKind, SetScenarioActiveInput, StepStatus, UpdateScenarioInput,
};

use super::catalogue::{
    AGENT_TOOLS, FLOW_BRANCH, KNOWLEDGE_STORES, MEMORY_FACTS, MEMORY_WINDOW, MODEL_CHAT,
    OUTPUT_CHAT_REPLY, TOOL_SET, TRIGGER_CHAT,
};
use super::compile::{EVENT_KEY, PINNED_SOURCE, compile, compile_node, from_key};
use super::nodes::{EffectsSlot, ScenarioEffects, ScenarioNodeOptions, register_scenario_nodes};
use super::runner::{Fired, ScenarioRunner, ScenarioRunnerOptions};
use super::service::{SCENARIO_INVALID, ScenarioService, ScenarioServiceOptions};
use crate::data::schema::StepRow;
use crate::kernel::{FnNode, NodeRegistry, PermissionRequirement};
use crate::platform::events::EventBus;
use crate::services::{RunService, RunServiceOptions};
use crate::testing::{CLOCK_ORIGIN, MemorySink, TemporaryDatabase, TokioClock, temporary_database};

const UPPER: &str = "test.upper";
const FAIL: &str = "test.fail";
const WAIT: &str = "test.wait";
const SEARCH: &str = "test.search";
const BROKEN: &str = "Сервис не ответил";

#[derive(Default)]
struct Effects(Mutex<Vec<(String, String)>>);

impl Effects {
    fn posted(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ScenarioEffects for Effects {
    fn post_message(&self, conversation_id: &str, text: &str) -> Result<Value, AppError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((conversation_id.to_owned(), text.to_owned()));
        Ok(json!({ "conversationId": conversation_id }))
    }

    fn create_conversation(&self, _title: &str, _text: &str) -> Result<Value, AppError> {
        Ok(Value::Null)
    }

    fn notify(&self, _title: &str, _body: &str) -> Result<(), AppError> {
        Ok(())
    }

    fn write_file(&self, _path: &str, _text: &str) -> Result<Value, AppError> {
        Ok(Value::Null)
    }
}

struct Rig {
    _database: TemporaryDatabase,
    runs: RunService,
    scenarios: ScenarioService,
    runner: ScenarioRunner,
    effects: Arc<Effects>,
    gate: Arc<Notify>,
    sink: Arc<MemorySink>,
}

fn rig() -> Rig {
    let database = temporary_database();
    let nodes = Arc::new(NodeRegistry::new());
    let slot = EffectsSlot::default();
    register_scenario_nodes(
        &nodes,
        &ScenarioNodeOptions {
            effects: slot.clone(),
            tools: None,
            memory: None,
        },
    )
    .expect("registers the scenario nodes");
    nodes
        .register(crate::kernel::nodes::BranchNode)
        .expect("registers the branch");
    nodes
        .register(FnNode::new(
            UPPER,
            PermissionRequirement::None,
            false,
            |_context, input: Value| async move {
                let text = input["text"].as_str().unwrap_or_default().to_uppercase();
                Ok(json!({ "text": text }))
            },
        ))
        .expect("registers");
    nodes
        .register(FnNode::new(
            FAIL,
            PermissionRequirement::None,
            false,
            |_context, _input: Value| async move {
                Err::<Value, _>(AppError::new(AppErrorCode::RunFailed, BROKEN))
            },
        ))
        .expect("registers");
    let gate = Arc::new(Notify::new());
    let waiting = Arc::clone(&gate);
    nodes
        .register(FnNode::new(
            WAIT,
            PermissionRequirement::None,
            false,
            move |_context, input: Value| {
                let gate = Arc::clone(&waiting);
                async move {
                    gate.notified().await;
                    Ok(input)
                }
            },
        ))
        .expect("registers");
    nodes
        .register(FnNode::new(
            SEARCH,
            PermissionRequirement::tool(SEARCH),
            false,
            |_context, _input: Value| async move { Ok(json!([])) },
        ))
        .expect("registers");
    let sink = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(sink.clone());
    let runs = RunService::new(RunServiceOptions {
        clock: Arc::new(TokioClock::new(CLOCK_ORIGIN)),
        ..RunServiceOptions::new(database.shared(), bus, Arc::clone(&nodes))
    })
    .expect("a run service");
    let mut options = ScenarioServiceOptions::new(database.shared(), Arc::clone(&nodes));
    options
        .actions
        .extend([UPPER, FAIL, WAIT, SEARCH].map(str::to_owned));
    let scenarios = ScenarioService::new(options);
    let runner = ScenarioRunner::new(ScenarioRunnerOptions::new(scenarios.clone(), runs.clone()));
    let effects = Arc::new(Effects::default());
    slot.attach(effects.clone());
    Rig {
        _database: database,
        runs,
        scenarios,
        runner,
        effects,
        gate,
        sink,
    }
}

impl Rig {
    fn create(&self, graph: ScenarioGraphDto) -> ScenarioDetailDto {
        let created = self
            .scenarios
            .create(&CreateScenarioInput {
                name: "Триаж".to_owned(),
                tags: None,
                graph: Some(graph),
                note: None,
            })
            .expect("creates");
        assert_eq!(created.issues, [], "the fixture graph is valid");
        created
    }

    fn activate(&self, detail: &ScenarioDetailDto, overlap: ScenarioOverlap) {
        self.scenarios
            .update(&UpdateScenarioInput {
                id: detail.scenario.id,
                name: None,
                tags: None,
                graph: None,
                note: None,
                overlap: Some(overlap),
            })
            .expect("sets the overlap");
        let input = SetScenarioActiveInput {
            id: detail.scenario.id,
            active: true,
        };
        self.scenarios
            .set_active(&input.id, input.active)
            .expect("activates");
    }

    async fn finish(&self, id: &RunId) -> RunStatus {
        tokio::time::timeout(Duration::from_secs(5), self.runs.wait(id))
            .await
            .expect("the run ends");
        self.runs.get(id).expect("the run").status
    }

    fn steps(&self, id: &RunId) -> Vec<StepRow> {
        self.runs.step_rows(&id.to_string()).expect("steps")
    }

    fn started_order(&self, id: &RunId) -> Vec<String> {
        let run = self.runs.get(id).expect("the run");
        self.sink
            .events()
            .into_iter()
            .filter(|event| event.stream_id == run.stream_id)
            .filter_map(|event| match event.body {
                HostEventDraft::Step { step } if step["status"] == "running" => {
                    step["nodeId"].as_str().map(str::to_owned)
                }
                _ => None,
            })
            .collect()
    }

    fn scenario_runs(&self, scenario: &ScenarioDetailDto) -> Vec<zvs_contract::RunSummaryDto> {
        self.runs
            .list(&zvs_contract::RunListFilter {
                subject_id: Some(scenario.scenario.id.to_string()),
                ..zvs_contract::RunListFilter::default()
            })
            .expect("lists runs")
            .items
    }
}

fn node(id: &str, node_type: &str, params: Value) -> ScenarioNodeDto {
    ScenarioNodeDto {
        id: id.to_owned(),
        node_type: node_type.to_owned(),
        name: None,
        position: ScenarioPositionDto { x: 0.5, y: 0.5 },
        params: params.as_object().cloned().unwrap_or_default(),
        disabled: None,
        settings: None,
        notes: None,
        pinned_input: None,
    }
}

fn edge(from: &str, to: &str) -> ScenarioEdgeDto {
    ScenarioEdgeDto {
        from: from.to_owned(),
        to: to.to_owned(),
        from_port: "main".to_owned(),
        to_port: "main".to_owned(),
    }
}

fn port_edge(from: &str, port: &str, to: &str) -> ScenarioEdgeDto {
    ScenarioEdgeDto {
        from_port: port.to_owned(),
        ..edge(from, to)
    }
}

fn branch_graph(condition: Value) -> ScenarioGraphDto {
    ScenarioGraphDto {
        nodes: vec![
            node("trigger", TRIGGER_CHAT, json!({})),
            node("branch", FLOW_BRANCH, json!({ "condition": condition })),
            node("shout", UPPER, json!({ "text": "{{result.text}}" })),
            node("broken", FAIL, json!({})),
            node(
                "reply",
                OUTPUT_CHAT_REPLY,
                json!({ "template": "{{result.text}}" }),
            ),
        ],
        edges: vec![
            edge("trigger", "branch"),
            port_edge("branch", "true", "shout"),
            port_edge("branch", "false", "broken"),
            edge("shout", "reply"),
        ],
        attachments: Vec::new(),
    }
}

fn attach(parent: &str, child: &str, slot: ScenarioSlot) -> ScenarioAttachmentDto {
    ScenarioAttachmentDto {
        parent_node_id: parent.to_owned(),
        child_node_id: child.to_owned(),
        slot,
    }
}

fn reply_graph(middle: &str) -> ScenarioGraphDto {
    ScenarioGraphDto {
        nodes: vec![
            node("trigger", TRIGGER_CHAT, json!({})),
            node("shout", middle, json!({ "text": "{{result.text}}" })),
            node(
                "reply",
                OUTPUT_CHAT_REPLY,
                json!({ "template": "Ответ: {{result.text}}" }),
            ),
        ],
        edges: vec![edge("trigger", "shout"), edge("shout", "reply")],
        attachments: Vec::new(),
    }
}

fn agent_graph() -> ScenarioGraphDto {
    ScenarioGraphDto {
        nodes: vec![
            node("trigger", TRIGGER_CHAT, json!({})),
            node(
                "agent",
                AGENT_TOOLS,
                json!({ "systemMessage": "Отвечать кратко" }),
            ),
            node(
                "model",
                MODEL_CHAT,
                json!({ "providerId": "p", "modelId": "m", "temperature": 0.2 }),
            ),
            node("memory", MEMORY_WINDOW, json!({ "size": 4 })),
            node("set", TOOL_SET, json!({ "tools": [UPPER] })),
            node("search", SEARCH, json!({})),
            node("reply", OUTPUT_CHAT_REPLY, json!({})),
        ],
        edges: vec![edge("trigger", "agent"), edge("agent", "reply")],
        attachments: vec![
            attach("agent", "model", ScenarioSlot::Model),
            attach("agent", "memory", ScenarioSlot::Memory),
            attach("agent", "set", ScenarioSlot::Tool),
            attach("agent", "search", ScenarioSlot::Tool),
        ],
    }
}

fn event() -> Value {
    json!({ "conversationId": "c1", "text": "привет" })
}

#[tokio::test]
async fn compilation_folds_attachments_into_the_parent_step() {
    let rig = rig();
    let compiled = compile(&agent_graph(), &rig.scenarios.catalogue()).expect("compiles");
    let ids: Vec<&str> = compiled.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids, ["trigger", "agent", "reply"]);
    let agent = &compiled.nodes[1];
    assert_eq!(agent.dependencies(), ["trigger"]);
    let input = agent.input();
    assert_eq!(input["model"]["params"]["modelId"], "m");
    assert_eq!(input["model"]["nodeId"], "model");
    assert_eq!(input["memory"]["params"]["size"], 4);
    assert_eq!(input["tools"], json!([UPPER, SEARCH]));
    assert_eq!(input["params"]["systemMessage"], "Отвечать кратко");
    let bindings: Vec<(&String, &GraphBinding)> = agent.bindings().collect();
    assert_eq!(
        bindings,
        [
            (&EVENT_KEY.to_owned(), &GraphBinding::Run {}),
            (&from_key("trigger"), &GraphBinding::node("trigger")),
        ]
    );
    assert_eq!(
        compiled.allowed_tools,
        Some(vec![UPPER.to_owned(), SEARCH.to_owned()])
    );
}

#[tokio::test]
async fn compilation_skips_disabled_steps_and_binds_result_references() {
    let rig = rig();
    let mut graph = reply_graph(UPPER);
    graph.nodes.insert(2, node("muted", UPPER, json!({})));
    graph.nodes[2].disabled = Some(true);
    graph.edges = vec![
        edge("trigger", "shout"),
        edge("shout", "muted"),
        edge("muted", "reply"),
    ];
    graph.nodes[1].settings = Some(ScenarioNodeSettingsDto {
        max_attempts: Some(3),
        backoff_ms: Some(10),
        timeout_ms: Some(2_000),
        on_error: Some(ScenarioOnError::Continue),
    });
    let compiled = compile(&graph, &rig.scenarios.catalogue()).expect("compiles");
    let ids: Vec<&str> = compiled.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids, ["trigger", "shout", "reply"]);
    let shout = &compiled.nodes[1];
    assert_eq!(
        shout.bindings().collect::<Vec<_>>(),
        [(
            &"text".to_owned(),
            &GraphBinding::node_path("trigger", "text")
        )]
    );
    assert_eq!(shout.retry().max_attempts, 3);
    assert_eq!(shout.timeout_ms, Some(2_000));
    assert!(shout.continues_on_error());
    assert_eq!(compiled.nodes[2].dependencies(), ["shout"]);
}

#[tokio::test]
async fn compilation_refuses_what_the_validator_refuses() {
    let rig = rig();
    let mut graph = agent_graph();
    graph
        .attachments
        .retain(|attachment| attachment.slot != ScenarioSlot::Model);
    let error = compile(&graph, &rig.scenarios.catalogue()).expect_err("refused");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, SCENARIO_INVALID);
    assert!(
        error
            .details
            .as_ref()
            .is_some_and(|details| details["issues"][0]["nodeId"] == "agent"),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_test_run_executes_steps_in_dependency_order_and_feeds_results_forward() {
    let rig = rig();
    let scenario = rig.create(reply_graph(UPPER));
    let handle = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Succeeded);
    assert_eq!(rig.started_order(&handle.id), ["trigger", "shout", "reply"]);
    let steps = rig.steps(&handle.id);
    let shout = steps
        .iter()
        .find(|step| step.node_id == "shout")
        .expect("shout");
    assert_eq!(shout.input, json!({ "text": "привет" }));
    assert_eq!(shout.output, json!({ "text": "ПРИВЕТ" }));
    assert_eq!(
        rig.effects.posted(),
        [("c1".to_owned(), "Ответ: ПРИВЕТ".to_owned())]
    );
    let run = rig.runs.get(&handle.id).expect("the run");
    assert_eq!(run.subject_id, Some(scenario.scenario.id.to_string()));
}

#[tokio::test]
async fn a_single_step_runs_on_its_pinned_input() {
    let rig = rig();
    let mut graph = reply_graph(UPPER);
    graph.nodes[2].pinned_input = Some(json!({ "text": "закреплено" }));
    let scenario = rig.create(reply_graph(UPPER));
    let handle = rig
        .runner
        .run_node(&ScenarioRunNodeInput {
            id: scenario.scenario.id,
            graph: graph.clone(),
            node_id: "reply".to_owned(),
            input: None,
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Failed);
    let steps = rig.steps(&handle.id);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].node_id, "reply");
    assert_eq!(
        steps[0].input[from_key(PINNED_SOURCE)],
        json!({ "text": "закреплено" })
    );
    let explicit = rig
        .runner
        .run_node(&ScenarioRunNodeInput {
            id: scenario.scenario.id,
            graph,
            node_id: "reply".to_owned(),
            input: Some(json!({ "conversationId": "c9", "text": "вход" })),
        })
        .expect("starts");
    assert_eq!(rig.finish(&explicit.id).await, RunStatus::Succeeded);
    assert_eq!(
        rig.effects.posted(),
        [("c9".to_owned(), "Ответ: вход".to_owned())]
    );
}

#[tokio::test]
async fn a_single_registry_step_resolves_result_references_from_the_pinned_input() {
    let rig = rig();
    let scenario = rig.create(reply_graph(UPPER));
    let compiled = compile_node(
        &scenario.graph,
        &rig.scenarios.catalogue(),
        "shout",
        &json!({ "text": "тихо" }),
    )
    .expect("compiles");
    assert_eq!(compiled.nodes.len(), 1);
    assert_eq!(compiled.nodes[0].input(), &json!({ "text": "тихо" }));
    assert_eq!(compiled.nodes[0].dependencies(), Vec::<String>::new());
}

#[tokio::test]
async fn a_failing_step_fails_the_run_at_that_node_and_stops_downstream() {
    let rig = rig();
    let scenario = rig.create(reply_graph(FAIL));
    let handle = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Failed);
    let steps = rig.steps(&handle.id);
    let failed: Vec<(&str, StepStatus)> = steps
        .iter()
        .map(|step| (step.node_id.as_str(), step.status))
        .collect();
    assert_eq!(
        failed,
        [
            ("trigger", StepStatus::Succeeded),
            ("shout", StepStatus::Failed)
        ]
    );
    assert_eq!(steps[1].error.as_deref(), Some(BROKEN));
    assert!(rig.effects.posted().is_empty());
}

#[tokio::test]
async fn continue_on_error_passes_the_failure_downstream() {
    let rig = rig();
    let mut graph = reply_graph(FAIL);
    graph.nodes[1].settings = Some(ScenarioNodeSettingsDto {
        on_error: Some(ScenarioOnError::Continue),
        ..ScenarioNodeSettingsDto::default()
    });
    graph.nodes[2].params = json!({ "template": "Ошибка: {{result.error}}" })
        .as_object()
        .cloned()
        .unwrap_or_else(Map::new);
    let scenario = rig.create(graph);
    let handle = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Succeeded);
    assert_eq!(
        rig.effects.posted(),
        [("c1".to_owned(), format!("Ошибка: {BROKEN}"))]
    );
}

#[tokio::test]
async fn retrying_a_failed_idempotent_step_reuses_the_steps_before_it() {
    let rig = rig();
    let scenario = rig.create(reply_graph(FAIL));
    let failed = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&failed.id).await, RunStatus::Failed);
    let retried = rig
        .runner
        .retry_step(&ScenarioRetryStepInput {
            run_id: failed.id,
            node_id: "shout".to_owned(),
        })
        .expect("retries");
    assert_eq!(rig.finish(&retried.id).await, RunStatus::Failed);
    assert_eq!(rig.started_order(&retried.id), ["shout"]);
    let run = rig.runs.get(&retried.id).expect("the retry");
    assert_eq!(run.retry_of_id, Some(failed.id));
    let refused = rig
        .runner
        .retry_step(&ScenarioRetryStepInput {
            run_id: failed.id,
            node_id: "trigger".to_owned(),
        })
        .expect_err("the trigger succeeded");
    assert_eq!(refused.code, AppErrorCode::Conflict);
}

#[tokio::test]
async fn a_step_with_side_effects_cannot_be_retried_on_its_own() {
    let rig = rig();
    let mut graph = reply_graph(UPPER);
    graph.nodes[2].params = Map::new();
    let scenario = rig.create(graph);
    let failed = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(json!({ "text": "без диалога" })),
        })
        .expect("starts");
    assert_eq!(rig.finish(&failed.id).await, RunStatus::Failed);
    let refused = rig
        .runner
        .retry_step(&ScenarioRetryStepInput {
            run_id: failed.id,
            node_id: "reply".to_owned(),
        })
        .expect_err("refused");
    assert_eq!(refused.message, super::runner::STEP_NOT_IDEMPOTENT);
}

async fn overlapping(overlap: ScenarioOverlap) -> (Rig, ScenarioDetailDto, Vec<Fired>) {
    let rig = rig();
    let scenario = rig.create(reply_graph(WAIT));
    rig.activate(&scenario, overlap);
    let mut fired = Vec::new();
    for _ in 0..2 {
        let mut outcomes = rig
            .runner
            .fire(ScenarioTriggerKind::Chat, &event())
            .expect("fires");
        assert_eq!(outcomes.len(), 1);
        fired.push(outcomes.remove(0).1);
    }
    (rig, scenario, fired)
}

#[tokio::test]
async fn an_overlapping_trigger_is_skipped_when_the_scenario_skips() {
    let (rig, scenario, fired) = overlapping(ScenarioOverlap::Skip).await;
    let Fired::Started(first) = &fired[0] else {
        panic!("the first trigger starts a run: {fired:?}");
    };
    assert_eq!(fired[1], Fired::Skipped);
    rig.gate.notify_one();
    assert_eq!(rig.finish(&first.id).await, RunStatus::Succeeded);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(rig.scenario_runs(&scenario).len(), 1);
}

#[tokio::test]
async fn an_overlapping_trigger_waits_its_turn_when_the_scenario_queues() {
    let (rig, scenario, fired) = overlapping(ScenarioOverlap::Queue).await;
    let Fired::Started(first) = &fired[0] else {
        panic!("the first trigger starts a run: {fired:?}");
    };
    assert_eq!(fired[1], Fired::Queued);
    assert_eq!(rig.runner.queued(&scenario.scenario.id.to_string()), 1);
    assert_eq!(rig.scenario_runs(&scenario).len(), 1);
    rig.gate.notify_one();
    assert_eq!(rig.finish(&first.id).await, RunStatus::Succeeded);
    let second = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let runs = rig.scenario_runs(&scenario);
            if let Some(run) = runs.iter().find(|run| run.id != first.id) {
                return run.id;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the queued trigger starts");
    rig.gate.notify_one();
    assert_eq!(rig.finish(&second).await, RunStatus::Succeeded);
    assert_eq!(rig.runner.queued(&scenario.scenario.id.to_string()), 0);
    assert_eq!(rig.effects.posted().len(), 2);
}

#[tokio::test]
async fn a_chat_trigger_only_fires_for_its_conversation() {
    let rig = rig();
    let mut graph = reply_graph(UPPER);
    graph.nodes[0].params = json!({ "conversationId": "c2" })
        .as_object()
        .cloned()
        .unwrap_or_default();
    let scenario = rig.create(graph);
    rig.activate(&scenario, ScenarioOverlap::Skip);
    let other = rig
        .runner
        .fire(ScenarioTriggerKind::Chat, &event())
        .expect("fires");
    assert!(other.is_empty());
    let matching = rig
        .runner
        .fire(
            ScenarioTriggerKind::Chat,
            &json!({ "conversationId": "c2", "text": "да" }),
        )
        .expect("fires");
    let Fired::Started(handle) = &matching[0].1 else {
        panic!("starts: {matching:?}");
    };
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Succeeded);
    assert_eq!(
        rig.effects.posted(),
        [("c2".to_owned(), "Ответ: ДА".to_owned())]
    );
}

#[tokio::test]
async fn a_step_that_overruns_its_timeout_fails_with_a_timeout() {
    let rig = rig();
    let mut graph = reply_graph(WAIT);
    graph.nodes[1].settings = Some(ScenarioNodeSettingsDto {
        timeout_ms: Some(30),
        ..ScenarioNodeSettingsDto::default()
    });
    let scenario = rig.create(graph);
    let handle = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Failed);
    let steps = rig.steps(&handle.id);
    let shout = steps
        .iter()
        .find(|step| step.node_id == "shout")
        .expect("shout");
    assert_eq!(
        shout.error.as_deref(),
        Some(crate::kernel::scheduler::STEP_TIMED_OUT)
    );
}

#[tokio::test]
async fn a_knowledge_node_grants_only_the_scoped_search_and_memory_grants_no_tool() {
    let rig = rig();
    let mut graph = agent_graph();
    graph.nodes.push(node(
        "stores",
        KNOWLEDGE_STORES,
        json!({ "stores": ["0199aa11-2222-7111-8111-111111111111"] }),
    ));
    graph.nodes.push(node(
        "facts",
        MEMORY_FACTS,
        json!({ "policy": "readWrite", "area": "both" }),
    ));
    graph
        .attachments
        .push(attach("agent", "stores", ScenarioSlot::Memory));
    graph
        .attachments
        .push(attach("agent", "facts", ScenarioSlot::Memory));
    let compiled = compile(&graph, &rig.scenarios.catalogue()).expect("compiles");
    assert_eq!(
        compiled.allowed_tools,
        Some(vec![
            UPPER.to_owned(),
            SEARCH.to_owned(),
            "knowledge.search".to_owned()
        ])
    );
    let agent = &compiled.nodes[1];
    assert_eq!(
        agent.input()["knowledge"]["params"]["stores"],
        json!(["0199aa11-2222-7111-8111-111111111111"])
    );
    assert_eq!(agent.input()["facts"]["params"]["area"], "both");
    assert_eq!(
        agent.input()["tools"],
        json!([UPPER, SEARCH, "knowledge.search"])
    );
}

#[tokio::test]
async fn an_agent_without_tool_nodes_is_allowed_no_tool() {
    let rig = rig();
    let mut graph = agent_graph();
    graph
        .nodes
        .retain(|node| node.id != "set" && node.id != "search");
    graph
        .attachments
        .retain(|attachment| attachment.slot != ScenarioSlot::Tool);
    let compiled = compile(&graph, &rig.scenarios.catalogue()).expect("compiles");
    assert_eq!(compiled.allowed_tools, Some(Vec::new()));
    assert!(compiled.nodes[1].input().get("tools").is_none());
}

#[tokio::test]
async fn branch_ports_compile_into_conditions_on_the_steps_they_lead_to() {
    let rig = rig();
    let compiled = compile(
        &branch_graph(json!("{{result.flag}}")),
        &rig.scenarios.catalogue(),
    )
    .expect("compiles");
    let step = |id: &str| {
        compiled
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("{id} compiled"))
    };
    let shout = step("shout").when.clone().expect("a condition");
    assert_eq!(
        (shout.node_id.as_str(), shout.field.as_str(), shout.equals),
        ("branch", "condition", true)
    );
    assert!(!step("broken").when.clone().expect("a condition").equals);
    assert_eq!(step("reply").when, None);
    let branch = step("branch");
    assert_eq!(
        branch
            .bindings()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["condition", "else", "then"]
    );
}

#[tokio::test]
async fn a_branch_runs_only_the_taken_port_and_skips_what_follows_the_other() {
    let rig = rig();
    let scenario = rig.create(branch_graph(json!("true")));
    let handle = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Succeeded);
    assert_eq!(
        rig.started_order(&handle.id),
        ["trigger", "branch", "shout", "reply"]
    );
    assert_eq!(
        rig.effects.posted(),
        [("c1".to_owned(), "ПРИВЕТ".to_owned())]
    );

    let scenario = rig.create(branch_graph(json!(false)));
    let handle = rig
        .runner
        .test(&ScenarioRunInput {
            id: scenario.scenario.id,
            graph: None,
            input: Some(event()),
        })
        .expect("starts");
    assert_eq!(rig.finish(&handle.id).await, RunStatus::Failed);
    assert_eq!(
        rig.started_order(&handle.id),
        ["trigger", "branch", "broken"]
    );
}
