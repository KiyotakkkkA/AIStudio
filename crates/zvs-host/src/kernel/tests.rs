use std::future::{Future, pending};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::stream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, DeltaKind, HostEvent, HostEventDraft, RunGraph,
    RunHandleDto, RunId, RunKind, RunListFilter, RunStatus, StartRunInput, StepStatus, StreamId,
    ToolTier, Validate, VectorSearchHitDto, validate::invalid,
};

use super::*;
use crate::data::Database;
use crate::data::repositories::{Repositories, RunDraft};
use crate::data::schema::StepRow;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::{
    DeltaStream, DiscoveredModel, GenerateRequest, GenerateResult, SharedTextDriver, TextDelta,
    TextGenerationDriver,
};
use crate::platform::events::{EventBus, EventSink};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, silent_logger};
use crate::services::runs::{RunService, RunServiceOptions};
use crate::services::vector_stores::VectorSearchOptions;
use crate::testing::{
    CLOCK_ORIGIN, FAKE_CAPABILITIES, MemoryLogger, MemorySink, TemporaryDatabase, TokioClock,
    temporary_database,
};

#[derive(Clone)]
struct Latch(Arc<watch::Sender<bool>>);

impl Latch {
    fn new() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }

    fn open(&self) {
        self.0.send_replace(true);
    }

    async fn wait(&self) {
        let mut receiver = self.0.subscribe();
        let _ = receiver.wait_for(|open| *open).await;
    }
}

struct Harness {
    service: RunService,
    registry: Arc<NodeRegistry>,
    bus: EventBus,
    sink: Arc<MemorySink>,
    logger: Arc<MemoryLogger>,
}

impl Harness {
    fn sent(&self) -> Vec<HostEvent> {
        self.sink.events()
    }

    fn steps(&self, id: &RunId) -> Vec<zvs_contract::StepDto> {
        self.service.steps(id).expect("steps")
    }

    fn status(&self, id: &RunId) -> RunStatus {
        self.service.get(id).expect("a run").status
    }

    fn statuses(&self, id: &RunId) -> Vec<StepStatus> {
        self.steps(id).into_iter().map(|step| step.status).collect()
    }

    fn ends(&self) -> usize {
        self.sent()
            .iter()
            .filter(|event| matches!(event.body, HostEventDraft::End { .. }))
            .count()
    }
}

fn harness_with(
    database: Arc<Database>,
    registry: Arc<NodeRegistry>,
    configure: impl FnOnce(&mut RunServiceOptions),
) -> Harness {
    let sink = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(sink.clone());
    let logger = MemoryLogger::shared();
    let mut options =
        RunServiceOptions::new(Arc::clone(&database), bus.clone(), Arc::clone(&registry));
    options.grace = Duration::from_millis(20);
    options.logger = logger.clone();
    options.clock = Arc::new(TokioClock::new(CLOCK_ORIGIN));
    configure(&mut options);
    Harness {
        service: RunService::new(options).expect("a run service"),
        registry,
        bus,
        sink,
        logger,
    }
}

fn harness(database: &TemporaryDatabase) -> Harness {
    harness_with(database.shared(), Arc::new(NodeRegistry::new()), |_| {})
}

fn register<F, Fut>(registry: &NodeRegistry, node_type: &str, side_effect_free: bool, run: F)
where
    F: Fn(StepContext, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, AppError>> + Send + 'static,
{
    registry
        .register(FnNode::new(
            node_type,
            PermissionRequirement::None,
            !side_effect_free,
            run,
        ))
        .expect("registers");
}

fn start_input(kind: &str, nodes: Value, concurrency: u64, input: Value) -> StartRunInput {
    zvs_contract::parse(json!({
        "kind": kind,
        "graph": { "nodes": nodes },
        "concurrency": concurrency,
        "input": input,
    }))
    .expect("a start input")
}

fn start(harness: &Harness, nodes: Value) -> RunHandleDto {
    try_start(harness, nodes, 4, Value::Null).expect("starts")
}

fn try_start(
    harness: &Harness,
    nodes: Value,
    concurrency: u64,
    input: Value,
) -> Result<RunHandleDto, AppError> {
    harness
        .service
        .start(start_input("job", nodes, concurrency, input))
}

async fn eventually(mut condition: impl FnMut() -> bool) {
    for _ in 0..5_000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the condition never held");
}

#[tokio::test]
async fn diamond_dependencies_deterministic_ready_order_input_bindings_and_checkpoints() {
    let database = temporary_database();
    let host = harness(&database);
    let order = Arc::new(Mutex::new(Vec::new()));
    for node_type in ["a", "b", "c", "d"] {
        let order = Arc::clone(&order);
        register(&host.registry, node_type, true, move |_context, input| {
            let order = Arc::clone(&order);
            async move {
                order.lock().expect("order").push(node_type);
                Ok(json!({ "type": node_type, "input": input }))
            }
        });
    }
    let handle = try_start(
        &host,
        json!([
            {
                "id": "d",
                "type": "d",
                "dependencies": ["b", "c"],
                "input": {},
                "bindings": {
                    "left": { "source": "node", "nodeId": "b" },
                    "right": { "source": "node", "nodeId": "c" }
                }
            },
            { "id": "c", "type": "c", "dependencies": ["a"] },
            { "id": "b", "type": "b", "dependencies": ["a"] },
            { "id": "a", "type": "a", "input": {}, "bindings": { "original": { "source": "run" } } }
        ]),
        2,
        json!({ "query": "hello" }),
    )
    .expect("starts");
    assert_eq!(host.status(&handle.id), RunStatus::Queued);
    host.service.wait(&handle.id).await;
    assert_eq!(*order.lock().expect("order"), ["a", "b", "c", "d"]);
    assert_eq!(host.status(&handle.id), RunStatus::Succeeded);
    let steps = host.steps(&handle.id);
    assert_eq!(host.statuses(&handle.id), vec![StepStatus::Succeeded; 4]);
    assert_eq!(steps[0].input, json!({ "original": { "query": "hello" } }));
    assert_eq!(
        steps[3].input,
        json!({
            "left": { "type": "b", "input": null },
            "right": { "type": "c", "input": null }
        })
    );
    for step in &steps {
        assert!(step.finished_at.expect("finished") >= step.started_at);
        assert_eq!(step.attempt, 1);
    }
}

#[tokio::test]
async fn concurrency_is_bounded_and_a_freed_slot_is_refilled_while_another_node_still_runs() {
    let database = temporary_database();
    let host = harness(&database);
    let gates: Arc<Vec<Latch>> = Arc::new((0..3).map(|_| Latch::new()).collect());
    let entered: Arc<Vec<Latch>> = Arc::new((0..3).map(|_| Latch::new()).collect());
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    {
        let (gates, entered, active, peak) = (
            Arc::clone(&gates),
            Arc::clone(&entered),
            Arc::clone(&active),
            Arc::clone(&peak),
        );
        register(&host.registry, "work", true, move |_context, input| {
            let (gates, entered, active, peak) = (
                Arc::clone(&gates),
                Arc::clone(&entered),
                Arc::clone(&active),
                Arc::clone(&peak),
            );
            async move {
                let index = usize::try_from(input.as_u64().expect("an index")).expect("small");
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                entered[index].open();
                gates[index].wait().await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(Value::Null)
            }
        });
    }
    let handle = try_start(
        &host,
        json!([0, 1, 2].map(|id| json!({ "id": id.to_string(), "type": "work", "input": id }))),
        2,
        Value::Null,
    )
    .expect("starts");
    entered[0].wait().await;
    entered[1].wait().await;
    assert_eq!(host.steps(&handle.id).len(), 2);
    gates[1].open();
    entered[2].wait().await;
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    gates[0].open();
    gates[2].open();
    host.service.wait(&handle.id).await;
    assert_eq!(host.status(&handle.id), RunStatus::Succeeded);
}

#[tokio::test]
async fn cooperative_cancellation_is_prompt_and_prevents_dependent_nodes() {
    let database = temporary_database();
    let host = harness(&database);
    let entered = Latch::new();
    {
        let entered = entered.clone();
        register(&host.registry, "wait", true, move |context, _input| {
            let entered = entered.clone();
            async move {
                entered.open();
                context.cancel.cancelled().await;
                Err(types::cancelled())
            }
        });
    }
    let handle = start(
        &host,
        json!([
            { "id": "a", "type": "wait" },
            { "id": "b", "type": "wait", "dependencies": ["a"] }
        ]),
    );
    entered.wait().await;
    let began = Instant::now();
    host.service.cancel(&handle.id).await.expect("cancels");
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(host.status(&handle.id), RunStatus::Cancelled);
    assert_eq!(host.statuses(&handle.id), [StepStatus::Cancelled]);
    host.service
        .cancel(&handle.id)
        .await
        .expect("cancels again");
    assert_eq!(host.ends(), 1);
}

#[tokio::test]
async fn uncooperative_steps_are_abandoned_after_grace_and_late_output_and_events_cannot_change_history()
 {
    let database = temporary_database();
    let host = harness(&database);
    let entered: Arc<Mutex<Option<StepContext>>> = Arc::new(Mutex::new(None));
    let ready = Latch::new();
    let gate = Latch::new();
    {
        let (entered, ready, gate) = (Arc::clone(&entered), ready.clone(), gate.clone());
        register(&host.registry, "stuck", true, move |context, _input| {
            let (entered, ready, gate) = (Arc::clone(&entered), ready.clone(), gate.clone());
            async move {
                *entered.lock().expect("context") = Some(context);
                ready.open();
                gate.wait().await;
                Ok(Value::Null)
            }
        });
    }
    let handle = start(&host, json!([{ "id": "a", "type": "stuck" }]));
    ready.wait().await;
    let context = entered.lock().expect("context").clone().expect("entered");
    let began = Instant::now();
    host.service.cancel(&handle.id).await.expect("cancels");
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(host.statuses(&handle.id), [StepStatus::Abandoned]);
    let abandoned = host.logger.matching("Abandoned node ignored cancellation");
    assert_eq!(abandoned.len(), 1);
    assert_eq!(abandoned[0].level, LogLevel::Error);
    assert_eq!(abandoned[0].scope, "kernel");
    let count = host.sent().len();
    gate.open();
    context
        .emit(HostEventDraft::Log {
            line: zvs_contract::log_line(&[("late", json!(true))]),
        })
        .expect("ignored, not refused");
    tokio::task::yield_now().await;
    assert_eq!(host.sent().len(), count);
    assert_eq!(host.status(&handle.id), RunStatus::Cancelled);
    assert_eq!(host.statuses(&handle.id), [StepStatus::Abandoned]);
}

#[tokio::test]
async fn cancel_before_the_queued_run_starts_invokes_no_nodes() {
    let database = temporary_database();
    let host = harness(&database);
    let calls = Arc::new(AtomicUsize::new(0));
    {
        let calls = Arc::clone(&calls);
        register(&host.registry, "work", true, move |_context, _input| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok(Value::Null) }
        });
    }
    let handle = start(&host, json!([{ "id": "a", "type": "work" }]));
    host.service.cancel(&handle.id).await.expect("cancels");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(host.status(&handle.id), RunStatus::Cancelled);
}

#[tokio::test(start_paused = true)]
async fn retry_failures_are_separate_persisted_attempts_with_bounded_linear_backoff() {
    let database = temporary_database();
    let host = harness(&database);
    let calls = Arc::new(AtomicUsize::new(0));
    {
        let calls = Arc::clone(&calls);
        register(&host.registry, "retry", true, move |_context, _input| {
            let attempt = calls.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if attempt < 3 {
                    Err(AppError::new(AppErrorCode::Unknown, "temporary"))
                } else {
                    Ok(json!("done"))
                }
            }
        });
    }
    let handle = start(
        &host,
        json!([{ "id": "a", "type": "retry", "retry": { "maxAttempts": 3, "backoffMs": 15 } }]),
    );
    host.service.wait(&handle.id).await;
    let steps = host.steps(&handle.id);
    assert_eq!(
        steps
            .iter()
            .map(|step| (step.attempt, step.status))
            .collect::<Vec<_>>(),
        [
            (1, StepStatus::Failed),
            (2, StepStatus::Failed),
            (3, StepStatus::Succeeded)
        ]
    );
    assert!(steps[1].started_at - steps[0].finished_at.expect("finished") >= 15);
    assert!(steps[2].started_at - steps[1].finished_at.expect("finished") >= 30);
    assert_eq!(steps[0].error.as_deref(), Some("temporary"));
    assert_eq!(steps[2].output, Some(json!("done")));
}

#[tokio::test(start_paused = true)]
async fn cancellation_interrupts_retry_backoff_without_waiting_for_it() {
    let database = temporary_database();
    let host = harness(&database);
    register(&host.registry, "retry", true, |_context, _input| async {
        Err(AppError::new(AppErrorCode::Unknown, "temporary"))
    });
    let handle = start(
        &host,
        json!([{ "id": "a", "type": "retry", "retry": { "maxAttempts": 3, "backoffMs": 60_000 } }]),
    );
    eventually(|| host.statuses(&handle.id).first() == Some(&StepStatus::Failed)).await;
    let began = tokio::time::Instant::now();
    host.service.cancel(&handle.id).await.expect("cancels");
    assert!(began.elapsed() < Duration::from_secs(60));
    assert_eq!(host.status(&handle.id), RunStatus::Cancelled);
    assert_eq!(host.steps(&handle.id).len(), 1);
}

#[tokio::test]
async fn exhausted_failure_aborts_siblings_and_never_schedules_dependents() {
    let database = temporary_database();
    let host = harness(&database);
    register(&host.registry, "fail", true, |_context, _input| async {
        Err(AppError::new(AppErrorCode::Unknown, "permanent"))
    });
    register(&host.registry, "stuck", true, |_context, _input| async {
        pending::<Result<Value, AppError>>().await
    });
    let handle = start(
        &host,
        json!([
            { "id": "a", "type": "fail" },
            { "id": "b", "type": "stuck" },
            { "id": "c", "type": "fail", "dependencies": ["a"] }
        ]),
    );
    host.service.wait(&handle.id).await;
    let run = host.service.get(&handle.id).expect("a run");
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.error.as_deref(), Some("permanent"));
    assert_eq!(
        host.statuses(&handle.id),
        [StepStatus::Failed, StepStatus::Abandoned]
    );
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
struct Positive(f64);

impl Validate for Positive {
    fn validate(&self) -> Result<(), AppError> {
        if self.0 > 0.0 {
            Ok(())
        } else {
            Err(invalid("", "positive"))
        }
    }
}

#[tokio::test]
async fn input_and_output_schemas_are_enforced_and_invalid_output_is_never_checkpointed() {
    let database = temporary_database();
    let host = harness(&database);
    let calls = Arc::new(AtomicUsize::new(0));
    {
        let calls = Arc::clone(&calls);
        host.registry
            .register(FnNode::new(
                "validated",
                PermissionRequirement::None,
                false,
                move |_context, _input: Positive| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(Positive(-1.0)) }
                },
            ))
            .expect("registers");
    }
    let first = start(
        &host,
        json!([{ "id": "a", "type": "validated", "input": "bad" }]),
    );
    host.service.wait(&first.id).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let second = start(
        &host,
        json!([{ "id": "a", "type": "validated", "input": 1 }]),
    );
    host.service.wait(&second.id).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let step = &host.steps(&second.id)[0];
    assert_eq!(step.status, StepStatus::Failed);
    assert_eq!(step.output, None);
}

#[tokio::test]
async fn invalid_graphs_create_neither_rows_nor_streams() {
    let database = temporary_database();
    let host = harness(&database);
    register(&host.registry, "work", true, |_context, _input| async {
        Ok(Value::Null)
    });
    for nodes in [
        json!([{ "id": "a", "type": "work" }, { "id": "a", "type": "work" }]),
        json!([{ "id": "a", "type": "work", "dependencies": ["missing"] }]),
        json!([
            { "id": "a", "type": "work", "dependencies": ["b"] },
            { "id": "b", "type": "work", "dependencies": ["a"] }
        ]),
        json!([{ "id": "a", "type": "unknown" }]),
        json!([{
            "id": "a",
            "type": "work",
            "input": {},
            "bindings": { "x": { "source": "node", "nodeId": "missing" } }
        }]),
        json!([{
            "id": "a",
            "type": "work",
            "bindings": { "x": { "source": "run" } }
        }]),
        json!([{ "id": "a", "type": "work", "dependencies": ["a", "a"] }]),
    ] {
        let refused = try_start(&host, nodes.clone(), 4, Value::Null).expect_err("refused");
        assert_eq!(refused.code, AppErrorCode::ValidationFailed, "{nodes}");
    }
    assert!(
        host.service
            .list(&RunListFilter::default())
            .expect("lists")
            .items
            .is_empty()
    );
    assert_eq!(host.bus.open(), 0);
}

#[tokio::test]
async fn all_state_changes_use_one_stream_with_monotonic_sequence_and_exactly_one_terminal_event() {
    let database = temporary_database();
    let host = harness(&database);
    register(&host.registry, "work", true, |context, _input| async move {
        context.emit(HostEventDraft::Token {
            delta: "hello".to_owned(),
            kind: None,
        })?;
        Ok(Value::Null)
    });
    let handle = start(&host, json!([{ "id": "a", "type": "work" }]));
    host.service.wait(&handle.id).await;
    let sent = host.sent();
    assert!(sent.iter().all(|event| event.stream_id == handle.stream_id));
    assert_eq!(
        sent.iter().map(|event| event.seq).collect::<Vec<_>>(),
        (0..sent.len() as u64).collect::<Vec<_>>()
    );
    assert!(matches!(
        &sent[0].body,
        HostEventDraft::Log { line } if line.get("status") == Some(&json!("queued"))
    ));
    assert!(matches!(
        &sent.last().expect("events").body,
        HostEventDraft::End { outcome } if outcome.status == zvs_contract::RunOutcomeStatus::Ok
    ));
    assert_eq!(
        sent.iter()
            .filter_map(|event| match &event.body {
                HostEventDraft::Step { step } => step.get("status").cloned(),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [json!("running"), json!("succeeded")]
    );
    assert!(sent.iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Token { delta, .. } if delta == "hello"
    )));
    assert_eq!(
        host.service
            .record(&handle.id.to_string())
            .expect("persisted"),
        sent
    );
}

#[tokio::test]
async fn a_node_cannot_end_the_run_or_emit_an_invalid_event() {
    let database = temporary_database();
    let host = harness(&database);
    register(
        &host.registry,
        "rogue",
        true,
        |context, _input| async move {
            let ended = context.emit(HostEventDraft::End {
                outcome: zvs_contract::RunOutcomeDto {
                    status: zvs_contract::RunOutcomeStatus::Ok,
                    code: None,
                    message: None,
                },
            });
            assert_eq!(
                ended.expect_err("refused").message,
                scheduler::ONLY_KERNEL_ENDS
            );
            context.emit(HostEventDraft::Progress {
                done: u64::MAX,
                total: 1,
            })?;
            Ok(Value::Null)
        },
    );
    let handle = start(&host, json!([{ "id": "a", "type": "rogue" }]));
    host.service.wait(&handle.id).await;
    assert_eq!(host.statuses(&handle.id), [StepStatus::Failed]);
    assert_eq!(host.ends(), 1);
}

struct Checkpoint {
    id: RunId,
    stream_id: StreamId,
    graph: RunGraph,
}

fn checkpoint(database: &Database, nodes: Value, status: RunStatus) -> Checkpoint {
    let input = start_input("job", nodes, 4, Value::Null);
    let id = RunId::new(new_id());
    let stream_id = StreamId::new(new_id());
    let graph = input.graph.clone().normalized();
    database
        .with_connection(|connection| {
            let runs = Repositories::new(connection).runs;
            runs.create(&RunDraft {
                id: id.to_string(),
                kind: RunKind::Job,
                subject_id: None,
                title: None,
                retry_of_id: None,
                status,
                graph: graph.clone(),
                input: Value::Null,
                stream_id: stream_id.to_string(),
                concurrency: 4,
                created_at: 1,
                started_at: Some(2),
            })?;
            runs.append_event(
                &id.to_string(),
                &HostEvent::new(
                    stream_id,
                    7,
                    2,
                    HostEventDraft::Log {
                        line: zvs_contract::log_line(&[("status", json!(status))]),
                    },
                ),
            )?;
            Ok(())
        })
        .expect("a checkpoint");
    Checkpoint {
        id,
        stream_id,
        graph,
    }
}

fn saved_step(database: &Database, run: &Checkpoint, index: usize, status: StepStatus) {
    let node = &run.graph.nodes[index];
    database
        .with_connection(|connection| {
            Repositories::new(connection).runs.add_step(&StepRow {
                id: new_id().hyphenated().to_string(),
                run_id: run.id.to_string(),
                node_id: node.id.clone(),
                step_type: node.node_type.clone(),
                status,
                input: node.input().clone(),
                output: if status == StepStatus::Succeeded {
                    json!("saved")
                } else {
                    Value::Null
                },
                error: None,
                started_at: 2,
                finished_at: (status != StepStatus::Running).then_some(3),
                attempt: 1,
            })?;
            Ok(())
        })
        .expect("a saved step");
}

fn counting(
    registry: &NodeRegistry,
    node_type: &str,
    side_effect_free: bool,
    result: Value,
) -> Arc<AtomicUsize> {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    register(
        registry,
        node_type,
        side_effect_free,
        move |_context, _input| {
            counter.fetch_add(1, Ordering::SeqCst);
            let result = result.clone();
            async move { Ok(result) }
        },
    );
    calls
}

#[tokio::test]
async fn boot_resumes_safe_runs_from_a_durable_checkpoint_using_the_original_stream_and_sequence() {
    for status in [RunStatus::Queued, RunStatus::Running, RunStatus::Blocked] {
        let database = temporary_database();
        let run = checkpoint(
            &database,
            json!([
                { "id": "a", "type": "safe" },
                { "id": "b", "type": "safe", "dependencies": ["a"] }
            ]),
            status,
        );
        saved_step(&database, &run, 0, StepStatus::Succeeded);
        saved_step(&database, &run, 1, StepStatus::Running);
        let reopened = Arc::new(
            Database::open(&database.directory().join("studio.sqlite"), silent_logger())
                .expect("reopens"),
        );
        let host = harness_with(reopened, Arc::new(NodeRegistry::new()), |_| {});
        let calls = counting(&host.registry, "safe", true, json!("new"));
        host.service.recover().expect("recovers");
        host.service.recover().expect("recovers twice");
        host.service.wait(&run.id).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{status}");
        assert_eq!(
            host.steps(&run.id)
                .into_iter()
                .map(|step| (step.node_id, step.attempt, step.status))
                .collect::<Vec<_>>(),
            [
                ("a".to_owned(), 1, StepStatus::Succeeded),
                ("b".to_owned(), 1, StepStatus::Abandoned),
                ("b".to_owned(), 2, StepStatus::Succeeded),
            ]
        );
        let stored = host.service.get(&run.id).expect("a run");
        assert_eq!(stored.status, RunStatus::Succeeded);
        assert_eq!(stored.stream_id, run.stream_id);
        assert_eq!(stored.started_at, Some(2));
        let sent = host.sent();
        assert_eq!(sent[0].seq, 8);
        assert_eq!(
            sent.iter().map(|event| event.seq).collect::<Vec<_>>(),
            (8..8 + sent.len() as u64).collect::<Vec<_>>()
        );
        host.service.dispose().await;
    }
}

#[tokio::test]
async fn boot_marks_a_graph_containing_an_unsafe_or_missing_node_interrupted_without_executing_any_node()
 {
    for node_type in ["unsafe", "missing"] {
        let database = temporary_database();
        let host = harness(&database);
        let calls = counting(&host.registry, "safe", true, Value::Null);
        let unsafe_calls = counting(&host.registry, "unsafe", false, Value::Null);
        let run = checkpoint(
            &database,
            json!([{ "id": "a", "type": "safe" }, { "id": "b", "type": node_type }]),
            RunStatus::Blocked,
        );
        saved_step(&database, &run, 0, StepStatus::Running);
        host.service.recover().expect("recovers");
        host.service.recover().expect("recovers twice");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(unsafe_calls.load(Ordering::SeqCst), 0);
        assert_eq!(host.status(&run.id), RunStatus::Interrupted);
        assert_eq!(host.statuses(&run.id), [StepStatus::Abandoned]);
        assert_eq!(host.ends(), 1);
    }
}

#[tokio::test]
async fn recovery_respects_exhausted_retries_and_never_reexecutes_checkpointed_success() {
    let database = temporary_database();
    let host = harness(&database);
    let calls = counting(&host.registry, "safe", true, Value::Null);
    let failed = checkpoint(
        &database,
        json!([{ "id": "a", "type": "safe" }]),
        RunStatus::Running,
    );
    saved_step(&database, &failed, 0, StepStatus::Failed);
    host.service.recover().expect("recovers");
    host.service.wait(&failed.id).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(host.status(&failed.id), RunStatus::Failed);
    let complete = checkpoint(
        &database,
        json!([{ "id": "a", "type": "safe" }]),
        RunStatus::Running,
    );
    saved_step(&database, &complete, 0, StepStatus::Succeeded);
    host.service.recover().expect("recovers");
    host.service.wait(&complete.id).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(host.status(&complete.id), RunStatus::Succeeded);
}

#[tokio::test]
async fn shutdown_leaves_a_resumable_run_and_fences_abandoned_work_before_database_close() {
    let database = temporary_database();
    let host = harness(&database);
    let entered = Latch::new();
    {
        let entered = entered.clone();
        register(&host.registry, "safe", true, move |_context, _input| {
            let entered = entered.clone();
            async move {
                entered.open();
                pending::<Result<Value, AppError>>().await
            }
        });
    }
    let handle = start(&host, json!([{ "id": "a", "type": "safe" }]));
    entered.wait().await;
    host.service.dispose().await;
    assert_eq!(host.status(&handle.id), RunStatus::Running);
    assert_eq!(host.statuses(&handle.id), [StepStatus::Abandoned]);
    assert_eq!(host.ends(), 0);
    assert_eq!(
        try_start(&host, json!([]), 4, Value::Null)
            .expect_err("closed")
            .code,
        AppErrorCode::Conflict
    );
    host.bus.dispose();

    let next = harness(&database);
    counting(&next.registry, "safe", true, Value::Null);
    next.service.recover().expect("recovers");
    next.service.wait(&handle.id).await;
    assert_eq!(next.status(&handle.id), RunStatus::Succeeded);
}

fn gated_approval(entered: Latch, decision: Latch) -> scheduler::ApprovalHook {
    Arc::new(move |_request, _cancel| {
        let (entered, decision) = (entered.clone(), decision.clone());
        Box::pin(async move {
            entered.open();
            decision.wait().await;
            Ok(ApprovalDecision::Approved)
        })
    })
}

fn register_gate(registry: &NodeRegistry) {
    register(registry, "gate", true, |context, _input| async move {
        let mut request = ApprovalRequestDto::new();
        request.insert("tool".to_owned(), json!("test"));
        let decision = context.request_approval(request).await?;
        Ok(json!(decision.as_str()))
    });
}

#[tokio::test]
async fn the_approval_hook_blocks_and_resumes_the_run_with_the_same_event_stream() {
    let database = temporary_database();
    let (entered, decision) = (Latch::new(), Latch::new());
    let hook = gated_approval(entered.clone(), decision.clone());
    let host = harness_with(
        database.shared(),
        Arc::new(NodeRegistry::new()),
        |options| {
            options.approval = Some(hook);
        },
    );
    register_gate(&host.registry);
    let handle = start(&host, json!([{ "id": "a", "type": "gate" }]));
    entered.wait().await;
    assert_eq!(host.status(&handle.id), RunStatus::Blocked);
    decision.open();
    host.service.wait(&handle.id).await;
    assert_eq!(host.status(&handle.id), RunStatus::Succeeded);
    assert_eq!(host.steps(&handle.id)[0].output, Some(json!("approved")));
    assert!(
        host.sent()
            .iter()
            .any(|event| matches!(event.body, HostEventDraft::Approval { .. }))
    );
}

#[tokio::test]
async fn cancellation_bounds_approval_waits_and_a_late_approval_cannot_revive_a_run() {
    let database = temporary_database();
    let (entered, decision) = (Latch::new(), Latch::new());
    let hook = gated_approval(entered.clone(), decision.clone());
    let host = harness_with(
        database.shared(),
        Arc::new(NodeRegistry::new()),
        |options| {
            options.approval = Some(hook);
        },
    );
    register_gate(&host.registry);
    let handle = start(&host, json!([{ "id": "a", "type": "gate" }]));
    entered.wait().await;
    host.service.cancel(&handle.id).await.expect("cancels");
    decision.open();
    tokio::task::yield_now().await;
    assert_eq!(host.status(&handle.id), RunStatus::Cancelled);
    assert!(matches!(
        host.sent().last().map(|event| &event.body),
        Some(HostEventDraft::End { .. })
    ));
}

#[tokio::test]
async fn node_mutations_cannot_rewrite_recorded_inputs_or_another_nodes_dependency_output() {
    let database = temporary_database();
    let host = harness(&database);
    register(
        &host.registry,
        "mutate",
        true,
        |_context, mut input| async move {
            input
                .as_object_mut()
                .expect("an object")
                .insert("changed".to_owned(), json!(true));
            Ok(input)
        },
    );
    let handle = start(
        &host,
        json!([{ "id": "a", "type": "mutate", "input": { "original": true } }]),
    );
    host.service.wait(&handle.id).await;
    let step = &host.steps(&handle.id)[0];
    assert_eq!(step.input, json!({ "original": true }));
    assert_eq!(
        step.output,
        Some(json!({ "original": true, "changed": true }))
    );

    let shared = Arc::new(Mutex::new(json!({ "value": "original" })));
    {
        let shared = Arc::clone(&shared);
        register(&host.registry, "source", true, move |_context, _input| {
            let value = shared.lock().expect("shared").clone();
            async move { Ok(value) }
        });
    }
    {
        let shared = Arc::clone(&shared);
        register(&host.registry, "mutator", true, move |_context, _input| {
            *shared.lock().expect("shared") = json!({ "value": "changed" });
            async { Ok(Value::Null) }
        });
    }
    register(
        &host.registry,
        "consumer",
        true,
        |_context, input| async move { Ok(input) },
    );
    let second = start(
        &host,
        json!([
            { "id": "a", "type": "source" },
            { "id": "b", "type": "mutator", "dependencies": ["a"] },
            {
                "id": "c",
                "type": "consumer",
                "dependencies": ["a", "b"],
                "input": {},
                "bindings": { "saved": { "source": "node", "nodeId": "a" } }
            }
        ]),
    );
    host.service.wait(&second.id).await;
    assert_eq!(
        host.steps(&second.id)[2].output,
        Some(json!({ "saved": { "value": "original" } }))
    );
}

struct FailingSink;

impl EventSink for FailingSink {
    fn deliver(&self, _event: &HostEvent) -> Result<(), String> {
        Err("window closed".to_owned())
    }
}

#[tokio::test]
async fn renderer_delivery_failure_cannot_roll_back_a_checkpoint_or_fail_the_run() {
    let database = temporary_database();
    let host = harness(&database);
    host.bus.attach(Arc::new(FailingSink));
    register(&host.registry, "work", true, |_context, _input| async {
        Ok(json!("saved"))
    });
    let handle = start(&host, json!([{ "id": "a", "type": "work" }]));
    host.service.wait(&handle.id).await;
    assert_eq!(host.status(&handle.id), RunStatus::Succeeded);
    assert_eq!(host.steps(&handle.id)[0].output, Some(json!("saved")));
    assert!(matches!(
        host.service
            .record(&handle.id.to_string())
            .expect("events")
            .last()
            .map(|event| &event.body),
        Some(HostEventDraft::End { .. })
    ));
}

#[tokio::test]
async fn nodes_needing_permission_fail_closed_when_the_stored_ceiling_is_off() {
    let database = temporary_database();
    let host = harness(&database);
    let calls = Arc::new(AtomicUsize::new(0));
    {
        let calls = Arc::clone(&calls);
        host.registry
            .register(FnNode::new(
                "tool",
                PermissionRequirement::tool("mail.send"),
                true,
                move |_context, _input: Value| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(Value::Null) }
                },
            ))
            .expect("registers");
    }
    host.service
        .permissions()
        .grant("mail.send", "global", ToolTier::Off, "user")
        .expect("grants");
    let handle = start(&host, json!([{ "id": "a", "type": "tool" }]));
    host.service.wait(&handle.id).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let run = host.service.get(&handle.id).expect("a run");
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(
        run.outcome.and_then(|outcome| outcome.code),
        Some(AppErrorCode::PermissionDenied)
    );
}

#[tokio::test]
async fn cancelling_one_run_leaves_another_run_and_its_event_stream_independent() {
    let database = temporary_database();
    let host = harness(&database);
    let entered: Arc<Vec<Latch>> = Arc::new((0..2).map(|_| Latch::new()).collect());
    let gates: Arc<Vec<Latch>> = Arc::new((0..2).map(|_| Latch::new()).collect());
    let tokens: Arc<Mutex<Vec<Option<CancellationToken>>>> = Arc::new(Mutex::new(vec![None, None]));
    {
        let (entered, gates, tokens) = (
            Arc::clone(&entered),
            Arc::clone(&gates),
            Arc::clone(&tokens),
        );
        register(&host.registry, "wait", true, move |context, input| {
            let (entered, gates, tokens) = (
                Arc::clone(&entered),
                Arc::clone(&gates),
                Arc::clone(&tokens),
            );
            async move {
                let index = usize::try_from(input.as_u64().expect("an index")).expect("small");
                tokens.lock().expect("tokens")[index] = Some(context.cancel.clone());
                entered[index].open();
                gates[index].wait().await;
                Ok(Value::Null)
            }
        });
    }
    let first = start(&host, json!([{ "id": "a", "type": "wait", "input": 0 }]));
    let second = start(&host, json!([{ "id": "a", "type": "wait", "input": 1 }]));
    entered[0].wait().await;
    entered[1].wait().await;
    host.service.cancel(&first.id).await.expect("cancels");
    let cancelled: Vec<bool> = tokens
        .lock()
        .expect("tokens")
        .iter()
        .map(|token| token.as_ref().is_some_and(CancellationToken::is_cancelled))
        .collect();
    assert_eq!(cancelled, [true, false]);
    gates[1].open();
    host.service.wait(&second.id).await;
    assert_eq!(host.status(&second.id), RunStatus::Succeeded);
    for handle in [first, second] {
        let events: Vec<HostEvent> = host
            .sent()
            .into_iter()
            .filter(|event| event.stream_id == handle.stream_id)
            .collect();
        assert_eq!(
            events.iter().map(|event| event.seq).collect::<Vec<_>>(),
            (0..events.len() as u64).collect::<Vec<_>>()
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.body, HostEventDraft::End { .. }))
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn observers_see_every_event_synchronously_and_are_dropped_at_the_end() {
    let database = temporary_database();
    let host = harness(&database);
    let gate = Latch::new();
    {
        let gate = gate.clone();
        register(&host.registry, "talk", true, move |context, _input| {
            let gate = gate.clone();
            async move {
                gate.wait().await;
                context.emit(HostEventDraft::Token {
                    delta: "привет".to_owned(),
                    kind: Some(DeltaKind::Text),
                })?;
                Ok(Value::Null)
            }
        });
    }
    let handle = start(&host, json!([{ "id": "a", "type": "talk" }]));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let subscription = {
        let seen = Arc::clone(&seen);
        host.service
            .subscribe(
                &handle.id.to_string(),
                Arc::new(move |event: &HostEvent| seen.lock().expect("seen").push(event.seq)),
            )
            .expect("subscribes")
    };
    assert_eq!(host.service.observers(&handle.id.to_string()), 1);
    gate.open();
    host.service.wait(&handle.id).await;
    let sent = host.sent();
    assert_eq!(
        *seen.lock().expect("seen"),
        sent.iter()
            .skip(1)
            .map(|event| event.seq)
            .collect::<Vec<_>>()
    );
    assert_eq!(host.service.observers(&handle.id.to_string()), 0);
    subscription.unsubscribe();
    assert!(
        host.service
            .subscribe("missing", Arc::new(|_: &HostEvent| {}))
            .is_err()
    );
}

#[test]
fn typed_nodes_publish_json_schema_for_tool_exposure() {
    let registry = NodeRegistry::new();
    register_core_nodes(&registry).expect("registers");
    register_job_nodes(&registry).expect("registers");
    assert_eq!(
        registry
            .list()
            .iter()
            .map(|node| (node.node_type().to_owned(), node.side_effect()))
            .collect::<Vec<_>>(),
        [
            ("llm.generate".to_owned(), true),
            ("vector.search".to_owned(), false),
            ("vector.list".to_owned(), false),
            ("vector.index".to_owned(), true),
            ("knowledge.search".to_owned(), false),
            ("flow.branch".to_owned(), false),
            ("flow.map".to_owned(), false),
            ("job.run".to_owned(), true),
        ]
    );
    let search = registry.resolve("vector.search").expect("a node");
    let schema = search.input_schema();
    assert_eq!(schema["type"], json!("object"));
    assert_eq!(
        schema["properties"]["storeId"],
        json!({ "type": "string", "format": "uuid" })
    );
    assert_eq!(
        schema["properties"]["mode"]["enum"],
        json!(["hybrid", "vector"])
    );
    assert_eq!(schema["required"], json!(["storeId", "query"]));
    assert_eq!(search.output_schema()["type"], json!("array"));
    let branch = registry.resolve("flow.branch").expect("a node");
    assert_eq!(
        branch.input_schema()["required"],
        json!(["condition", "then", "else"])
    );
    let knowledge = registry.resolve("knowledge.search").expect("a node");
    assert_eq!(knowledge.input_schema()["required"], Value::Null);
    assert!(
        knowledge.input_schema()["properties"]
            .get("documentId")
            .is_some()
    );
    assert!(
        knowledge.input_schema()["properties"]
            .get("scope")
            .is_none()
    );
    let generate = registry.resolve("llm.generate").expect("a node");
    assert_eq!(
        generate.input_schema()["anyOf"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        generate.input_schema()["anyOf"][0]["properties"]["messages"]["items"]["properties"]["role"]
            ["enum"],
        json!(["system", "user", "assistant"])
    );
    let job = registry.resolve("job.run").expect("a node");
    assert_eq!(job.permission(), &PermissionRequirement::tool("job.run"));
    assert_eq!(job.input_schema()["required"], json!(["job"]));
    assert!(registry.for_tool("job.run").is_some());
    assert!(registry.for_tool("flow.map").is_none());
    assert_eq!(
        registry
            .register(MapNodeAgain)
            .expect_err("a duplicate")
            .message,
        "Duplicate node type: flow.map"
    );
    assert!(registry.unregister("flow.map"));
    assert!(!registry.unregister("flow.map"));
    assert_eq!(
        registry
            .resolve("flow.map")
            .err()
            .map(|error| error.message),
        Some("Неизвестный тип узла: flow.map".to_owned())
    );
}

struct MapNodeAgain;

#[async_trait]
impl NodeDef for MapNodeAgain {
    type Input = Value;
    type Output = Value;

    fn node_type(&self) -> &str {
        "flow.map"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(&self, _context: &StepContext, input: Value) -> Result<Value, AppError> {
        Ok(input)
    }
}

#[derive(Default)]
struct RecordingLink(Mutex<Vec<HostEventDraft>>);

impl StepLink for RecordingLink {
    fn emit(&self, event: HostEventDraft) -> Result<(), AppError> {
        self.0.lock().expect("drafts").push(event);
        Ok(())
    }

    fn request_approval(
        &self,
        _request: ApprovalRequestDto,
    ) -> futures::future::BoxFuture<'static, Result<ApprovalDecision, AppError>> {
        Box::pin(async { Ok(ApprovalDecision::Denied) })
    }
}

#[derive(Default)]
struct ScriptedText {
    requests: Mutex<Vec<GenerateRequest>>,
}

#[async_trait]
impl TextGenerationDriver for ScriptedText {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn generate(
        &self,
        _request: &GenerateRequest,
        _cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        unreachable!("the node streams")
    }

    fn stream(&self, request: &GenerateRequest, _cancel: &CancellationToken) -> DeltaStream {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        Box::pin(stream::iter([
            Ok(TextDelta::reasoning("Thinking")),
            Ok(TextDelta::ToolCall(crate::drivers::ai::ToolCall::new(
                "c", "t", "{}",
            ))),
            Ok(TextDelta::text("Hello")),
            Ok(TextDelta::text(" world")),
        ]))
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct Drivers {
    driver: Arc<ScriptedText>,
    asked: Mutex<Vec<String>>,
}

impl TextDrivers for Drivers {
    fn text(&self, provider_id: &str) -> Result<SharedTextDriver, AppError> {
        self.asked
            .lock()
            .expect("asked")
            .push(provider_id.to_owned());
        Ok(self.driver.clone())
    }
}

fn detached(
    services: KernelServices,
    link: Arc<RecordingLink>,
    cancel: CancellationToken,
) -> StepContext {
    StepContext::new(RunId::new(new_id()), cancel, services, link)
}

#[tokio::test]
async fn core_generation_streams_text_and_reasoning_through_the_provider_port() {
    let registry = NodeRegistry::new();
    register_core_nodes(&registry).expect("registers");
    let driver = Arc::new(ScriptedText::default());
    let drivers = Arc::new(Drivers {
        driver: Arc::clone(&driver),
        asked: Mutex::new(Vec::new()),
    });
    let link = Arc::new(RecordingLink::default());
    let context = detached(
        KernelServices {
            providers: Some(drivers.clone()),
            ..KernelServices::default()
        },
        Arc::clone(&link),
        CancellationToken::new(),
    );
    let generate = registry.resolve("llm.generate").expect("a node");
    let result = generate
        .execute(
            &context,
            json!({
                "providerId": "provider",
                "model": "model",
                "messages": [{ "role": "user", "content": "Hi" }]
            }),
        )
        .await
        .expect("generates");
    assert_eq!(*drivers.asked.lock().expect("asked"), ["provider"]);
    let requests = driver.requests.lock().expect("requests").clone();
    assert_eq!(
        requests,
        [GenerateRequest::new(
            "model",
            vec![crate::drivers::ai::ChatMessage::user("Hi")]
        )]
    );
    assert_eq!(
        result,
        json!({ "text": "Hello world", "reasoning": "Thinking" })
    );
    let token = |delta: &str, kind| HostEventDraft::Token {
        delta: delta.to_owned(),
        kind: Some(kind),
    };
    assert_eq!(
        *link.0.lock().expect("drafts"),
        [
            token("Thinking", DeltaKind::Reasoning),
            token("Hello", DeltaKind::Text),
            token(" world", DeltaKind::Text),
        ]
    );
    assert!(generate.side_effect());
    assert!(!generate.side_effect_free());

    let prepared = generate
        .execute(
            &context,
            json!({ "request": {
                "providerId": "provider",
                "model": "model",
                "messages": [],
                "temperature": 0.5,
                "topK": 3,
                "maxOutputTokens": 64,
                "system": "Be brief"
            } }),
        )
        .await
        .expect("generates from a prepared request");
    assert_eq!(prepared["text"], json!("Hello world"));
    let last = driver
        .requests
        .lock()
        .expect("requests")
        .last()
        .cloned()
        .expect("a request");
    assert_eq!(last.system.as_deref(), Some("Be brief"));
    assert_eq!(
        (last.temperature, last.top_k, last.max_output_tokens),
        (Some(0.5), Some(3), Some(64))
    );
    for bad in [
        json!({ "providerId": "", "model": "m", "messages": [] }),
        json!({ "providerId": "p", "model": "m", "messages": [], "temperature": 3 }),
        json!({ "providerId": "p", "model": "m", "messages": [{ "role": "tool", "content": "" }] }),
    ] {
        assert_eq!(
            generate
                .execute(&context, bad.clone())
                .await
                .expect_err("invalid")
                .code,
            AppErrorCode::ValidationFailed,
            "{bad}"
        );
    }
    let without = detached(
        KernelServices::default(),
        Arc::new(RecordingLink::default()),
        CancellationToken::new(),
    );
    assert_eq!(
        generate
            .execute(
                &without,
                json!({ "providerId": "p", "model": "m", "messages": [] })
            )
            .await
            .expect_err("no providers")
            .message,
        nodes::PROVIDERS_UNAVAILABLE
    );
}

#[derive(Default)]
struct RecordingSearch(Mutex<Vec<(String, String, VectorSearchOptions)>>);

#[async_trait]
impl VectorSearch for RecordingSearch {
    async fn search(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        _cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        self.0
            .lock()
            .expect("searches")
            .push((store_id.to_owned(), query.to_owned(), options));
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn core_vector_search_delegates_validated_options_and_observes_cancellation() {
    let registry = NodeRegistry::new();
    register_core_nodes(&registry).expect("registers");
    let search = Arc::new(RecordingSearch::default());
    let cancel = CancellationToken::new();
    let context = detached(
        KernelServices {
            vector_stores: Some(search.clone()),
            ..KernelServices::default()
        },
        Arc::new(RecordingLink::default()),
        cancel.clone(),
    );
    let node = registry.resolve("vector.search").expect("a node");
    let store = new_id().hyphenated().to_string();
    assert_eq!(
        node.execute(
            &context,
            json!({ "storeId": store, "query": "query", "k": 3, "minScore": 0.5 })
        )
        .await
        .expect("searches"),
        json!([])
    );
    {
        let searches = search.0.lock().expect("searches");
        assert_eq!(searches.len(), 1);
        let (store_id, query, options) = &searches[0];
        assert_eq!(
            (store_id.as_str(), query.as_str()),
            (store.as_str(), "query")
        );
        assert_eq!(
            (options.k, options.min_score, options.mode),
            (Some(3), Some(0.5), None)
        );
        assert!(options.rerank);
    }
    cancel.cancel();
    assert_eq!(
        node.execute(&context, json!({ "storeId": store, "query": "query" }))
            .await
            .expect_err("cancelled")
            .code,
        AppErrorCode::RunCancelled
    );
    assert_eq!(search.0.lock().expect("searches").len(), 1);
}

#[tokio::test]
async fn flow_map_projects_own_properties_in_order_and_fails_on_missing_fields() {
    let registry = NodeRegistry::new();
    register_core_nodes(&registry).expect("registers");
    let context = detached(
        KernelServices::default(),
        Arc::new(RecordingLink::default()),
        CancellationToken::new(),
    );
    let map = registry.resolve("flow.map").expect("a node");
    assert_eq!(
        map.execute(
            &context,
            json!({ "items": [{ "a": { "b": 1 } }, { "a": { "b": 2 } }], "path": ["a", "b"] })
        )
        .await
        .expect("maps"),
        json!([1, 2])
    );
    assert_eq!(
        map.execute(&context, json!({ "items": [[10, 20], "x"], "path": [] }))
            .await
            .expect("an identity map"),
        json!([[10, 20], "x"])
    );
    assert_eq!(
        map.execute(&context, json!({ "items": [[10, 20]], "path": ["1"] }))
            .await
            .expect("an index"),
        json!([20])
    );
    assert_eq!(
        map.execute(
            &context,
            json!({ "items": [{ "a": 1 }, { "b": 2 }], "path": ["a"] })
        )
        .await
        .expect_err("a missing field")
        .message,
        "Missing map field: a"
    );
    let branch = registry.resolve("flow.branch").expect("a node");
    assert_eq!(
        branch
            .execute(
                &context,
                json!({ "condition": false, "then": 1, "else": null })
            )
            .await
            .expect("branches"),
        Value::Null
    );
    assert!(
        branch
            .execute(&context, json!({ "condition": true, "then": 1 }))
            .await
            .is_err()
    );
}

const CRASH_DB: &str = "ZVS_KERNEL_CRASH_DB";
const CRASH_SAFETY: &str = "ZVS_KERNEL_CRASH_SAFETY";
const CHECKPOINT_MARK: &str = "ZVS_KERNEL_CHECKPOINT";

fn register_crash_work(registry: &NodeRegistry, side_effect_free: bool, calls: Arc<AtomicUsize>) {
    registry
        .register(FnNode::new(
            "crash.work",
            PermissionRequirement::None,
            !side_effect_free,
            move |context: StepContext, input: String| {
                calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if input == "checkpoint" {
                        return Ok("saved".to_owned());
                    }
                    if std::env::var(CRASH_DB).is_ok() {
                        println!("{CHECKPOINT_MARK} {}", context.run_id);
                        pending::<()>().await;
                    }
                    Ok("resumed".to_owned())
                }
            },
        ))
        .expect("registers");
}

#[tokio::test]
#[ignore = "the child process of a_killed_host_process_recovers_runs_according_to_the_selected_policy"]
async fn kernel_crash_child() {
    let Ok(file) = std::env::var(CRASH_DB) else {
        return;
    };
    let safe = std::env::var(CRASH_SAFETY).is_ok_and(|value| value == "safe");
    let database = Arc::new(Database::open(Path::new(&file), silent_logger()).expect("opens"));
    let registry = Arc::new(NodeRegistry::new());
    register_crash_work(&registry, safe, Arc::new(AtomicUsize::new(0)));
    let service = RunService::new(RunServiceOptions::new(
        database,
        EventBus::default(),
        registry,
    ))
    .expect("a run service");
    service
        .start(start_input(
            "job",
            json!([
                { "id": "a", "type": "crash.work", "input": "checkpoint" },
                { "id": "b", "type": "crash.work", "input": "wait", "dependencies": ["a"] }
            ]),
            4,
            Value::Null,
        ))
        .expect("starts");
    pending::<()>().await;
}

fn run_crash_child(file: &Path, safety: &str) -> RunId {
    let mut child = Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "kernel::tests::kernel_crash_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CRASH_DB, file)
        .env(CRASH_SAFETY, safety)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawns the child");
    let stdout = child.stdout.take().expect("a stdout pipe");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some((_, id)) = line.split_once(CHECKPOINT_MARK) {
                let _ = sender.send(id.trim().to_owned());
                return;
            }
        }
    });
    let id = receiver.recv_timeout(Duration::from_secs(15));
    let _ = child.kill();
    let _ = child.wait();
    RunId::parse(&id.expect("the child reached its checkpoint")).expect("a run id")
}

#[tokio::test]
async fn a_killed_host_process_recovers_runs_according_to_the_selected_policy() {
    for safety in ["safe", "unsafe"] {
        let database = temporary_database();
        let id = run_crash_child(&database.directory().join("studio.sqlite"), safety);
        let host = harness(&database);
        let calls = Arc::new(AtomicUsize::new(0));
        register_crash_work(&host.registry, safety == "safe", Arc::clone(&calls));
        assert_eq!(
            host.statuses(&id),
            [StepStatus::Succeeded, StepStatus::Running]
        );
        let prior = host.service.record(&id.to_string()).expect("events");
        host.service.recover().expect("recovers");
        host.service.wait(&id).await;
        assert_eq!(
            host.status(&id),
            if safety == "safe" {
                RunStatus::Succeeded
            } else {
                RunStatus::Interrupted
            }
        );
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(safety == "safe"));
        assert_eq!(host.steps(&id)[0].output, Some(json!("saved")));
        let sent = host.sent();
        assert_eq!(sent[0].seq, prior.last().expect("prior events").seq + 1);
        assert!(matches!(
            sent.last().map(|event| &event.body),
            Some(HostEventDraft::End { .. })
        ));
    }
}

#[test]
fn a_vector_search_tool_call_carries_its_hits_as_citations_for_display_only() {
    let store = "0199aa11-2222-7111-8111-111111111111";
    let hits: Vec<VectorSearchHitDto> = serde_json::from_value(json!([{
        "id": "doc-1:0",
        "documentId": "doc-1",
        "path": "docs/a.pdf",
        "chunkIndex": 0,
        "score": 0.9,
        "text": "passage",
        "section": "Intro",
        "page": 2,
        "payload": {},
    }]))
    .expect("hits");
    let node = super::nodes::VectorSearchNode;
    let meta = node
        .tool_meta(&json!({ "storeId": store, "query": "x" }), &hits)
        .expect("a meta");
    assert_eq!(
        meta,
        json!({ "citations": [{
            "storeId": store,
            "documentId": "doc-1",
            "sourcePath": "docs/a.pdf",
            "chunkIndex": 0,
            "score": 0.9,
            "section": "Intro",
            "page": 2,
            "related": null,
        }] })
    );
    assert!(node.tool_meta(&json!({ "query": "x" }), &hits).is_none());
}
