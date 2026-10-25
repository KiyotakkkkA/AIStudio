use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, HostEventDraft, RunHandleDto, RunId, RunStatus, ToolDefaultTier,
    ToolDefaultsDto, ToolDto, ToolListFilter, ToolReferenceDto, ToolReferenceKind, ToolSource,
    ToolTier, VectorSearchHitDto,
};

use super::*;
use crate::data::Repositories;
use crate::kernel::{
    FnNode, KernelServices, NodeRegistry, PermissionRequirement, VectorSearch, register_core_nodes,
};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::services::permissions::{Admission, PermissionContext};
use crate::services::vector_stores::VectorSearchOptions;
use crate::services::{RunService, RunServiceOptions, SettingService};
use crate::testing::{CLOCK_ORIGIN, MemorySink, TemporaryDatabase, temporary_database};

mod catalogue;

struct Host {
    database: TemporaryDatabase,
    nodes: Arc<NodeRegistry>,
    tools: ToolRegistry,
    runs: RunService,
    sink: Arc<MemorySink>,
    clock: Arc<FakeClock>,
    executed: Arc<AtomicUsize>,
}

fn run_service(host: &Host, services: KernelServices) -> RunService {
    let bus = EventBus::default();
    bus.attach(host.sink.clone());
    RunService::new(RunServiceOptions {
        approval_timeout: Duration::from_millis(1_000),
        services,
        tools: Some(host.tools.clone()),
        ..RunServiceOptions::new(host.database.shared(), bus, Arc::clone(&host.nodes))
    })
    .expect("a run service")
}

fn host() -> Host {
    let database = temporary_database();
    let nodes = Arc::new(NodeRegistry::new());
    let executed = Arc::new(AtomicUsize::new(0));
    {
        let executed = Arc::clone(&executed);
        nodes
            .register(FnNode::new(
                "tool",
                PermissionRequirement::tool("mail.send"),
                true,
                move |_context, _input: Value| {
                    executed.fetch_add(1, Ordering::SeqCst);
                    async { Ok::<_, AppError>("executed".to_owned()) }
                },
            ))
            .expect("registers");
    }
    let clock = Arc::new(FakeClock::new(CLOCK_ORIGIN));
    let tools = ToolRegistry::new(ToolRegistryOptions {
        clock: clock.clone(),
        ..ToolRegistryOptions::new(database.shared())
    });
    let sink = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(sink.clone());
    let runs = RunService::new(RunServiceOptions {
        approval_timeout: Duration::from_millis(1_000),
        tools: Some(tools.clone()),
        ..RunServiceOptions::new(database.shared(), bus, Arc::clone(&nodes))
    })
    .expect("a run service");
    Host {
        database,
        nodes,
        tools,
        runs,
        sink,
        clock,
        executed,
    }
}

fn descriptor() -> ToolDescriptor {
    ToolDescriptor {
        display_name: Some("mail.send".to_owned()),
        description: "Отправляет письмо".to_owned(),
        input_schema: json!({ "type": "object" }),
        ..ToolDescriptor::new("mail.send", ToolSource::Builtin)
    }
}

fn read_only() -> ToolDescriptor {
    ToolDescriptor {
        read_only: true,
        ..descriptor()
    }
}

impl Host {
    fn register(&self, descriptor: &ToolDescriptor) -> ToolDto {
        self.tools.register(descriptor).expect("registers")
    }

    fn start(&self, scopes: &[&str]) -> RunHandleDto {
        self.start_kind("job", None, scopes, "tool", json!(null))
    }

    fn start_kind(
        &self,
        kind: &str,
        subject: Option<&str>,
        scopes: &[&str],
        node_type: &str,
        input: Value,
    ) -> RunHandleDto {
        start_on(&self.runs, kind, subject, scopes, node_type, input)
    }

    fn admit(&self, scopes: &[&str]) -> Admission {
        self.runs
            .permissions()
            .admit("tool", &PermissionContext::new("r", scopes))
            .expect("admits")
    }

    fn grant(&self, scope: &str, tier: ToolTier) {
        self.runs
            .permissions()
            .grant("mail.send", scope, tier, "user")
            .expect("grants");
    }

    fn outcome(&self, id: &RunId) -> Option<AppErrorCode> {
        self.runs
            .get(id)
            .expect("a run")
            .outcome
            .and_then(|outcome| outcome.code)
    }

    fn executed(&self) -> usize {
        self.executed.load(Ordering::SeqCst)
    }

    fn grants(&self) -> usize {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .permissions
                    .list("mail.send")?
                    .len())
            })
            .expect("reads")
    }

    async fn approval(&self) -> String {
        for _ in 0..5_000 {
            let found = self
                .sink
                .events()
                .into_iter()
                .find_map(|event| match event.body {
                    HostEventDraft::Approval { request } => {
                        request.get("id").and_then(Value::as_str).map(str::to_owned)
                    }
                    _ => None,
                });
            if let Some(id) = found {
                return id;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("no approval was requested");
    }

    fn keys(&self, filter: &ToolListFilter) -> Vec<String> {
        self.tools
            .list(filter)
            .expect("lists")
            .into_iter()
            .map(|tool| tool.key)
            .collect()
    }
}

fn start_on(
    runs: &RunService,
    kind: &str,
    subject: Option<&str>,
    scopes: &[&str],
    node_type: &str,
    input: Value,
) -> RunHandleDto {
    let mut payload = json!({
        "kind": kind,
        "graph": { "permissionScopes": scopes, "nodes": [{ "id": "a", "type": node_type }] },
    });
    if !input.is_null() {
        payload["graph"]["nodes"][0]["input"] = input;
    }
    if let Some(subject) = subject {
        payload["subjectId"] = json!(subject);
    }
    runs.start(zvs_contract::parse(payload).expect("a start input"))
        .expect("starts")
}

fn available_only() -> ToolListFilter {
    ToolListFilter {
        include_unavailable: Some(false),
        ..ToolListFilter::default()
    }
}

#[derive(Default)]
struct RecordingSearch(Mutex<usize>);

#[async_trait]
impl VectorSearch for RecordingSearch {
    async fn search(
        &self,
        _store_id: &str,
        _query: &str,
        _options: VectorSearchOptions,
        _cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        *self.0.lock().expect("searches") += 1;
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn each_kind_of_tool_defaults_to_its_tier() {
    for (read_only, destructive, expected) in [
        (true, false, ToolTier::Auto),
        (false, false, ToolTier::Ask),
        (false, true, ToolTier::Off),
        (true, true, ToolTier::Off),
    ] {
        let host = host();
        assert_eq!(
            default_tool_tier(read_only, destructive, ToolDefaultTier::Inferred),
            expected
        );
        let registered = host.register(&ToolDescriptor {
            read_only,
            destructive,
            ..descriptor()
        });
        assert_eq!(registered.tier, expected, "{read_only} {destructive}");
    }
}

#[tokio::test]
async fn registration_is_idempotent_by_key_and_keeps_the_decided_tier_and_usage() {
    let host = host();
    let first = host.register(&descriptor());
    host.tools
        .record_use("mail.send", CLOCK_ORIGIN)
        .expect("records");
    host.tools
        .set_tier(&first.id.to_string(), ToolTier::Auto, "user")
        .expect("sets");
    host.clock.advance(1000);
    let second = host.register(&ToolDescriptor {
        source: ToolSource::Mcp,
        source_ref: Some("mail-server".to_owned()),
        description: "Обновлённое описание".to_owned(),
        ..descriptor()
    });
    assert_eq!(second.id, first.id);
    assert_eq!(
        host.tools
            .list(&ToolListFilter::default())
            .expect("lists")
            .len(),
        1
    );
    assert_eq!(second.source, ToolSource::Mcp);
    assert_eq!(second.source_ref.as_deref(), Some("mail-server"));
    assert_eq!(second.description, "Обновлённое описание");
    assert_eq!(second.tier, ToolTier::Auto);
    assert_eq!(second.call_count, 1);
    assert_eq!(second.registered_at, first.registered_at);
    assert_eq!(second.updated_at, CLOCK_ORIGIN + 1000);
}

#[tokio::test]
async fn a_tool_that_becomes_destructive_is_pushed_back_to_off() {
    let host = host();
    assert_eq!(host.register(&read_only()).tier, ToolTier::Auto);
    let destructive = host.register(&ToolDescriptor {
        destructive: true,
        ..read_only()
    });
    assert_eq!(destructive.tier, ToolTier::Off);
}

#[tokio::test]
async fn a_declared_tier_replaces_the_default_only_until_the_user_changes_it() {
    let host = host();
    let declared = ToolDescriptor {
        destructive: true,
        tier: Some(ToolTier::Ask),
        ..descriptor()
    };
    let registered = host.register(&declared);
    assert_eq!(registered.tier, ToolTier::Ask);
    host.tools
        .set_tier(&registered.id.to_string(), ToolTier::Off, "user")
        .expect("sets");
    assert_eq!(host.register(&declared).tier, ToolTier::Off);
}

#[tokio::test]
async fn the_registered_ceiling_is_the_one_admission_composes_against() {
    let host = host();
    let registered = host.register(&read_only());
    assert_eq!(host.admit(&[]), Admission::Allow);
    host.tools
        .set_tier(&registered.id.to_string(), ToolTier::Ask, "user")
        .expect("sets");
    assert_eq!(host.admit(&[]), Admission::Ask);
    assert_eq!(host.grants(), 1);
}

#[tokio::test]
async fn a_narrowing_scope_cannot_widen_the_registered_ceiling() {
    let host = host();
    let id = host.register(&descriptor()).id.to_string();
    host.tools
        .set_tier(&id, ToolTier::Ask, "user")
        .expect("sets");
    host.grant("skill:test", ToolTier::Auto);
    assert_eq!(host.admit(&["skill:test"]), Admission::Ask);
    host.tools
        .set_tier(&id, ToolTier::Off, "user")
        .expect("sets");
    host.grant("scenario:one", ToolTier::Auto);
    assert_eq!(host.admit(&["scenario:one"]), Admission::Deny);
    host.grant("skill:test", ToolTier::Off);
    host.tools
        .set_tier(&id, ToolTier::Auto, "user")
        .expect("sets");
    assert_eq!(host.admit(&["skill:test"]), Admission::Deny);
}

#[tokio::test]
async fn an_always_allow_grant_cannot_widen_a_registered_ask_ceiling() {
    let host = host();
    host.register(&descriptor());
    let handle = host.start(&["skill:a"]);
    let approval = host.approval().await;
    host.runs
        .approve(&handle.id, &approval, true)
        .expect("approves");
    host.runs.wait(&handle.id).await;
    assert_eq!(host.executed(), 1);
    assert_eq!(host.admit(&["skill:a"]), Admission::Ask);
}

#[tokio::test]
async fn disabling_a_tool_blocks_admission_everywhere() {
    let host = host();
    let registered = host.register(&read_only());
    let allowed = host.start(&[]);
    host.runs.wait(&allowed.id).await;
    assert_eq!(
        host.runs.get(&allowed.id).expect("a run").status,
        RunStatus::Succeeded
    );
    host.tools
        .set_enabled(&registered.id.to_string(), false)
        .expect("disables");
    let blocked = host.start(&[]);
    host.runs.wait(&blocked.id).await;
    assert_eq!(
        host.outcome(&blocked.id),
        Some(AppErrorCode::PermissionDenied)
    );
    assert_eq!(host.executed(), 1);
    assert_eq!(host.admit(&[]), Admission::Deny);
}

#[tokio::test]
async fn setting_a_tool_to_off_fails_the_run_at_admission() {
    let host = host();
    let registered = host.register(&read_only());
    host.tools
        .set_tier(&registered.id.to_string(), ToolTier::Off, "user")
        .expect("sets");
    let handle = host.start(&[]);
    host.runs.wait(&handle.id).await;
    assert_eq!(
        host.outcome(&handle.id),
        Some(AppErrorCode::PermissionDenied)
    );
    assert_eq!(host.executed(), 0);
}

fn vector_search_run(runs: &RunService, kind: &str, subject: Option<&str>) -> RunHandleDto {
    start_on(
        runs,
        kind,
        subject,
        &[],
        "vector.search",
        json!({ "storeId": "0199aa11-1111-7111-8111-111111111111", "query": "вопрос" }),
    )
}

#[tokio::test]
async fn chat_stays_permission_free_even_when_its_capabilities_are_disabled() {
    let host = host();
    register_core_nodes(&host.nodes).expect("registers");
    let vector = register_kernel_tools(&host.nodes, &host.tools)
        .expect("registers")
        .into_iter()
        .find(|tool| tool.key == "vector.search")
        .expect("vector.search");
    assert_eq!(vector.tier, ToolTier::Auto);
    assert!(vector.read_only);
    host.tools
        .set_enabled(&vector.id.to_string(), false)
        .expect("disables");
    let search = Arc::new(RecordingSearch::default());
    let chat = run_service(
        &host,
        KernelServices {
            vector_stores: Some(search.clone()),
            ..KernelServices::default()
        },
    );
    let handle = vector_search_run(&chat, "chat", None);
    chat.wait(&handle.id).await;
    assert_eq!(
        chat.get(&handle.id).expect("a run").status,
        RunStatus::Succeeded
    );
    assert_eq!(*search.0.lock().expect("searches"), 1);
    chat.dispose().await;
}

#[tokio::test]
async fn a_disabled_tool_is_refused_in_the_runs_that_do_go_through_admission() {
    let host = host();
    register_core_nodes(&host.nodes).expect("registers");
    let vector = register_kernel_tools(&host.nodes, &host.tools)
        .expect("registers")
        .into_iter()
        .find(|tool| tool.key == "vector.search")
        .expect("vector.search");
    host.tools
        .set_enabled(&vector.id.to_string(), false)
        .expect("disables");
    let search = Arc::new(RecordingSearch::default());
    let scenario = run_service(
        &host,
        KernelServices {
            vector_stores: Some(search.clone()),
            ..KernelServices::default()
        },
    );
    let handle = vector_search_run(&scenario, "scenario", Some("one"));
    scenario.wait(&handle.id).await;
    assert_eq!(
        scenario
            .get(&handle.id)
            .expect("a run")
            .outcome
            .and_then(|outcome| outcome.code),
        Some(AppErrorCode::PermissionDenied)
    );
    assert_eq!(*search.0.lock().expect("searches"), 0);
    scenario.dispose().await;
}

#[tokio::test]
async fn usage_is_recorded_on_every_invocation_and_exposed_through_the_detail_rail() {
    let host = host();
    let id = host.register(&read_only()).id.to_string();
    for _ in 0..2 {
        let handle = host.start(&[]);
        host.runs.wait(&handle.id).await;
    }
    let usage = host.tools.usage(&id).expect("usage");
    assert_eq!(usage.call_count, 2);
    assert!(usage.last_used_at.is_some_and(|at| at > 0));
    assert_eq!(host.tools.get(&id).expect("a tool").tool.call_count, 2);
}

#[tokio::test]
async fn a_denied_invocation_is_not_counted_as_a_use() {
    let host = host();
    let id = host.register(&descriptor()).id.to_string();
    let handle = host.start(&[]);
    let approval = host.approval().await;
    host.runs.deny(&handle.id, &approval).expect("denies");
    host.runs.wait(&handle.id).await;
    let usage = host.tools.usage(&id).expect("usage");
    assert_eq!((usage.call_count, usage.last_used_at), (0, None));
}

#[tokio::test]
async fn references_are_a_replaceable_reverse_index_over_scenarios_and_skills() {
    let host = host();
    let mail = host.register(&descriptor()).id.to_string();
    let search = host
        .register(&ToolDescriptor {
            key: "vector.search".to_owned(),
            display_name: Some("vector.search".to_owned()),
            ..read_only()
        })
        .id
        .to_string();
    let triage = ToolReferenceSubject {
        kind: ToolReferenceKind::Scenario,
        ref_id: "s1".to_owned(),
        name: "Триаж".to_owned(),
    };
    host.tools
        .record_references(&triage, &["mail.send", "vector.search", "does.notExist"])
        .expect("records");
    host.tools
        .record_references(
            &ToolReferenceSubject {
                kind: ToolReferenceKind::Scenario,
                ref_id: "s2".to_owned(),
                name: "Эскалация".to_owned(),
            },
            &["mail.send"],
        )
        .expect("records");
    let reference = |kind, ref_id: &str, name: &str| ToolReferenceDto {
        kind,
        ref_id: ref_id.to_owned(),
        name: name.to_owned(),
    };
    assert_eq!(
        host.tools.get(&mail).expect("a tool").references,
        [
            reference(ToolReferenceKind::Scenario, "s1", "Триаж"),
            reference(ToolReferenceKind::Scenario, "s2", "Эскалация"),
        ]
    );
    host.tools
        .record_references(&triage, &["vector.search"])
        .expect("records");
    assert_eq!(
        host.tools.get(&mail).expect("a tool").references,
        [reference(ToolReferenceKind::Scenario, "s2", "Эскалация")]
    );
    assert_eq!(
        host.tools.references(&search).expect("references"),
        [reference(ToolReferenceKind::Scenario, "s1", "Триаж")]
    );
}

#[tokio::test]
async fn deregistering_keeps_the_row_its_history_and_references_and_denies_admission() {
    let host = host();
    let id = host.register(&read_only()).id.to_string();
    let first = host.start(&[]);
    host.runs.wait(&first.id).await;
    host.tools
        .record_references(
            &ToolReferenceSubject {
                kind: ToolReferenceKind::Scenario,
                ref_id: "s1".to_owned(),
                name: "Триаж".to_owned(),
            },
            &["mail.send"],
        )
        .expect("records");
    let gone = host.tools.deregister("mail.send").expect("deregisters");
    assert_eq!(gone.map(|tool| tool.available), Some(false));
    assert!(host.keys(&available_only()).is_empty());
    assert_eq!(host.keys(&ToolListFilter::default()).len(), 1);
    let usage = host.tools.usage(&id).expect("usage");
    assert_eq!((usage.call_count, usage.references.len()), (1, 1));
    assert_eq!(host.admit(&[]), Admission::Deny);
    let blocked = host.start(&[]);
    host.runs.wait(&blocked.id).await;
    assert_eq!(
        host.outcome(&blocked.id),
        Some(AppErrorCode::PermissionDenied)
    );
    assert!(host.register(&read_only()).available);
    assert_eq!(host.tools.usage(&id).expect("usage").call_count, 1);
}

#[tokio::test]
async fn deregistering_a_source_withdraws_exactly_that_servers_tools() {
    let host = host();
    for (key, server) in [
        ("mail.send", "server-a"),
        ("mail.draft", "server-a"),
        ("fs.readFile", "server-b"),
    ] {
        host.register(&ToolDescriptor {
            key: key.to_owned(),
            source: ToolSource::Mcp,
            source_ref: Some(server.to_owned()),
            ..descriptor()
        });
    }
    let withdrawn: Vec<String> = host
        .tools
        .deregister_source(ToolSource::Mcp, Some("server-a"))
        .expect("withdraws")
        .into_iter()
        .map(|tool| tool.key)
        .collect();
    assert_eq!(withdrawn, ["mail.draft", "mail.send"]);
    assert_eq!(host.keys(&available_only()), ["fs.readFile"]);
}

#[tokio::test]
async fn the_list_filters_by_source_tier_and_free_text() {
    let host = host();
    host.register(&ToolDescriptor {
        key: "vector.search".to_owned(),
        description: "Семантический поиск".to_owned(),
        ..read_only()
    });
    host.register(&ToolDescriptor {
        source: ToolSource::Mcp,
        source_ref: Some("server-a".to_owned()),
        ..descriptor()
    });
    let filter = |source, tier, query: Option<&str>| ToolListFilter {
        source,
        tier,
        query: query.map(str::to_owned),
        include_unavailable: None,
    };
    assert_eq!(
        host.keys(&filter(Some(ToolSource::Mcp), None, None)),
        ["mail.send"]
    );
    assert_eq!(
        host.keys(&filter(None, Some(ToolTier::Auto), None)),
        ["vector.search"]
    );
    assert_eq!(
        host.keys(&filter(None, None, Some("ПИСЬМО"))),
        ["mail.send"]
    );
}

#[tokio::test]
async fn generation_is_not_a_tool_and_never_reaches_the_registry() {
    let host = host();
    register_core_nodes(&host.nodes).expect("registers");
    register_kernel_tools(&host.nodes, &host.tools).expect("registers");
    assert!(
        !host
            .keys(&ToolListFilter::default())
            .contains(&"llm.generate".to_owned())
    );
    assert_eq!(host.tools.ceiling("llm.generate").expect("reads"), None);
}

#[tokio::test]
async fn kernel_nodes_register_themselves_as_builtin_capabilities_exactly_once() {
    let host = host();
    register_core_nodes(&host.nodes).expect("registers");
    let mut first: Vec<String> = register_kernel_tools(&host.nodes, &host.tools)
        .expect("registers")
        .into_iter()
        .map(|tool| tool.key)
        .collect();
    let mut second: Vec<String> = register_kernel_tools(&host.nodes, &host.tools)
        .expect("registers")
        .into_iter()
        .map(|tool| tool.key)
        .collect();
    first.sort();
    second.sort();
    assert_eq!(first, second);
    assert_eq!(
        host.keys(&ToolListFilter::default()),
        [
            "knowledge.search",
            "mail.send",
            "vector.list",
            "vector.search"
        ]
    );
    let tools = host.tools.list(&ToolListFilter::default()).expect("lists");
    let search = tools
        .iter()
        .find(|tool| tool.key == "knowledge.search")
        .expect("knowledge.search");
    assert_eq!(
        (search.source, search.read_only, search.tier),
        (ToolSource::Builtin, true, ToolTier::Auto)
    );
    assert!(!search.input_schema.is_null());
    assert_eq!(
        tools
            .iter()
            .find(|tool| tool.key == "vector.search")
            .map(|tool| tool.tier),
        Some(ToolTier::Auto)
    );
}

#[tokio::test]
async fn kernel_jobs_never_become_tools() {
    let host = host();
    register_core_nodes(&host.nodes).expect("registers");
    crate::kernel::register_job_nodes(&host.nodes).expect("registers");
    crate::kernel::register_download_nodes(&host.nodes).expect("registers");
    let registered: Vec<String> = register_kernel_tools(&host.nodes, &host.tools)
        .expect("registers")
        .into_iter()
        .map(|tool| tool.key)
        .collect();
    for job in ["vector.index", "job.run", "download.fetch"] {
        assert!(host.nodes.get(job).is_ok(), "{job} is a kernel node");
        assert!(!registered.contains(&job.to_owned()), "{job}");
        assert_eq!(host.tools.ceiling(job).expect("reads"), None, "{job}");
        assert!(!model_tool(job), "{job}");
    }
    assert!(!model_tool("vector.search"));
    assert!(model_tool("fs.read"));
}

#[test]
fn old_tool_keys_map_onto_the_merged_tools() {
    let keys = [
        "skill.load",
        "chat.file",
        "fs.read",
        "skill.script",
        "skill.read",
        "knowledge.read",
        "knowledge.list",
        "job.run",
        "mcp.server.tool",
    ]
    .map(str::to_owned);
    assert_eq!(
        current_tool_keys(&keys),
        [
            "skill.load",
            "fs.read",
            "fs.list",
            "fs.apply",
            "shell.run",
            "knowledge.search",
            "mcp.server.tool"
        ]
    );
}

#[tokio::test]
async fn permission_defaults_apply_to_newly_registered_tools_only() {
    let host = host();
    let settings = SettingService::new(host.database.shared(), host.clock.clone());
    let tools = ToolRegistry::new(ToolRegistryOptions {
        settings: Some(settings.clone()),
        clock: host.clock.clone(),
        ..ToolRegistryOptions::new(host.database.shared())
    });
    let register = |descriptor: ToolDescriptor| tools.register(&descriptor).expect("registers");
    let turned_off = register(descriptor());
    tools
        .set_tier(&turned_off.id.to_string(), ToolTier::Off, "user")
        .expect("sets");
    let disabled = register(ToolDescriptor {
        key: "mail.archive".to_owned(),
        ..descriptor()
    });
    tools
        .set_enabled(&disabled.id.to_string(), false)
        .expect("disables");
    let forward = || ToolDescriptor {
        key: "mail.forward".to_owned(),
        source: ToolSource::Mcp,
        source_ref: Some("s".to_owned()),
        ..descriptor()
    };
    let withdrawn = register(forward());
    tools
        .set_tier(&withdrawn.id.to_string(), ToolTier::Off, "user")
        .expect("sets");
    tools
        .deregister_source(ToolSource::Mcp, Some("s"))
        .expect("withdraws");

    assert_eq!(tools.defaults().builtin, ToolDefaultTier::Inferred);
    tools
        .set_defaults(ToolDefaultsDto {
            builtin: ToolDefaultTier::Auto,
            mcp: ToolDefaultTier::Auto,
            ..ToolDefaultsDto::default()
        })
        .expect("sets");
    let reloaded = ToolRegistry::new(ToolRegistryOptions {
        settings: Some(settings),
        ..ToolRegistryOptions::new(host.database.shared())
    })
    .defaults();
    assert_eq!(
        (reloaded.builtin, reloaded.mcp, reloaded.skill),
        (
            ToolDefaultTier::Auto,
            ToolDefaultTier::Auto,
            ToolDefaultTier::Inferred
        )
    );

    assert_eq!(register(descriptor()).tier, ToolTier::Off);
    let archive = register(ToolDescriptor {
        key: "mail.archive".to_owned(),
        ..descriptor()
    });
    assert_eq!((archive.tier, archive.enabled), (ToolTier::Ask, false));
    assert_eq!(register(forward()).tier, ToolTier::Off);
    assert_eq!(
        register(ToolDescriptor {
            key: "mail.reply".to_owned(),
            ..descriptor()
        })
        .tier,
        ToolTier::Auto
    );
    assert_eq!(
        register(ToolDescriptor {
            key: "mail.purge".to_owned(),
            destructive: true,
            ..descriptor()
        })
        .tier,
        ToolTier::Off
    );
    let browser = |key: &str| ToolDescriptor {
        key: key.to_owned(),
        source: ToolSource::Browser,
        ..read_only()
    };
    assert_eq!(register(browser("web.fetch")).tier, ToolTier::Auto);
    tools
        .set_defaults(ToolDefaultsDto {
            browser: ToolDefaultTier::Ask,
            ..ToolDefaultsDto::default()
        })
        .expect("sets");
    assert_eq!(register(browser("web.read")).tier, ToolTier::Ask);
    assert_eq!(
        host.runs
            .permissions()
            .admit_subject(
                "mail.send",
                &PermissionContext::new("r", &[]),
                ToolTier::Auto
            )
            .expect("admits"),
        Admission::Deny
    );
}
