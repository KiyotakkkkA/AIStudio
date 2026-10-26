use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AgenticBudgetDto, AgenticInstructInput, AgenticPermissionMode, AgenticPlanEditDto,
    AgenticPlanStepStatus, AgenticSessionDto, AgenticSessionId, AgenticSessionStatus,
    AgenticStopReason, AppError, AppErrorCode, CreateAgenticSessionInput,
    ExtendAgenticSessionInput, HostEventDraft, ProviderCapability, ProviderId, ProviderKind, RunId,
    RunStatus, SetAgenticModeInput, StepStatus, ToolDto, ToolMode, ToolSource, ToolTier,
    UpdateAgenticPlanInput,
};

use super::node::{
    AGENTIC_DOMAIN, ARTIFACTS_RULES, COMPLETE_TOOL, FINISH_TOOL, NO_TOOL_SUPPORT, NOT_THIS_RUN,
    PLAN_TOOL, TOOL_NOT_OFFERED,
};
use super::service::{
    BUDGET_NOT_ABOVE_USAGE, SESSION_FINISHED, SESSION_NOT_PAUSED, SESSION_RUNNING, TOOL_UNKNOWN,
};
use super::{AgenticService, AgenticServiceOptions, register_agentic_nodes};
use crate::data::Repositories;
use crate::data::repositories::ProviderDraft;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::ai::{
    DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult, TextDelta, TextGenerationDriver,
    ToolCall,
};
use crate::kernel::{FnNode, KernelServices, NodeRegistry, PermissionRequirement, TextDrivers};
use crate::platform::events::EventBus;
use crate::services::{RunService, RunServiceOptions};
use crate::testing::{
    CLOCK_ORIGIN, FAKE_CAPABILITIES, FakeClock, MemorySink, TemporaryDatabase, TokioClock,
    temporary_database,
};
use crate::tools::{ToolDescriptor, ToolRegistry, ToolRegistryOptions};
use crate::workspace::{FolderAccessService, FolderTarget, ForbiddenFolders, OUTSIDE_WORKSPACE};

const READ: &str = "demo.read";
const WRITE: &str = "demo.write";
const READ_NAME: &str = "demo__read";
const WRITE_NAME: &str = "demo__write";
const WAIT: &str = "demo.wait";
const WAIT_NAME: &str = "demo__wait";

type Round = Box<dyn FnOnce(&GenerateRequest) -> Vec<TextDelta> + Send>;

struct Scripted {
    requests: Mutex<Vec<GenerateRequest>>,
    rounds: Mutex<VecDeque<Round>>,
    mode: Mutex<ToolMode>,
    fail: Mutex<Option<AppError>>,
}

impl Scripted {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            rounds: Mutex::new(VecDeque::new()),
            mode: Mutex::new(ToolMode::Native),
            fail: Mutex::new(None),
        })
    }

    fn round(&self, round: impl FnOnce(&GenerateRequest) -> Vec<TextDelta> + Send + 'static) {
        self.rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(Box::new(round));
    }

    fn reply(&self, deltas: Vec<TextDelta>) {
        self.round(move |_| deltas);
    }

    fn requests(&self) -> Vec<GenerateRequest> {
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

    fn stream(&self, request: &GenerateRequest, _cancel: &CancellationToken) -> DeltaStream {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        if let Some(error) = self
            .fail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Box::pin(stream::iter(vec![Err(error)]));
        }
        let round = self
            .rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front();
        let deltas = round.map_or_else(|| vec![TextDelta::text("Готово.")], |round| round(request));
        Box::pin(stream::iter(deltas.into_iter().map(Ok)))
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct OneDriver(SharedTextDriver);

impl TextDrivers for OneDriver {
    fn text(&self, _provider_id: &str) -> Result<SharedTextDriver, AppError> {
        Ok(Arc::clone(&self.0))
    }
}

static CALLS: AtomicUsize = AtomicUsize::new(0);

fn call(name: &str, arguments: &Value) -> TextDelta {
    let id = CALLS.fetch_add(1, Ordering::SeqCst);
    TextDelta::ToolCall(ToolCall::new(
        format!("call_{id}"),
        name,
        arguments.to_string(),
    ))
}

fn plan(steps: &[&str]) -> TextDelta {
    call(PLAN_TOOL, &json!({ "steps": steps }))
}

fn complete(summary: &str) -> TextDelta {
    call(COMPLETE_TOOL, &json!({ "summary": summary }))
}

fn finish(summary: &str) -> TextDelta {
    call(FINISH_TOOL, &json!({ "summary": summary }))
}

fn read(path: &str) -> TextDelta {
    call(READ_NAME, &json!({ "path": path }))
}

#[derive(Default)]
struct Effects {
    reads: Mutex<Vec<String>>,
    writes: Mutex<Vec<String>>,
}

impl Effects {
    fn reads(&self) -> Vec<String> {
        self.reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn writes(&self) -> Vec<String> {
        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Rig {
    database: TemporaryDatabase,
    workspace: TempDir,
    runs: RunService,
    service: AgenticService,
    tools: ToolRegistry,
    folders: FolderAccessService,
    driver: Arc<Scripted>,
    clock: Arc<FakeClock>,
    effects: Arc<Effects>,
    sink: Arc<MemorySink>,
    write: ToolDto,
    provider: ProviderId,
}

fn register_demo_nodes(nodes: &NodeRegistry, effects: &Arc<Effects>) {
    let reads = Arc::clone(effects);
    nodes
        .register(FnNode::new(
            READ,
            PermissionRequirement::tool(READ),
            false,
            move |_context, input: Value| {
                let effects = Arc::clone(&reads);
                async move {
                    let path = input["path"].as_str().unwrap_or_default().to_owned();
                    effects
                        .reads
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(path.clone());
                    Ok(json!({ "text": format!("contents of {path}") }))
                }
            },
        ))
        .expect("registers read");
    let writes = Arc::clone(effects);
    nodes
        .register(FnNode::new(
            WRITE,
            PermissionRequirement::tool(WRITE),
            true,
            move |_context, input: Value| {
                let effects = Arc::clone(&writes);
                async move {
                    let path = input["path"].as_str().unwrap_or_default().to_owned();
                    effects
                        .writes
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(path.clone());
                    Ok(json!({
                        "text": "written",
                        "changes": [{
                            "path": path,
                            "kind": "modify",
                            "added": 2,
                            "removed": 1,
                            "diff": "",
                            "truncated": false
                        }]
                    }))
                }
            },
        ))
        .expect("registers write");
    nodes
        .register(FnNode::new(
            WAIT,
            PermissionRequirement::tool(WAIT),
            false,
            |context: crate::kernel::StepContext, _input: Value| async move {
                context.cancel.cancelled().await;
                Err::<Value, _>(crate::kernel::types::cancelled())
            },
        ))
        .expect("registers wait");
}

fn rig() -> Rig {
    let database = temporary_database();
    let workspace = TempDir::new().expect("a workspace");
    let nodes = Arc::new(NodeRegistry::new());
    let effects = Arc::new(Effects::default());
    register_demo_nodes(&nodes, &effects);
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    let path_schema = json!({ "type": "object", "properties": { "path": { "type": "string" } } });
    tools
        .register(&ToolDescriptor {
            description: "Прочитать файл".to_owned(),
            input_schema: path_schema.clone(),
            read_only: true,
            tier: Some(ToolTier::Auto),
            ..ToolDescriptor::new(READ, ToolSource::Builtin)
        })
        .expect("registers read");
    let write = tools
        .register(&ToolDescriptor {
            description: "Записать файл".to_owned(),
            input_schema: path_schema,
            tier: Some(ToolTier::Auto),
            ..ToolDescriptor::new(WRITE, ToolSource::Builtin)
        })
        .expect("registers write");
    tools
        .register(&ToolDescriptor {
            read_only: true,
            tier: Some(ToolTier::Auto),
            ..ToolDescriptor::new(WAIT, ToolSource::Builtin)
        })
        .expect("registers wait");
    let driver = Scripted::new();
    let sink = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(sink.clone());
    let runs = RunService::new(RunServiceOptions {
        clock: Arc::new(TokioClock::new(CLOCK_ORIGIN)),
        tools: Some(tools.clone()),
        services: KernelServices {
            providers: Some(Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver))),
            ..KernelServices::default()
        },
        ..RunServiceOptions::new(database.shared(), bus, Arc::clone(&nodes))
    })
    .expect("a run service");
    let clock = Arc::new(FakeClock::default());
    let folders = FolderAccessService::new(
        database.shared(),
        runs.approvals(),
        ForbiddenFolders::new(&[], &[], Arc::new(Vec::new)),
        clock.clone(),
    );
    let service = AgenticService::new(AgenticServiceOptions {
        clock: clock.clone(),
        ..AgenticServiceOptions::new(
            database.shared(),
            runs.clone(),
            folders.clone(),
            tools.clone(),
        )
    });
    register_agentic_nodes(&nodes, service.store(), tools.clone())
        .expect("registers the session node");
    let provider = database
        .shared()
        .with_connection(|connection| {
            Repositories::new(connection)
                .providers
                .create(&ProviderDraft::new(
                    ProviderKind::OpenaiCompatible,
                    "Локальный",
                    "http://127.0.0.1:1234/v1",
                    vec![ProviderCapability::Text],
                    1,
                ))
        })
        .expect("a provider");
    Rig {
        database,
        workspace,
        runs,
        service,
        tools,
        folders,
        driver,
        clock,
        effects,
        sink,
        write,
        provider: ProviderId::parse(&provider.id).expect("a provider id"),
    }
}

impl Rig {
    fn input(&self, mode: AgenticPermissionMode) -> CreateAgenticSessionInput {
        CreateAgenticSessionInput {
            goal: "Прочитать файлы и написать сводку".to_owned(),
            workspace: self.workspace.path().to_string_lossy().into_owned(),
            provider_id: self.provider,
            model_id: "scripted".to_owned(),
            mode,
            budget: None,
            tools: None,
        }
    }

    fn start(&self, mode: AgenticPermissionMode) -> AgenticSessionDto {
        self.service.create(&self.input(mode)).expect("creates")
    }

    fn start_with(&self, input: CreateAgenticSessionInput) -> AgenticSessionDto {
        self.service.create(&input).expect("creates")
    }

    async fn settle(&self, session: &AgenticSessionDto) -> AgenticSessionDto {
        let run = session.run_id.expect("a run");
        tokio::time::timeout(Duration::from_secs(5), self.runs.wait(&run))
            .await
            .expect("the run ends");
        self.service
            .get(&session.id.to_string())
            .expect("the session")
    }

    fn run_status(&self, run: &RunId) -> RunStatus {
        self.runs.get(run).expect("the run").status
    }

    async fn blocked(&self, run: &RunId) -> String {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let approval = self
                    .sink
                    .events()
                    .into_iter()
                    .find_map(|event| match event.body {
                        HostEventDraft::Approval { request } => {
                            request.get("id").and_then(Value::as_str).map(str::to_owned)
                        }
                        _ => None,
                    });
                if let Some(approval) = approval
                    && self.run_status(run) == RunStatus::Blocked
                {
                    return approval;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the run blocks on an approval")
    }

    fn set_tier(&self, tool: &ToolDto, tier: ToolTier) {
        self.tools
            .set_tier(&tool.id.to_string(), tier, "test")
            .expect("sets the tier");
    }

    fn offered_names(&self, index: usize) -> Vec<String> {
        self.driver.requests()[index]
            .tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect()
    }
}

fn plan_titles(session: &AgenticSessionDto) -> Vec<(&str, AgenticPlanStepStatus)> {
    session
        .plan
        .iter()
        .map(|step| (step.title.as_str(), step.status))
        .collect()
}

fn last_message(request: &GenerateRequest) -> String {
    request
        .messages
        .last()
        .map(|message| message.content.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn the_loop_plans_executes_observes_and_finishes_on_the_goal() {
    let rig = rig();
    rig.driver
        .reply(vec![plan(&["Прочитать файлы", "Написать сводку"])]);
    rig.driver
        .reply(vec![read("a.md"), complete("Прочитан a.md")]);
    rig.driver.round(|request| {
        assert!(
            request
                .messages
                .iter()
                .any(|message| message.content.contains("contents of a.md"))
        );
        vec![complete("Сводка готова"), finish("Сводка написана")]
    });
    let session = rig.start(AgenticPermissionMode::Auto);
    assert_eq!(session.status, AgenticSessionStatus::Running);

    let done = rig.settle(&session).await;

    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Goal));
    assert_eq!(done.summary.as_deref(), Some("Сводка написана"));
    assert_eq!(
        plan_titles(&done),
        [
            ("Прочитать файлы", AgenticPlanStepStatus::Done),
            ("Написать сводку", AgenticPlanStepStatus::Done)
        ]
    );
    assert_eq!(done.plan[0].summary.as_deref(), Some("Прочитан a.md"));
    assert_eq!(done.usage.steps, 3);
    assert_eq!(done.usage.tool_calls, 1);
    assert!(done.usage.tokens > 0);
    assert_eq!(rig.effects.reads(), ["a.md"]);
    let run = done.run_id.expect("the run");
    assert_eq!(rig.run_status(&run), RunStatus::Succeeded);
    let steps = rig.runs.step_rows(&run.to_string()).expect("steps");
    let tool_step = steps
        .iter()
        .find(|step| step.step_type == READ)
        .expect("the tool call is its own kernel step");
    assert_eq!(tool_step.node_id, "session.call1");
    assert_eq!(tool_step.status, StepStatus::Succeeded);
    assert_eq!(
        done.plan[0]
            .step_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        std::slice::from_ref(&tool_step.id)
    );
    let system = rig.driver.requests()[0].system.clone().unwrap_or_default();
    assert!(system.contains("update_plan"));
    assert!(system.contains(ARTIFACTS_RULES));
    assert!(rig.sink.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Step { step } if step.get("domain") == Some(&json!(AGENTIC_DOMAIN))
    )));
}

#[tokio::test]
async fn a_plain_answer_without_tool_calls_completes_the_session() {
    let rig = rig();
    rig.driver
        .reply(vec![TextDelta::text("Цель уже достигнута.")]);
    let done = rig.settle(&rig.start(AgenticPermissionMode::Ask)).await;
    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert_eq!(done.summary.as_deref(), Some("Цель уже достигнута."));
}

#[tokio::test]
async fn the_step_budget_pauses_the_session_and_an_extension_continues_it() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Прочитать"])]);
    rig.driver.reply(vec![read("a.md")]);
    rig.driver.reply(vec![read("b.md")]);
    let session = rig.start_with(CreateAgenticSessionInput {
        budget: Some(AgenticBudgetDto {
            max_steps: 2,
            ..AgenticBudgetDto::default()
        }),
        ..rig.input(AgenticPermissionMode::Auto)
    });

    let paused = rig.settle(&session).await;

    assert_eq!(paused.status, AgenticSessionStatus::Paused);
    assert_eq!(paused.stop_reason, Some(AgenticStopReason::Steps));
    assert_eq!(paused.usage.steps, 2);
    assert_eq!(rig.driver.requests().len(), 2);
    assert_eq!(rig.effects.reads(), ["a.md"]);
    let first_run = paused.run_id.expect("the first run");
    assert_eq!(rig.run_status(&first_run), RunStatus::Succeeded);

    let id = paused.id;
    let too_small = rig.service.extend(&ExtendAgenticSessionInput {
        id,
        budget: AgenticBudgetDto {
            max_steps: 2,
            ..AgenticBudgetDto::default()
        },
    });
    assert_eq!(
        too_small.expect_err("refused").message,
        BUDGET_NOT_ABOVE_USAGE
    );
    let resumed = rig
        .service
        .extend(&ExtendAgenticSessionInput {
            id,
            budget: AgenticBudgetDto {
                max_steps: 10,
                ..AgenticBudgetDto::default()
            },
        })
        .expect("extends");
    assert_eq!(resumed.status, AgenticSessionStatus::Running);
    assert_ne!(resumed.run_id, Some(first_run));

    let done = rig.settle(&resumed).await;

    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert_eq!(rig.effects.reads(), ["a.md", "b.md"]);
    let resumed_request = &rig.driver.requests()[2];
    assert!(
        resumed_request
            .messages
            .iter()
            .any(|message| message.content.contains("contents of a.md")),
        "the transcript survives the pause"
    );
    assert_eq!(
        rig.service
            .extend(&ExtendAgenticSessionInput {
                id,
                budget: AgenticBudgetDto::default()
            })
            .expect_err("only a paused session extends")
            .message,
        SESSION_NOT_PAUSED
    );
}

#[tokio::test]
async fn the_time_budget_pauses_at_the_next_boundary() {
    let rig = rig();
    let clock = Arc::clone(&rig.clock);
    rig.driver.round(move |_| {
        clock.advance(16 * 60_000);
        vec![plan(&["Долгий шаг"])]
    });
    let done = rig.settle(&rig.start(AgenticPermissionMode::Auto)).await;
    assert_eq!(done.status, AgenticSessionStatus::Paused);
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Time));
    assert_eq!(rig.driver.requests().len(), 1);
}

#[tokio::test]
async fn the_token_budget_pauses_the_session() {
    let rig = rig();
    rig.driver
        .reply(vec![TextDelta::text("слово ".repeat(2000)), plan(&["Шаг"])]);
    let session = rig.start_with(CreateAgenticSessionInput {
        budget: Some(AgenticBudgetDto {
            max_tokens: 1000,
            ..AgenticBudgetDto::default()
        }),
        ..rig.input(AgenticPermissionMode::Auto)
    });
    let done = rig.settle(&session).await;
    assert_eq!(done.status, AgenticSessionStatus::Paused);
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Tokens));
    assert!(done.usage.tokens >= 1000);
}

#[tokio::test]
async fn the_default_budget_is_conservative_and_follows_settings() {
    let rig = rig();
    assert_eq!(
        rig.service.default_budget().expect("a budget"),
        AgenticBudgetDto {
            max_steps: 25,
            max_tokens: 200_000,
            max_minutes: 15
        }
    );
    rig.database
        .shared()
        .with_connection(|connection| {
            let settings = Repositories::new(connection).settings;
            settings.set(super::service::STEPS_KEY, "40", 1)?;
            Ok(())
        })
        .expect("stores the setting");
    assert_eq!(
        rig.service.default_budget().expect("a budget").max_steps,
        40
    );
}

#[tokio::test]
async fn an_approval_suspends_the_run_and_resumes_it_once_approved() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Записать сводку"])]);
    rig.driver
        .reply(vec![call(WRITE_NAME, &json!({ "path": "summary.md" }))]);
    rig.driver.round(|request| {
        assert!(last_message(request).contains("written"));
        vec![finish("Записано")]
    });
    let session = rig.start(AgenticPermissionMode::Ask);
    let run = session.run_id.expect("a run");

    let approval = rig.blocked(&run).await;
    assert!(rig.effects.writes().is_empty());
    rig.runs.approve(&run, &approval, false).expect("approves");
    let done = rig.settle(&session).await;

    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert_eq!(rig.effects.writes(), ["summary.md"]);
    assert_eq!(done.changed_files.len(), 1);
    assert_eq!(
        (done.changed_files[0].added, done.changed_files[0].removed),
        (2, 1)
    );
}

#[tokio::test]
async fn a_denied_approval_is_reported_to_the_model_and_nothing_runs() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Записать"])]);
    rig.driver
        .reply(vec![call(WRITE_NAME, &json!({ "path": "x.md" }))]);
    rig.driver.round(|request| {
        assert!(last_message(request).contains("denied"));
        vec![finish("Отменено пользователем")]
    });
    let session = rig.start(AgenticPermissionMode::Ask);
    let run = session.run_id.expect("a run");
    let approval = rig.blocked(&run).await;
    rig.runs.deny(&run, &approval).expect("denies");
    let done = rig.settle(&session).await;
    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert!(rig.effects.writes().is_empty());
}

#[tokio::test]
async fn auto_mode_still_asks_for_a_tool_the_tools_page_set_to_ask() {
    let rig = rig();
    rig.set_tier(&rig.write, ToolTier::Ask);
    rig.driver.reply(vec![plan(&["Записать"])]);
    rig.driver
        .reply(vec![call(WRITE_NAME, &json!({ "path": "x.md" }))]);
    let session = rig.start(AgenticPermissionMode::Auto);
    let run = session.run_id.expect("a run");
    let approval = rig.blocked(&run).await;
    assert!(rig.effects.writes().is_empty());
    rig.runs.approve(&run, &approval, false).expect("approves");
    rig.settle(&session).await;
    assert_eq!(rig.effects.writes(), ["x.md"]);
}

#[tokio::test]
async fn auto_mode_cannot_reach_a_tool_the_tools_page_switched_off() {
    let rig = rig();
    let session_tools = Some(vec![READ.to_owned(), WRITE.to_owned()]);
    rig.set_tier(&rig.write, ToolTier::Off);
    rig.driver.reply(vec![plan(&["Записать"])]);
    rig.driver.round(|request| {
        assert!(!request.tools.iter().any(|tool| tool.name == WRITE_NAME));
        vec![call(WRITE_NAME, &json!({ "path": "x.md" }))]
    });
    rig.driver.round(|request| {
        assert!(last_message(request).contains(TOOL_NOT_OFFERED));
        assert!(last_message(request).contains(&format!(
            "There is no tool named \"{WRITE_NAME}\" in this turn. The tools available now are: {READ_NAME}."
        )));
        vec![finish("Не удалось")]
    });
    let session = rig.start_with(CreateAgenticSessionInput {
        tools: session_tools,
        ..rig.input(AgenticPermissionMode::Auto)
    });
    let done = rig.settle(&session).await;
    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert!(rig.effects.writes().is_empty());
}

#[tokio::test]
async fn read_only_mode_offers_and_runs_only_reading_tools() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Прочитать"])]);
    rig.driver.reply(vec![
        call(WRITE_NAME, &json!({ "path": "x.md" })),
        read("a.md"),
    ]);
    let done = rig
        .settle(&rig.start(AgenticPermissionMode::ReadOnly))
        .await;
    assert_eq!(done.status, AgenticSessionStatus::Completed);
    let offered = rig.offered_names(0);
    assert!(offered.contains(&READ_NAME.to_owned()));
    assert!(!offered.contains(&WRITE_NAME.to_owned()));
    let system = rig.driver.requests()[0].system.clone().unwrap_or_default();
    assert!(!system.contains(ARTIFACTS_RULES));
    assert!(rig.effects.writes().is_empty());
    assert_eq!(rig.effects.reads(), ["a.md"]);
    assert!(done.tools.contains(&READ.to_owned()));
    assert!(!done.tools.contains(&WRITE.to_owned()));
}

#[tokio::test]
async fn plan_edits_wait_for_the_step_boundary_and_the_model_is_told() {
    let rig = rig();
    rig.driver.reply(vec![plan(&[
        "Прочитать",
        "Удалить тесты",
        "Написать сводку",
    ])]);
    let service = rig.service.clone();
    let holder: Arc<Mutex<Option<AgenticSessionId>>> = Arc::new(Mutex::new(None));
    let id_slot = Arc::clone(&holder);
    rig.driver.round(move |_| {
        let id = id_slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .expect("the session id");
        let session = service.get(&id.to_string()).expect("the session");
        let summary = session
            .plan
            .iter()
            .find(|step| step.title == "Написать сводку")
            .expect("queued")
            .id
            .clone();
        service
            .update_plan(&UpdateAgenticPlanInput {
                id,
                steps: vec![
                    AgenticPlanEditDto {
                        id: None,
                        title: "Проверить ссылки".to_owned(),
                    },
                    AgenticPlanEditDto {
                        id: Some(summary),
                        title: "Написать сводку".to_owned(),
                    },
                ],
            })
            .expect("edits the plan");
        let pending = service.get(&id.to_string()).expect("the session");
        assert!(pending.pending_plan.is_some());
        assert_eq!(pending.plan.len(), 3, "the edit is not applied mid-step");
        vec![read("a.md")]
    });
    rig.driver.round(|request| {
        let note = request
            .messages
            .iter()
            .rev()
            .find(|message| message.content.starts_with("Plan changed by the user"))
            .map(|message| message.content.clone())
            .expect("the model is told the plan changed");
        assert!(note.contains("Removed by the user, do not add back: Удалить тесты"));
        assert!(note.contains("Проверить ссылки"));
        vec![plan(&[
            "Удалить тесты",
            "Проверить ссылки",
            "Написать сводку",
        ])]
    });
    rig.driver.round(|request| {
        let reply = last_message(request);
        assert!(!reply.contains("[running] Удалить тесты"));
        assert!(!reply.contains("[queued] Удалить тесты"));
        vec![finish("Готово")]
    });
    let session = rig.start(AgenticPermissionMode::Auto);
    *holder.lock().unwrap_or_else(PoisonError::into_inner) = Some(session.id);

    let done = rig.settle(&session).await;

    assert_eq!(done.status, AgenticSessionStatus::Completed);
    assert!(done.pending_plan.is_none());
    let titles: Vec<&str> = done.plan.iter().map(|step| step.title.as_str()).collect();
    assert!(!titles.contains(&"Удалить тесты"));
    assert!(titles.contains(&"Проверить ссылки"));
    let second = &rig.driver.requests()[1];
    assert!(
        !second
            .messages
            .iter()
            .any(|message| message.content.starts_with("Plan changed by the user"))
    );
}

#[tokio::test]
async fn a_mode_change_and_an_instruction_apply_at_the_next_boundary() {
    let rig = rig();
    let service = rig.service.clone();
    let holder: Arc<Mutex<Option<AgenticSessionId>>> = Arc::new(Mutex::new(None));
    let id_slot = Arc::clone(&holder);
    rig.driver.round(move |_| {
        let id = id_slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .expect("the session id");
        service
            .set_mode(&SetAgenticModeInput {
                id,
                mode: AgenticPermissionMode::ReadOnly,
            })
            .expect("sets the mode");
        service
            .instruct(&AgenticInstructInput {
                id,
                text: "Не трогай папку tests".to_owned(),
            })
            .expect("instructs");
        vec![plan(&["Прочитать"])]
    });
    rig.driver.round(|request| {
        assert!(!request.tools.iter().any(|tool| tool.name == WRITE_NAME));
        let last = last_message(request);
        assert!(last.contains("read-only"));
        assert!(last.contains("Instruction from the user: Не трогай папку tests"));
        vec![finish("Готово")]
    });
    let session = rig.start(AgenticPermissionMode::Auto);
    *holder.lock().unwrap_or_else(PoisonError::into_inner) = Some(session.id);
    let done = rig.settle(&session).await;
    assert_eq!(done.mode, AgenticPermissionMode::ReadOnly);
    assert_eq!(done.pending_mode, None);
    assert!(done.pending_instructions.is_empty());
    assert!(rig.offered_names(0).contains(&WRITE_NAME.to_owned()));
    assert!(rig.sink.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Log { line } if line.get("message") == Some(&json!("permission mode changed"))
    )));
    assert!(rig.sink.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Step { step }
            if step.get("instructions") == Some(&json!(["Не трогай папку tests"]))
    )));
}

#[tokio::test]
async fn a_repeated_action_is_detected_as_a_loop() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Прочитать"])]);
    for _ in 0..5 {
        rig.driver.reply(vec![read("a.md")]);
    }
    let done = rig.settle(&rig.start(AgenticPermissionMode::Auto)).await;
    assert_eq!(done.status, AgenticSessionStatus::Stopped);
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Loop));
    assert_eq!(rig.effects.reads().len(), 3);
    assert_eq!(done.usage.steps, 4);
}

#[tokio::test]
async fn endless_plan_revisions_without_progress_are_detected() {
    let rig = rig();
    for index in 0..6 {
        rig.driver.reply(vec![plan(&[&format!("Вариант {index}")])]);
    }
    let done = rig.settle(&rig.start(AgenticPermissionMode::Auto)).await;
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Loop));
    assert_eq!(rig.driver.requests().len(), 4);
}

#[tokio::test]
async fn a_provider_failure_fails_the_session_with_the_error() {
    let rig = rig();
    *rig.driver
        .fail
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(AppError::new(
        AppErrorCode::ProviderUnreachable,
        "Провайдер недоступен",
    ));
    let done = rig.settle(&rig.start(AgenticPermissionMode::Auto)).await;
    assert_eq!(done.status, AgenticSessionStatus::Failed);
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Error));
    assert_eq!(done.error.as_deref(), Some("Провайдер недоступен"));
    assert_eq!(
        rig.service
            .instruct(&AgenticInstructInput {
                id: done.id,
                text: "Ещё раз".to_owned()
            })
            .expect_err("a finished session is closed")
            .message,
        SESSION_FINISHED
    );
}

#[tokio::test]
async fn a_model_without_tool_calls_cannot_run_a_session() {
    let rig = rig();
    *rig.driver
        .mode
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = ToolMode::None;
    let done = rig.settle(&rig.start(AgenticPermissionMode::Auto)).await;
    assert_eq!(done.status, AgenticSessionStatus::Failed);
    assert_eq!(done.error.as_deref(), Some(NO_TOOL_SUPPORT));
}

#[tokio::test]
async fn cancelling_a_running_session_stops_it_and_removal_waits_for_the_end() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Ждать"])]);
    rig.driver.reply(vec![call(WAIT_NAME, &json!({}))]);
    let session = rig.start(AgenticPermissionMode::Auto);
    let id = session.id.to_string();
    let run = session.run_id.expect("a run");
    tokio::time::timeout(Duration::from_secs(5), async {
        while !rig
            .runs
            .step_rows(&run.to_string())
            .expect("steps")
            .iter()
            .any(|step| step.step_type == WAIT && step.status == StepStatus::Running)
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the tool call starts");
    assert_eq!(
        rig.service.remove(&id).expect_err("still running").message,
        SESSION_RUNNING
    );

    rig.service.cancel(&id).await.expect("cancels");

    let done = rig.service.get(&id).expect("the session");
    assert_eq!(done.status, AgenticSessionStatus::Cancelled);
    assert_eq!(done.stop_reason, Some(AgenticStopReason::Cancelled));
    assert_eq!(
        plan_titles(&done),
        [("Ждать", AgenticPlanStepStatus::Failed)]
    );
    assert_eq!(rig.run_status(&run), RunStatus::Cancelled);
    rig.service.remove(&id).expect("removes");
    assert!(rig.service.list().expect("lists").is_empty());
}

#[tokio::test]
async fn retrying_a_finished_session_run_does_not_reopen_the_session() {
    let rig = rig();
    rig.driver.reply(vec![finish("Готово")]);
    let done = rig.settle(&rig.start(AgenticPermissionMode::Auto)).await;
    let retry = rig
        .runs
        .retry(&done.run_id.expect("the run"))
        .expect("the kernel retries the graph");
    tokio::time::timeout(Duration::from_secs(5), rig.runs.wait(&retry.id))
        .await
        .expect("the retry ends");
    let run = rig.runs.get(&retry.id).expect("the retry");
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.error.as_deref(), Some(NOT_THIS_RUN));
    let after = rig.service.get(&done.id.to_string()).expect("the session");
    assert_eq!(after.status, AgenticSessionStatus::Completed);
    assert_eq!(rig.driver.requests().len(), 1);
}

#[tokio::test]
async fn a_paused_session_can_be_cancelled_without_a_run() {
    let rig = rig();
    rig.driver.reply(vec![plan(&["Шаг"])]);
    let session = rig.start_with(CreateAgenticSessionInput {
        budget: Some(AgenticBudgetDto {
            max_steps: 1,
            ..AgenticBudgetDto::default()
        }),
        ..rig.input(AgenticPermissionMode::Auto)
    });
    let paused = rig.settle(&session).await;
    assert_eq!(paused.status, AgenticSessionStatus::Paused);
    rig.service
        .cancel(&paused.id.to_string())
        .await
        .expect("cancels");
    let cancelled = rig
        .service
        .get(&paused.id.to_string())
        .expect("the session");
    assert_eq!(cancelled.status, AgenticSessionStatus::Cancelled);
    assert_eq!(cancelled.stop_reason, Some(AgenticStopReason::Cancelled));
}

#[tokio::test]
async fn unknown_tools_and_forbidden_workspaces_are_refused_at_creation() {
    let rig = rig();
    let unknown = rig.service.create(&CreateAgenticSessionInput {
        tools: Some(vec!["demo.missing".to_owned()]),
        ..rig.input(AgenticPermissionMode::Auto)
    });
    assert_eq!(unknown.expect_err("refused").message, TOOL_UNKNOWN);
    let relative = rig.service.create(&CreateAgenticSessionInput {
        workspace: "notes".to_owned(),
        ..rig.input(AgenticPermissionMode::Auto)
    });
    assert_eq!(
        relative.expect_err("refused").code,
        AppErrorCode::ValidationFailed
    );
    assert!(rig.service.list().expect("lists").is_empty());
}

#[tokio::test]
async fn paths_resolve_against_the_workspace_and_never_leave_it() {
    let rig = rig();
    let session = rig.start(AgenticPermissionMode::ReadOnly);
    rig.settle(&session).await;
    std::fs::write(rig.workspace.path().join("a.md"), "text").expect("writes a file");
    let call = rig.folders.call(&session.id.to_string(), "fs.read");

    let inside = rig
        .folders
        .resolve(&call, &[FolderTarget::file("a.md")])
        .expect("a workspace path resolves");
    assert!(
        inside.folders.is_empty(),
        "no access request inside the workspace"
    );
    let canonical: PathBuf = rig
        .workspace
        .path()
        .canonicalize()
        .expect("canonical workspace");
    assert!(inside.resolved[0].path.starts_with(strip(&canonical)));

    let outside = TempDir::new().expect("another folder");
    for target in [
        FolderTarget::file(outside.path().join("b.md")),
        FolderTarget::folder(".."),
    ] {
        let Err(refused) = rig.folders.resolve(&call, &[target]) else {
            panic!("a path outside the workspace resolves");
        };
        assert_eq!(refused.code, AppErrorCode::PermissionDenied);
        assert_eq!(refused.message, OUTSIDE_WORKSPACE);
    }
    assert!(
        rig.folders
            .covered(&session.id.to_string(), &strip(&canonical))
            .expect("coverage is known"),
        "the workspace covers itself for the explorer"
    );
    assert!(
        !rig.folders
            .covered(&session.id.to_string(), outside.path())
            .expect("coverage is known")
    );
    assert_eq!(
        rig.folders
            .working_folder_of(&session.id.to_string())
            .expect("a folder")
            .map(|folder| strip(&folder)),
        Some(strip(&canonical))
    );
}

fn strip(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned())
}
