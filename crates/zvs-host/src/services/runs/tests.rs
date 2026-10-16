use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, HostEvent, HostEventDraft, RunHandleDto, RunId, RunKind, RunListFilter,
    RunStatus, StartRunInput, StepStatus, ToolTier,
};

use super::*;
use crate::data::Database;
use crate::data::schema::PendingApprovalRow;
use crate::kernel::{FnNode, NodeRegistry, PermissionRequirement, register_core_nodes};
use crate::platform::clock::{Clock, FakeClock};
use crate::services::permissions::{Admission, PermissionContext};
use crate::services::retention::{RetentionService, RetentionServiceOptions};
use crate::services::settings::SettingService;
use crate::testing::{
    CLOCK_ORIGIN, MemoryLogger, MemorySink, TemporaryDatabase, TokioClock, temporary_database,
};

struct Host {
    service: RunService,
    registry: Arc<NodeRegistry>,
    sink: Arc<MemorySink>,
    executed: Arc<AtomicUsize>,
    database: Arc<Database>,
}

impl Host {
    fn status(&self, id: &RunId) -> RunStatus {
        self.service.get(id).expect("a run").status
    }

    fn outcome_code(&self, id: &RunId) -> Option<AppErrorCode> {
        self.service
            .get(id)
            .expect("a run")
            .outcome
            .and_then(|outcome| outcome.code)
    }

    fn executed(&self) -> usize {
        self.executed.load(Ordering::SeqCst)
    }

    fn approval_row(&self, id: &str) -> Option<PendingApprovalRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).permissions.approval(id)?)
            })
            .expect("reads")
    }

    fn grants(&self) -> Vec<crate::data::schema::ToolPermissionRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .permissions
                    .list("mail.send")?)
            })
            .expect("reads")
    }

    fn uses(&self, grant: &str) -> Vec<crate::data::schema::PermissionUseRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).permissions.uses(grant)?)
            })
            .expect("reads")
    }

    fn admit(&self, node_type: &str, scopes: &[&str]) -> Admission {
        self.service
            .permissions()
            .admit(node_type, &PermissionContext::new("r", scopes))
            .expect("admits")
    }

    fn grant(&self, scope: &str, tier: ToolTier) -> String {
        self.service
            .permissions()
            .grant("mail.send", scope, tier, "user")
            .expect("grants")
    }

    async fn approval(&self) -> String {
        for _ in 0..5_000 {
            let request = self
                .sink
                .events()
                .into_iter()
                .find_map(|event| match event.body {
                    HostEventDraft::Approval { request } => {
                        request.get("id").and_then(Value::as_str).map(str::to_owned)
                    }
                    _ => None,
                });
            if let Some(id) = request {
                return id;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("no approval was requested");
    }

    fn start_tool(&self, scopes: &[&str]) -> RunHandleDto {
        self.service
            .start(
                zvs_contract::parse(json!({
                    "kind": "job",
                    "graph": {
                        "permissionScopes": scopes,
                        "nodes": [{ "id": "a", "type": "tool", "retry": { "maxAttempts": 3, "backoffMs": 0 } }]
                    }
                }))
                .expect("a start input"),
            )
            .expect("starts")
    }
}

fn register_tool(
    registry: &NodeRegistry,
    node_type: &str,
    tier: Option<ToolTier>,
    executed: &Arc<AtomicUsize>,
) {
    let executed = Arc::clone(executed);
    registry
        .register(FnNode::new(
            node_type,
            PermissionRequirement::Tool {
                tool: "mail.send".to_owned(),
                tier,
            },
            true,
            move |_context, _input: Value| {
                executed.fetch_add(1, Ordering::SeqCst);
                async { Ok("executed".to_owned()) }
            },
        ))
        .expect("registers");
}

fn host_on(
    database: Arc<Database>,
    registry: Arc<NodeRegistry>,
    executed: Arc<AtomicUsize>,
) -> Host {
    let sink = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(sink.clone());
    let service = RunService::new(RunServiceOptions {
        approval_timeout: Duration::from_millis(1_000),
        clock: Arc::new(TokioClock::new(CLOCK_ORIGIN)),
        ..RunServiceOptions::new(Arc::clone(&database), bus, Arc::clone(&registry))
    })
    .expect("a run service");
    Host {
        service,
        registry,
        sink,
        executed,
        database,
    }
}

fn host(database: &TemporaryDatabase) -> Host {
    let registry = Arc::new(NodeRegistry::new());
    let executed = Arc::new(AtomicUsize::new(0));
    register_tool(&registry, "tool", None, &executed);
    host_on(database.shared(), registry, executed)
}

#[tokio::test]
async fn a_ceiling_composed_with_a_scope_takes_the_strictest_tier() {
    let tiers = [ToolTier::Auto, ToolTier::Ask, ToolTier::Off];
    let expected = [Admission::Allow, Admission::Ask, Admission::Deny];
    for (i, global) in tiers.iter().enumerate() {
        for (j, scope) in tiers.iter().enumerate() {
            let database = temporary_database();
            let host = host(&database);
            host.grant("global", *global);
            host.grant("skill:test", *scope);
            assert_eq!(
                host.service
                    .permissions()
                    .admit("tool", &PermissionContext::new("test", &["skill:test"]))
                    .expect("admits"),
                expected[i.max(j)],
                "{global} ceiling with {scope} narrowing"
            );
        }
    }
}

#[tokio::test]
async fn admission_combines_all_applicable_scopes_and_ignores_unrelated_grants() {
    let database = temporary_database();
    let host = host(&database);
    host.grant("global", ToolTier::Auto);
    host.grant("skill:a", ToolTier::Ask);
    host.grant("site:example.com", ToolTier::Off);
    assert_eq!(
        host.admit("tool", &["skill:a", "site:example.com"]),
        Admission::Deny
    );
    assert_eq!(host.admit("tool", &[]), Admission::Allow);
}

#[tokio::test]
async fn a_missing_global_policy_means_ask_and_a_scoped_auto_never_widens_it() {
    let database = temporary_database();
    let host = host(&database);
    assert_eq!(host.admit("tool", &[]), Admission::Ask);
    host.grant("skill:a", ToolTier::Auto);
    assert_eq!(host.admit("tool", &["skill:a"]), Admission::Ask);
    let refused = host
        .service
        .permissions()
        .grant("mail.send", "tool:x", ToolTier::Auto, "user")
        .expect_err("an unknown scope");
    assert_eq!(refused.code, AppErrorCode::ValidationFailed);
}

#[tokio::test]
async fn a_node_requirement_can_narrow_a_global_auto_grant_at_step_admission() {
    let database = temporary_database();
    let host = host(&database);
    register_tool(
        &host.registry,
        "careful",
        Some(ToolTier::Ask),
        &host.executed,
    );
    host.grant("global", ToolTier::Auto);
    assert_eq!(host.admit("careful", &[]), Admission::Ask);
    let handle = host
        .service
        .start(
            zvs_contract::parse(json!({
                "kind": "job",
                "graph": { "nodes": [{ "id": "a", "type": "careful" }] }
            }))
            .expect("a start input"),
        )
        .expect("starts");
    let id = host.approval().await;
    assert_eq!(host.executed(), 0);
    host.service.deny(&handle.id, &id).expect("denies");
    host.service.wait(&handle.id).await;
    assert_eq!(
        host.outcome_code(&handle.id),
        Some(AppErrorCode::ApprovalDenied)
    );
}

#[tokio::test]
async fn unknown_nodes_reject_the_entire_graph_before_any_run_or_step_is_created() {
    let database = temporary_database();
    let host = host(&database);
    let refused = host.service.start(
        zvs_contract::parse(json!({
            "kind": "job",
            "graph": { "nodes": [
                { "id": "a", "type": "tool" },
                { "id": "b", "type": "missing", "dependencies": ["a"] }
            ] }
        }))
        .expect("a start input"),
    );
    assert!(refused.is_err());
    assert!(
        host.service
            .list(&RunListFilter::default())
            .expect("lists")
            .items
            .is_empty()
    );
    assert_eq!(host.executed(), 0);
}

#[tokio::test]
async fn an_approval_is_persisted_before_its_event_blocks_execution_then_resumes() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let persisted_first = Arc::new(Mutex::new(Vec::new()));
    {
        let persisted_first = Arc::clone(&persisted_first);
        let database = Arc::clone(&host.database);
        host.service
            .subscribe(
                &handle.id.to_string(),
                Arc::new(move |event: &HostEvent| {
                    if let HostEventDraft::Approval { request } = &event.body {
                        let id = request["id"].as_str().unwrap_or_default().to_owned();
                        let stored = database
                            .with_connection(|connection| {
                                Ok(Repositories::new(connection).permissions.approval(&id)?)
                            })
                            .expect("reads");
                        persisted_first.lock().expect("seen").push(stored.is_some());
                    }
                }),
            )
            .expect("subscribes");
    }
    let id = host.approval().await;
    assert_eq!(*persisted_first.lock().expect("seen"), [true]);
    assert_eq!(host.status(&handle.id), RunStatus::Blocked);
    assert_eq!(host.approval_row(&id).expect("a row").decision, None);
    assert_eq!(host.executed(), 0);
    host.service
        .approve(&handle.id, &id, false)
        .expect("approves");
    host.service.wait(&handle.id).await;
    assert_eq!(host.status(&handle.id), RunStatus::Succeeded);
    assert_eq!(host.executed(), 1);
    assert_eq!(
        host.service
            .approve(&handle.id, &id, false)
            .expect_err("decided")
            .code,
        AppErrorCode::Conflict
    );
}

#[tokio::test]
async fn a_denial_fails_with_approval_denied_without_retries() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let id = host.approval().await;
    host.service.deny(&handle.id, &id).expect("denies");
    host.service.wait(&handle.id).await;
    assert_eq!(
        host.outcome_code(&handle.id),
        Some(AppErrorCode::ApprovalDenied)
    );
    assert_eq!(host.service.steps(&handle.id).expect("steps").len(), 1);
    assert_eq!(host.executed(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_timeout_denies_and_never_leaves_a_blocked_run() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let id = host.approval().await;
    host.service.wait(&handle.id).await;
    assert_eq!(
        host.outcome_code(&handle.id),
        Some(AppErrorCode::ApprovalDenied)
    );
    assert_eq!(
        host.approval_row(&id).and_then(|row| row.decision),
        Some(crate::kernel::ApprovalDecision::Denied)
    );
    assert_eq!(host.executed(), 0);
}

#[tokio::test]
async fn always_allow_grants_are_persisted_logged_honoured_and_revocable() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let id = host.approval().await;
    host.service
        .approve(&handle.id, &id, true)
        .expect("approves");
    host.service.wait(&handle.id).await;
    let grant = host.grants().remove(0);
    assert_eq!(grant.scope, "global");
    let next = host.start_tool(&[]);
    host.service.wait(&next.id).await;
    assert_eq!(host.status(&next.id), RunStatus::Succeeded);
    assert!(
        host.uses(&grant.id)
            .iter()
            .any(|used| used.run_id == next.id.to_string())
    );
    host.service
        .permissions()
        .revoke(&grant.id)
        .expect("revokes");
    let used = &host.uses(&grant.id)[0];
    assert_eq!(
        (
            used.subject.as_str(),
            used.scope.as_str(),
            used.tier,
            used.granted_by.as_str()
        ),
        ("mail.send", "global", ToolTier::Auto, "user")
    );
    assert_eq!(host.admit("tool", &[]), Admission::Ask);
}

#[tokio::test]
async fn a_scoped_always_allow_cannot_widen_an_ask_ceiling() {
    let database = temporary_database();
    let host = host(&database);
    host.grant("global", ToolTier::Ask);
    let handle = host.start_tool(&["skill:a"]);
    let id = host.approval().await;
    host.service
        .approve(&handle.id, &id, true)
        .expect("approves");
    host.service.wait(&handle.id).await;
    assert_eq!(host.admit("tool", &["skill:a"]), Admission::Ask);
}

#[tokio::test]
async fn a_revocation_while_pending_cannot_be_overridden_by_always_allow() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let id = host.approval().await;
    host.grant("global", ToolTier::Off);
    assert_eq!(
        host.service
            .approve(&handle.id, &id, true)
            .expect_err("revoked")
            .code,
        AppErrorCode::PermissionDenied
    );
    host.service.deny(&handle.id, &id).expect("denies");
    host.service.wait(&handle.id).await;
    assert_eq!(host.executed(), 0);
    assert_eq!(host.grants()[0].tier, ToolTier::Off);
}

#[tokio::test]
async fn cancellation_resolves_a_pending_approval_and_rejects_late_decisions() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let id = host.approval().await;
    host.service.cancel(&handle.id).await.expect("cancels");
    assert_eq!(
        host.approval_row(&id).and_then(|row| row.decision),
        Some(crate::kernel::ApprovalDecision::Denied)
    );
    assert!(host.service.approve(&handle.id, &id, false).is_err());
    assert_eq!(host.executed(), 0);
}

#[tokio::test]
async fn an_approval_needs_the_matching_run_id() {
    let database = temporary_database();
    let host = host(&database);
    let handle = host.start_tool(&[]);
    let other = host.start_tool(&[]);
    let id = host.approval().await;
    let owner = host.approval_row(&id).expect("a row").run_id;
    let stranger = if owner == handle.id.to_string() {
        other.id
    } else {
        handle.id
    };
    assert_eq!(
        host.service
            .approve(&stranger, &id, false)
            .expect_err("not this run")
            .code,
        AppErrorCode::Conflict
    );
    host.service.dispose().await;
}

#[tokio::test]
async fn core_branch_and_map_compose_through_ordinary_graph_bindings() {
    let database = temporary_database();
    let host = host(&database);
    register_core_nodes(&host.registry).expect("registers");
    let handle = host
        .service
        .start(
            zvs_contract::parse(json!({
                "kind": "job",
                "graph": { "nodes": [
                    {
                        "id": "branch",
                        "type": "flow.branch",
                        "input": { "condition": true, "then": [{ "name": "A" }, { "name": "B" }], "else": [] }
                    },
                    {
                        "id": "map",
                        "type": "flow.map",
                        "dependencies": ["branch"],
                        "input": { "path": ["name"] },
                        "bindings": { "items": { "source": "node", "nodeId": "branch" } }
                    }
                ] }
            }))
            .expect("a start input"),
        )
        .expect("starts");
    host.service.wait(&handle.id).await;
    assert_eq!(
        host.service
            .steps(&handle.id)
            .expect("steps")
            .last()
            .and_then(|step| step.output.clone()),
        Some(json!(["A", "B"]))
    );
}

#[tokio::test]
async fn a_restart_fails_an_interrupted_approval_without_repeating_the_protected_operation() {
    let database = temporary_database();
    let first = host(&database);
    let handle = first.start_tool(&[]);
    first.approval().await;
    first.service.dispose().await;
    let next = host_on(
        database.shared(),
        Arc::clone(&first.registry),
        Arc::clone(&first.executed),
    );
    next.service.recover().expect("recovers");
    next.service.wait(&handle.id).await;
    assert_eq!(
        next.outcome_code(&handle.id),
        Some(AppErrorCode::ApprovalDenied)
    );
    assert_eq!(next.executed(), 0);
}

#[tokio::test]
async fn chat_runs_admit_their_own_generation_and_retrieval_without_asking() {
    let database = temporary_database();
    let host = host(&database);
    let calls = Arc::new(AtomicUsize::new(0));
    for tool in ["llm.generate", "vector.search"] {
        let calls = Arc::clone(&calls);
        host.registry
            .register(FnNode::new(
                tool,
                PermissionRequirement::tool(tool),
                false,
                move |_context, _input: Value| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async { Ok(Value::Null) }
                },
            ))
            .expect("registers");
    }
    let handle = host
        .service
        .start(
            zvs_contract::parse(json!({
                "kind": "chat",
                "graph": { "nodes": [
                    { "id": "a", "type": "vector.search" },
                    { "id": "b", "type": "llm.generate", "dependencies": ["a"] }
                ] }
            }))
            .expect("a start input"),
        )
        .expect("starts");
    host.service.wait(&handle.id).await;
    assert_eq!(host.status(&handle.id), RunStatus::Succeeded);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let job = host
        .service
        .start(
            zvs_contract::parse(json!({
                "kind": "scenario",
                "subjectId": "s1",
                "graph": { "nodes": [{ "id": "a", "type": "vector.search" }] }
            }))
            .expect("a start input"),
        )
        .expect("starts");
    let id = host.approval().await;
    assert_eq!(host.approval_row(&id).expect("a row").scope, "scenario:s1");
    host.service.cancel(&job.id).await.expect("cancels");
}

struct History {
    service: RunService,
    clock: Arc<FakeClock>,
    database: TemporaryDatabase,
}

fn history() -> History {
    let database = temporary_database();
    let clock = Arc::new(FakeClock::new(1_800_000_000_000));
    let registry = Arc::new(NodeRegistry::new());
    registry
        .register(FnNode::new(
            "work",
            PermissionRequirement::None,
            false,
            |_context, _input: Value| async { Ok(json!({ "ok": true })) },
        ))
        .expect("registers");
    registry
        .register(FnNode::new(
            "boom",
            PermissionRequirement::None,
            false,
            |_context, _input: Value| async {
                Err::<Value, _>(AppError::new(AppErrorCode::Unknown, "channel_not_found"))
            },
        ))
        .expect("registers");
    let service = RunService::new(RunServiceOptions {
        clock: clock.clone(),
        ..RunServiceOptions::new(database.shared(), EventBus::default(), registry)
    })
    .expect("a run service");
    History {
        service,
        clock,
        database,
    }
}

impl History {
    async fn run(&self, kind: &str, title: &str, nodes: Value) -> RunId {
        let handle = self.start(kind, title, nodes);
        self.service.wait(&handle).await;
        handle
    }

    fn start(&self, kind: &str, title: &str, nodes: Value) -> RunId {
        self.service
            .start(
                zvs_contract::parse::<StartRunInput>(json!({
                    "kind": kind,
                    "title": title,
                    "graph": { "nodes": nodes }
                }))
                .expect("a start input"),
            )
            .expect("starts")
            .id
    }

    fn list(&self, patch: Value) -> zvs_contract::RunPageDto {
        let mut filter = json!({ "limit": 50 });
        if let (Some(base), Value::Object(extra)) = (filter.as_object_mut(), patch) {
            base.extend(extra);
        }
        self.service
            .list(&zvs_contract::parse(filter).expect("a filter"))
            .expect("lists")
    }

    fn titles(&self, patch: Value) -> Vec<String> {
        self.list(patch)
            .items
            .into_iter()
            .map(|run| run.title)
            .collect()
    }
}

fn work() -> Value {
    json!([{ "id": "a", "type": "work" }])
}

fn boom() -> Value {
    json!([{ "id": "notify", "type": "boom" }])
}

#[tokio::test]
async fn the_list_is_one_query_over_one_table_kinds_statuses_search_and_paging() {
    let history = history();
    history.run("scenario", "Slack notify", boom()).await;
    history.clock.advance(1_000);
    history.run("job", "Re-index knowledge base", work()).await;

    let all = history.list(json!({}));
    assert_eq!(
        all.items
            .iter()
            .map(|run| run.title.as_str())
            .collect::<Vec<_>>(),
        ["Re-index knowledge base", "Slack notify"]
    );
    assert_eq!(all.counts.total, 2);
    assert_eq!(all.counts.by_kind[&RunKind::Scenario], 1);
    assert_eq!(all.counts.by_status[&RunStatus::Failed], 1);
    assert_eq!(all.counts.by_kind.len(), RunKind::ALL.len());

    assert_eq!(history.list(json!({ "kinds": ["job"] })).items.len(), 1);
    assert_eq!(
        history.titles(json!({ "statuses": ["failed"] })),
        ["Slack notify"]
    );
    assert_eq!(history.list(json!({ "query": "index" })).items.len(), 1);
    assert_eq!(
        history.titles(json!({ "query": "notify" })),
        ["Slack notify"]
    );
    assert!(
        history
            .list(json!({ "from": history.clock.now() + 1 }))
            .items
            .is_empty()
    );

    let page = history.list(json!({ "limit": 1 }));
    assert_eq!(page.items.len(), 1);
    let cursor = page.next_cursor.expect("a cursor");
    let next = history.list(json!({ "limit": 1, "cursor": cursor }));
    assert_eq!(
        next.items
            .iter()
            .map(|run| run.title.as_str())
            .collect::<Vec<_>>(),
        ["Slack notify"]
    );
    assert_eq!(next.next_cursor, None);
}

#[tokio::test]
async fn the_live_view_keeps_recent_failures_and_drops_settled_runs() {
    let history = history();
    history.run("scenario", "Slack notify", boom()).await;
    history.run("job", "Re-index", work()).await;
    assert_eq!(history.titles(json!({ "live": true })), ["Slack notify"]);
    history.clock.advance(25 * 60 * 60 * 1000);
    assert!(history.titles(json!({ "live": true })).is_empty());
}

#[tokio::test]
async fn a_summary_reports_progress_the_active_node_and_a_pending_approval() {
    let history = history();
    let id = history
        .run(
            "job",
            "Two steps",
            json!([
                { "id": "one", "type": "work" },
                { "id": "two", "type": "work", "dependencies": ["one"] }
            ]),
        )
        .await;
    let summary = history.list(json!({})).items.remove(0);
    assert_eq!((summary.progress.done, summary.progress.total), (2, 2));
    assert_eq!(summary.active_node_id, None);
    history
        .database
        .with_connection(|connection| {
            Repositories::new(connection)
                .permissions
                .create_approval(&PendingApprovalRow {
                    id: crate::platform::ids::new_id().hyphenated().to_string(),
                    run_id: id.to_string(),
                    subject: "mail.send".to_owned(),
                    scope: "global".to_owned(),
                    expires_at: history.clock.now() + 60_000,
                    decision: None,
                })?;
            Ok(())
        })
        .expect("an approval");
    assert_eq!(
        history.list(json!({})).items[0]
            .approval
            .as_ref()
            .map(|approval| approval.subject.as_str()),
        Some("mail.send")
    );
}

#[tokio::test]
async fn retry_starts_a_new_run_linked_to_the_original_and_refuses_chat_and_live_runs() {
    let history = history();
    let failed = history.run("scenario", "Slack notify", boom()).await;
    let retried = history.service.retry(&failed).expect("retries").id;
    history.service.wait(&retried).await;
    let run = history.service.get(&retried).expect("a run");
    assert_eq!(run.retry_of_id, Some(failed));
    assert_eq!(run.title.as_deref(), Some("Slack notify"));
    assert_eq!(
        run.graph,
        history.service.get(&failed).expect("a run").graph
    );
    assert_eq!(history.list(json!({})).items[0].retry_of_id, Some(failed));

    let chat = history.run("chat", "Ход чата", work()).await;
    assert!(
        history
            .service
            .retry(&chat)
            .expect_err("refused")
            .message
            .contains("страницы чата")
    );

    let live = history.start("job", "Ещё выполняется", work());
    history
        .database
        .with_connection(|connection| {
            Repositories::new(connection).runs.update(
                &live.to_string(),
                &crate::data::repositories::RunPatch {
                    status: Some(RunStatus::Running),
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .expect("updates");
    assert!(
        history
            .service
            .retry(&live)
            .expect_err("refused")
            .message
            .contains("не завершён")
    );
    history.service.wait(&live).await;
}

#[tokio::test]
async fn retention_keeps_the_run_summary_and_drops_step_payloads_past_the_window() {
    let history = history();
    let id = history.run("job", "Старое задание", work()).await;
    let settings = SettingService::new(history.database.shared(), history.clock.clone());
    let logger = MemoryLogger::shared();
    let retention = RetentionService::new(RetentionServiceOptions {
        settings: Some(settings.clone()),
        clock: history.clock.clone(),
        logger: logger.clone(),
        ..RetentionServiceOptions::new(history.database.shared())
    });
    assert_eq!(retention.days(), 30);
    assert_eq!(retention.sweep(), 0);
    assert_eq!(
        history.service.steps(&id).expect("steps")[0].output,
        Some(json!({ "ok": true }))
    );

    history.clock.advance(31 * 24 * 60 * 60 * 1000);
    assert_eq!(retention.sweep(), 1);
    assert_eq!(retention.sweep(), 0);
    assert_eq!(
        logger.matching("Pruned step payloads past retention").len(),
        1
    );

    let summary = history.list(json!({})).items.remove(0);
    assert_eq!(summary.title, "Старое задание");
    assert_eq!(summary.pruned_at, Some(history.clock.now()));
    let detail = history.service.detail(&id).expect("a detail");
    assert_eq!(detail.steps.len(), 1);
    assert_eq!(detail.steps[0].status, StepStatus::Succeeded);
    assert_eq!(detail.steps[0].output, None);
    assert!(detail.logs.is_empty());

    settings
        .set("runs.retention.days", &json!(7))
        .expect("stores");
    assert_eq!(retention.days(), 7);
    settings
        .set("runs.retention.days", &json!(-1))
        .expect("stores");
    assert_eq!(retention.days(), 30);
}

#[tokio::test(start_paused = true)]
async fn retention_sweeps_on_start_and_then_on_its_interval_until_stopped() {
    let history = history();
    history.run("job", "Старое задание", work()).await;
    history.clock.advance(31 * 24 * 60 * 60 * 1000);
    let retention = RetentionService::new(RetentionServiceOptions {
        clock: history.clock.clone(),
        interval: Duration::from_secs(60),
        ..RetentionServiceOptions::new(history.database.shared())
    });
    retention.start();
    retention.start();
    assert_eq!(
        history.list(json!({})).items[0].pruned_at,
        Some(history.clock.now())
    );
    history.run("job", "Второе", work()).await;
    history.clock.advance(31 * 24 * 60 * 60 * 1000);
    tokio::time::sleep(Duration::from_secs(61)).await;
    assert!(
        history
            .list(json!({}))
            .items
            .iter()
            .all(|run| run.pruned_at.is_some())
    );
    retention.stop();
    history.run("job", "Третье", work()).await;
    history.clock.advance(31 * 24 * 60 * 60 * 1000);
    tokio::time::sleep(Duration::from_secs(121)).await;
    assert_eq!(history.list(json!({})).items[0].pruned_at, None);
}

#[tokio::test]
async fn clearing_finished_runs_spares_the_ones_a_conversation_still_references() {
    let history = history();
    let kept = history.run("chat", "Ход чата", work()).await;
    history.run("job", "Задание", work()).await;
    history
        .database
        .with_connection(|connection| {
            connection.execute(
                "INSERT INTO conversation (id, title, provider_id, model_id, settings, system_prompt, \
                 attached_store_ids, created_at, updated_at) \
                 VALUES ('c1', 'Диалог', 'p', 'model', '{}', '', '[]', 1, 1)",
                [],
            )?;
            connection.execute(
                "INSERT INTO message (id, conversation_id, role, content, citations, tokens_in, \
                 tokens_out, duration_ms, run_id, partial, created_at) \
                 VALUES ('m1', 'c1', 'user', 'Привет', '[]', 1, 0, 0, ?1, 0, 1)",
                [kept.to_string()],
            )?;
            Ok(())
        })
        .expect("a conversation");
    assert_eq!(history.service.clear_finished().expect("clears").removed, 1);
    assert_eq!(
        history
            .list(json!({}))
            .items
            .iter()
            .map(|run| run.id)
            .collect::<Vec<_>>(),
        [kept]
    );
}
