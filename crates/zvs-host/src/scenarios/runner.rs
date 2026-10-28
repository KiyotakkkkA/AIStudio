use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, HostEvent, HostEventDraft, RunDto, RunGraph, RunHandleDto, RunId,
    RunKind, RunStatus, ScenarioDto, ScenarioGraphDto, ScenarioOverlap, ScenarioRetryStepInput,
    ScenarioRunInput, ScenarioRunNodeInput, ScenarioTriggerKind, StartRunInput, StepStatus,
};

use super::compile::{compile, compile_node};
use super::service::ScenarioService;
use crate::data::Repositories;
use crate::data::schema::StepRow;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::RunService;

pub const TEST_RUN_TITLE: &str = "тестовый запуск";
pub const NOT_A_SCENARIO_RUN: &str = "Запуск не относится к сценарию";
pub const RUN_NOT_FAILED: &str = "Повторить шаг можно только в запуске, завершившемся ошибкой";
pub const STEP_NOT_FAILED: &str = "Этот шаг не завершился ошибкой";
pub const STEP_NOT_IDEMPOTENT: &str =
    "Шаг нельзя повторить отдельно: он действует за пределами сценария. Запустите сценарий заново";
pub const NODE_NOT_IN_RUN: &str = "Шаг не найден в запуске";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fired {
    Started(RunHandleDto),
    Queued,
    Skipped,
}

#[derive(Default)]
struct Lane {
    running: Option<String>,
    queued: VecDeque<Value>,
}

struct Inner {
    scenarios: ScenarioService,
    runs: RunService,
    logger: SharedLogger,
    lanes: Mutex<HashMap<String, Lane>>,
}

pub struct ScenarioRunnerOptions {
    pub scenarios: ScenarioService,
    pub runs: RunService,
    pub logger: SharedLogger,
}

impl ScenarioRunnerOptions {
    pub fn new(scenarios: ScenarioService, runs: RunService) -> Self {
        Self {
            scenarios,
            runs,
            logger: silent_logger(),
        }
    }
}

#[derive(Clone)]
pub struct ScenarioRunner {
    inner: Arc<Inner>,
}

fn text_param<'a>(params: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn contains(event: &Value, key: &str, needle: &str) -> bool {
    event
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|value| value.to_lowercase().contains(&needle.to_lowercase()))
}

pub fn trigger_matches(scenario: &ScenarioDto, event: &Value) -> bool {
    let Some(trigger) = &scenario.trigger else {
        return false;
    };
    let params = &trigger.params;
    match trigger.kind {
        ScenarioTriggerKind::Chat => text_param(params, "conversationId").is_none_or(|wanted| {
            event.get("conversationId").and_then(Value::as_str) == Some(wanted)
        }),
        ScenarioTriggerKind::Mail => {
            text_param(params, "from").is_none_or(|from| contains(event, "from", from))
                && text_param(params, "subject")
                    .is_none_or(|subject| contains(event, "subject", subject))
        }
        ScenarioTriggerKind::Webhook => text_param(params, "path").is_none_or(|path| {
            event.get("path").and_then(Value::as_str).map(str::trim) == Some(path)
        }),
        ScenarioTriggerKind::Schedule => true,
        ScenarioTriggerKind::Integration => ["accountId", "trigger"].iter().all(|key| {
            text_param(params, key)
                .is_some_and(|wanted| event.get(*key).and_then(Value::as_str) == Some(wanted))
        }),
    }
}

fn copied_steps(run: &RunDto, steps: Vec<StepRow>, node_id: &str) -> Vec<StepRow> {
    let continues: HashMap<&str, bool> = run
        .graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.continues_on_error()))
        .collect();
    let mut latest: HashMap<String, StepRow> = HashMap::new();
    for step in steps {
        if step.node_id == node_id || !continues.contains_key(step.node_id.as_str()) {
            continue;
        }
        if latest
            .get(&step.node_id)
            .is_none_or(|previous| previous.attempt < step.attempt)
        {
            latest.insert(step.node_id.clone(), step);
        }
    }
    latest
        .into_values()
        .filter(|step| {
            step.status == StepStatus::Succeeded
                || (step.status == StepStatus::Failed
                    && continues
                        .get(step.node_id.as_str())
                        .copied()
                        .unwrap_or(false))
        })
        .collect()
}

impl ScenarioRunner {
    pub fn new(options: ScenarioRunnerOptions) -> Self {
        let runner = Self {
            inner: Arc::new(Inner {
                scenarios: options.scenarios,
                runs: options.runs,
                logger: options.logger,
                lanes: Mutex::new(HashMap::new()),
            }),
        };
        let watcher = runner.clone();
        runner
            .inner
            .runs
            .watch(Arc::new(move |run_id: &str, event: &HostEvent| {
                if matches!(event.body, HostEventDraft::End { .. }) {
                    watcher.finished(run_id);
                }
            }));
        runner
    }

    fn lanes(&self) -> MutexGuard<'_, HashMap<String, Lane>> {
        self.inner
            .lanes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.inner.logger.log(level, "scenarios", message, fields);
    }

    fn start(
        &self,
        scenario: &ScenarioDto,
        graph: RunGraph,
        title: String,
        input: Value,
    ) -> Result<RunHandleDto, AppError> {
        self.inner.runs.start(StartRunInput {
            subject_id: Some(scenario.id.to_string()),
            title: Some(title),
            input: Some(input),
            ..StartRunInput::new(RunKind::Scenario, graph)
        })
    }

    pub fn test(&self, input: &ScenarioRunInput) -> Result<RunHandleDto, AppError> {
        let detail = self.inner.scenarios.get(&input.id)?;
        let graph = input.graph.as_ref().unwrap_or(&detail.graph);
        let compiled = compile(graph, &self.inner.scenarios.catalogue())?;
        let event = input
            .input
            .clone()
            .or_else(|| trigger_input(graph))
            .unwrap_or_else(|| json!({}));
        self.start(
            &detail.scenario,
            compiled,
            format!("{} · {TEST_RUN_TITLE}", detail.scenario.name),
            event,
        )
    }

    pub fn run_node(&self, input: &ScenarioRunNodeInput) -> Result<RunHandleDto, AppError> {
        let detail = self.inner.scenarios.get(&input.id)?;
        let node = input
            .graph
            .nodes
            .iter()
            .find(|node| node.id == input.node_id);
        let pinned = input
            .input
            .clone()
            .or_else(|| node.and_then(|node| node.pinned_input.clone()))
            .unwrap_or_else(|| json!({}));
        let compiled = compile_node(
            &input.graph,
            &self.inner.scenarios.catalogue(),
            &input.node_id,
            &pinned,
        )?;
        let label = node
            .and_then(|node| node.name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| input.node_id.clone());
        self.start(
            &detail.scenario,
            compiled,
            format!("{} · шаг «{label}»", detail.scenario.name),
            pinned,
        )
    }

    pub fn retry_step(&self, input: &ScenarioRetryStepInput) -> Result<RunHandleDto, AppError> {
        let run = self.inner.runs.get(&input.run_id)?;
        if run.kind != RunKind::Scenario {
            return Err(AppError::new(AppErrorCode::Conflict, NOT_A_SCENARIO_RUN));
        }
        if !matches!(
            run.status,
            RunStatus::Failed | RunStatus::Interrupted | RunStatus::Cancelled
        ) {
            return Err(AppError::new(AppErrorCode::Conflict, RUN_NOT_FAILED));
        }
        let node = run
            .graph
            .nodes
            .iter()
            .find(|node| node.id == input.node_id)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NODE_NOT_IN_RUN))?;
        let idempotent = self
            .inner
            .scenarios
            .catalogue()
            .spec(&node.node_type)
            .is_some_and(|spec| spec.idempotent);
        if !idempotent {
            return Err(AppError::new(AppErrorCode::Conflict, STEP_NOT_IDEMPOTENT));
        }
        let run_id = run.id.to_string();
        let steps = self.inner.runs.step_rows(&run_id)?;
        let failed = steps
            .iter()
            .filter(|step| step.node_id == input.node_id)
            .max_by_key(|step| step.attempt)
            .is_some_and(|step| {
                matches!(
                    step.status,
                    StepStatus::Failed | StepStatus::Cancelled | StepStatus::Abandoned
                )
            });
        if !failed {
            return Err(AppError::new(AppErrorCode::Conflict, STEP_NOT_FAILED));
        }
        let copies = copied_steps(&run, steps, &input.node_id);
        let start = StartRunInput {
            subject_id: run.subject_id.clone(),
            title: run.title.clone(),
            retry_of_id: Some(run.id),
            input: Some(run.input.clone()),
            concurrency: Some(run.concurrency),
            ..StartRunInput::new(RunKind::Scenario, run.graph.clone())
        };
        let (handle, _) = self.inner.runs.start_with(
            start,
            |repositories: &Repositories<'_>, new_run: &RunId| {
                for step in &copies {
                    repositories.runs.add_step(&StepRow {
                        id: new_id().hyphenated().to_string(),
                        run_id: new_run.to_string(),
                        ..step.clone()
                    })?;
                }
                Ok(())
            },
            None,
        )?;
        Ok(handle)
    }

    pub fn fire(
        &self,
        kind: ScenarioTriggerKind,
        event: &Value,
    ) -> Result<Vec<(ScenarioDto, Fired)>, AppError> {
        let mut outcomes = Vec::new();
        for scenario in self.inner.scenarios.active_for_trigger(kind)? {
            if !trigger_matches(&scenario, event) {
                continue;
            }
            match self.admit(&scenario, event.clone()) {
                Ok(fired) => outcomes.push((scenario, fired)),
                Err(error) => self.log(
                    LogLevel::Warn,
                    "Could not start a triggered scenario",
                    &[
                        ("scenarioId", json!(scenario.id)),
                        ("error", json!(error.message)),
                    ],
                ),
            }
        }
        Ok(outcomes)
    }

    pub fn integration_sink(&self) -> crate::integrations::TriggerSink {
        let runner = self.clone();
        Arc::new(move |event: &Value| {
            Ok(runner
                .fire(ScenarioTriggerKind::Integration, event)?
                .into_iter()
                .filter(|(_, fired)| !matches!(fired, Fired::Skipped))
                .map(|(scenario, _)| scenario.name)
                .collect())
        })
    }

    pub fn chat_listener(&self) -> crate::services::chat::ChatMessageListener {
        let runner = self.clone();
        Arc::new(move |event: &Value| {
            if let Err(error) = runner.fire(ScenarioTriggerKind::Chat, event) {
                runner.log(
                    LogLevel::Warn,
                    "Could not fire chat-triggered scenarios",
                    &[("error", json!(error.message))],
                );
            }
        })
    }

    fn admit(&self, scenario: &ScenarioDto, event: Value) -> Result<Fired, AppError> {
        let key = scenario.id.to_string();
        {
            let mut lanes = self.lanes();
            let lane = lanes.entry(key.clone()).or_default();
            if lane.running.is_some() {
                return Ok(match scenario.overlap {
                    ScenarioOverlap::Skip => {
                        self.log(
                            LogLevel::Info,
                            "Skipped a trigger while the scenario is running",
                            &[("scenarioId", json!(key))],
                        );
                        Fired::Skipped
                    }
                    ScenarioOverlap::Queue => {
                        lane.queued.push_back(event);
                        Fired::Queued
                    }
                });
            }
            lane.running = Some(String::new());
        }
        match self.launch(scenario, event) {
            Ok(handle) => {
                self.claim(&key, &handle);
                Ok(Fired::Started(handle))
            }
            Err(error) => {
                self.release(&key);
                Err(error)
            }
        }
    }

    fn launch(&self, scenario: &ScenarioDto, event: Value) -> Result<RunHandleDto, AppError> {
        let (version, graph) = self.inner.scenarios.version(&scenario.current_version_id)?;
        let compiled = compile(&graph, &self.inner.scenarios.catalogue())?;
        self.start(
            scenario,
            compiled,
            format!("{} · v{}", scenario.name, version.version),
            event,
        )
    }

    fn claim(&self, key: &str, handle: &RunHandleDto) {
        let id = handle.id.to_string();
        let finished = self
            .inner
            .runs
            .get(&handle.id)
            .is_ok_and(|run| !run.status.is_unfinished());
        if let Some(lane) = self.lanes().get_mut(key) {
            lane.running = Some(id.clone());
        }
        if finished {
            self.finished(&id);
        }
    }

    fn release(&self, key: &str) {
        if let Some(lane) = self.lanes().get_mut(key) {
            lane.running = None;
        }
    }

    fn finished(&self, run_id: &str) {
        let next = {
            let mut lanes = self.lanes();
            let Some((key, lane)) = lanes
                .iter_mut()
                .find(|(_, lane)| lane.running.as_deref() == Some(run_id))
            else {
                return;
            };
            lane.running = None;
            lane.queued.pop_front().map(|event| (key.clone(), event))
        };
        let Some((key, event)) = next else {
            return;
        };
        let runner = self.clone();
        let work = move || runner.resume(&key, event);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move { work() });
            }
            Err(_) => work(),
        }
    }

    fn resume(&self, key: &str, event: Value) {
        let scenario = zvs_contract::ScenarioId::parse(key)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NOT_A_SCENARIO_RUN))
            .and_then(|id| self.inner.scenarios.get(&id));
        let outcome = scenario.and_then(|detail| {
            if !detail.scenario.active {
                return Ok(None);
            }
            self.admit(&detail.scenario, event).map(Some)
        });
        if let Err(error) = outcome {
            self.log(
                LogLevel::Warn,
                "Could not start a queued scenario run",
                &[("scenarioId", json!(key)), ("error", json!(error.message))],
            );
        }
    }

    pub fn queued(&self, scenario_id: &str) -> usize {
        self.lanes()
            .get(scenario_id)
            .map_or(0, |lane| lane.queued.len())
    }
}

fn trigger_input(graph: &ScenarioGraphDto) -> Option<Value> {
    graph
        .nodes
        .iter()
        .filter(|node| !node.is_disabled())
        .find(|node| node.node_type.starts_with("trigger."))
        .and_then(|node| node.pinned_input.clone())
}
