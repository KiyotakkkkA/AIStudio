use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use zvs_contract::{
    AppErrorCode, CreateScenarioInput, ScenarioAttachmentDto, ScenarioEdgeDto, ScenarioGraphDto,
    ScenarioIssueDto, ScenarioIssueRule, ScenarioNodeDto, ScenarioPositionDto, ScenarioSlot,
    ScenarioTriggerKind, ToolListFilter, UpdateScenarioInput,
};

use super::catalogue::{
    AGENT_TOOLS, EMBEDDING_MODEL, MEMORY_WINDOW, MODEL_CHAT, TRIGGER_CHAT, TRIGGER_MAIL,
    TRIGGER_SCHEDULE,
};
use super::*;
use crate::ipc::IpcServer;
use crate::kernel::NodeRegistry;
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TemporaryDatabase, temporary_database, test_services};

const SEARCH: &str = "kb.search";

fn catalogue() -> ScenarioCatalogue {
    ScenarioCatalogue::builtin()
        .with(NodeSpec {
            fills: vec![ScenarioSlot::Tool],
            required_params: vec!["query".to_owned()],
            tool_key: Some(SEARCH.to_owned()),
            ..NodeSpec::action(SEARCH)
        })
        .with(
            NodeSpec::action("kb.store").with_slots(&[SlotSpec::required(ScenarioSlot::Embedding)]),
        )
}

fn node(id: &str, node_type: &str) -> ScenarioNodeDto {
    ScenarioNodeDto {
        id: id.to_owned(),
        node_type: node_type.to_owned(),
        name: None,
        position: ScenarioPositionDto { x: 0.5, y: 0.5 },
        params: Map::new(),
        disabled: None,
    }
}

fn with_params(mut node: ScenarioNodeDto, params: Value) -> ScenarioNodeDto {
    node.params = params.as_object().cloned().unwrap_or_default();
    node
}

fn edge(from: &str, to: &str) -> ScenarioEdgeDto {
    ScenarioEdgeDto {
        from: from.to_owned(),
        to: to.to_owned(),
        from_port: "main".to_owned(),
        to_port: "main".to_owned(),
    }
}

fn attach(parent: &str, child: &str, slot: ScenarioSlot) -> ScenarioAttachmentDto {
    ScenarioAttachmentDto {
        parent_node_id: parent.to_owned(),
        child_node_id: child.to_owned(),
        slot,
    }
}

fn model() -> ScenarioNodeDto {
    with_params(
        node("model", MODEL_CHAT),
        json!({ "providerId": "p", "modelId": "gpt-oss:120b" }),
    )
}

fn valid_graph() -> ScenarioGraphDto {
    ScenarioGraphDto {
        nodes: vec![
            node("trigger", TRIGGER_CHAT),
            node("agent", AGENT_TOOLS),
            model(),
            node("memory", MEMORY_WINDOW),
            node("search", SEARCH),
        ],
        edges: vec![edge("trigger", "agent")],
        attachments: vec![
            attach("agent", "model", ScenarioSlot::Model),
            attach("agent", "memory", ScenarioSlot::Memory),
            attach("agent", "search", ScenarioSlot::Tool),
        ],
    }
}

fn issues(graph: &ScenarioGraphDto) -> Vec<ScenarioIssueDto> {
    validate_graph(graph, &catalogue())
}

fn has(graph: &ScenarioGraphDto, expected: &ScenarioIssueDto) {
    let found = issues(graph);
    assert!(found.contains(expected), "{expected:?} not in {found:?}");
}

fn rule(rule: ScenarioIssueRule, node_id: &str) -> ScenarioIssueDto {
    ScenarioIssueDto::node(rule, node_id)
}

#[test]
fn a_trunk_with_its_sub_nodes_is_valid_and_binds_its_trigger() {
    let graph = valid_graph();
    assert_eq!(issues(&graph), []);
    let binding = trigger_binding(&graph, &catalogue()).expect("a trigger");
    assert_eq!(binding.kind, ScenarioTriggerKind::Chat);
    assert_eq!(binding.node_id, "trigger");
}

#[test]
fn an_unknown_node_type_is_named() {
    let mut graph = valid_graph();
    graph.nodes.push(node("ghost", "n8n-nodes-base.slack"));
    graph.edges.push(edge("agent", "ghost"));
    assert_eq!(
        issues(&graph),
        [rule(ScenarioIssueRule::UnknownNodeType, "ghost")]
    );
}

#[test]
fn a_repeated_node_id_is_named() {
    let mut graph = valid_graph();
    graph.nodes.push(node("agent", AGENT_TOOLS));
    has(&graph, &rule(ScenarioIssueRule::DuplicateNode, "agent"));
}

#[test]
fn an_edge_or_attachment_to_a_missing_node_names_the_missing_id() {
    let mut graph = valid_graph();
    graph.edges.push(edge("agent", "nowhere"));
    graph
        .attachments
        .push(attach("agent", "lost", ScenarioSlot::Tool));
    let found = issues(&graph);
    assert!(found.contains(&rule(ScenarioIssueRule::UnknownNode, "nowhere")));
    assert!(found.contains(&rule(ScenarioIssueRule::UnknownNode, "lost")));
}

#[test]
fn a_repeated_edge_is_named_by_its_source() {
    let mut graph = valid_graph();
    graph.edges.push(edge("trigger", "agent"));
    assert_eq!(
        issues(&graph),
        [rule(ScenarioIssueRule::DuplicateEdge, "trigger")]
    );
}

#[test]
fn main_edges_never_touch_sub_nodes_or_enter_a_trigger() {
    let mut graph = valid_graph();
    graph.edges.push(edge("agent", "model"));
    has(&graph, &rule(ScenarioIssueRule::InvalidEdge, "model"));

    let mut graph = valid_graph();
    graph.edges.push(edge("agent", "trigger"));
    has(&graph, &rule(ScenarioIssueRule::InvalidEdge, "trigger"));

    let mut graph = valid_graph();
    graph.edges.push(edge("agent", "search"));
    has(&graph, &rule(ScenarioIssueRule::InvalidEdge, "search"));
}

#[test]
fn a_cycle_in_the_main_graph_names_every_node_on_it() {
    let mut graph = valid_graph();
    graph
        .nodes
        .push(with_params(node("a", SEARCH), json!({ "query": "x" })));
    graph
        .nodes
        .push(with_params(node("b", SEARCH), json!({ "query": "y" })));
    graph
        .edges
        .extend([edge("agent", "a"), edge("a", "b"), edge("b", "a")]);
    let found = issues(&graph);
    assert_eq!(
        found,
        [
            rule(ScenarioIssueRule::Cycle, "a"),
            rule(ScenarioIssueRule::Cycle, "b")
        ]
    );

    let mut graph = valid_graph();
    graph
        .nodes
        .push(with_params(node("loop", SEARCH), json!({ "query": "x" })));
    graph
        .edges
        .extend([edge("agent", "loop"), edge("loop", "loop")]);
    assert_eq!(issues(&graph), [rule(ScenarioIssueRule::Cycle, "loop")]);
}

#[test]
fn an_attachment_cycle_is_named() {
    let mut graph = valid_graph();
    graph
        .attachments
        .push(attach("model", "model", ScenarioSlot::Model));
    has(&graph, &rule(ScenarioIssueRule::Cycle, "model"));
}

#[test]
fn an_empty_required_slot_is_named_with_its_parent_and_slot() {
    let mut graph = valid_graph();
    graph.nodes.retain(|node| node.id != "model");
    graph
        .attachments
        .retain(|attachment| attachment.slot != ScenarioSlot::Model);
    assert_eq!(
        issues(&graph),
        [ScenarioIssueDto::slot(
            ScenarioIssueRule::MissingAttachment,
            "agent",
            ScenarioSlot::Model
        )]
    );
}

#[test]
fn a_disabled_sub_node_does_not_fill_a_required_slot() {
    let mut graph = valid_graph();
    graph.nodes[2].disabled = Some(true);
    assert_eq!(
        issues(&graph),
        [ScenarioIssueDto::slot(
            ScenarioIssueRule::MissingAttachment,
            "agent",
            ScenarioSlot::Model
        )]
    );
}

#[test]
fn a_nested_sub_node_slot_is_checked_too() {
    let mut graph = valid_graph();
    graph.nodes.push(node("store", "kb.store"));
    graph.edges.push(edge("agent", "store"));
    has(
        &graph,
        &ScenarioIssueDto::slot(
            ScenarioIssueRule::MissingAttachment,
            "store",
            ScenarioSlot::Embedding,
        ),
    );
    graph.nodes.push(with_params(
        node("embed", EMBEDDING_MODEL),
        json!({ "providerId": "p", "modelId": "mxbai-embed-large" }),
    ));
    graph
        .attachments
        .push(attach("store", "embed", ScenarioSlot::Embedding));
    assert_eq!(issues(&graph), []);
}

#[test]
fn a_single_slot_takes_one_sub_node() {
    let mut graph = valid_graph();
    graph.nodes.push(ScenarioNodeDto {
        id: "second".to_owned(),
        ..model()
    });
    graph
        .attachments
        .push(attach("agent", "second", ScenarioSlot::Model));
    assert_eq!(
        issues(&graph),
        [ScenarioIssueDto::slot(
            ScenarioIssueRule::TooManyAttachments,
            "agent",
            ScenarioSlot::Model
        )]
    );
}

#[test]
fn a_slot_the_parent_lacks_or_the_child_cannot_fill_is_named() {
    let mut graph = valid_graph();
    graph
        .attachments
        .push(attach("trigger", "memory", ScenarioSlot::Memory));
    has(
        &graph,
        &ScenarioIssueDto::slot(
            ScenarioIssueRule::UnsupportedSlot,
            "trigger",
            ScenarioSlot::Memory,
        ),
    );

    let mut graph = valid_graph();
    graph.attachments[1] = attach("agent", "memory", ScenarioSlot::Tool);
    has(
        &graph,
        &ScenarioIssueDto::slot(
            ScenarioIssueRule::SlotMismatch,
            "memory",
            ScenarioSlot::Tool,
        ),
    );
}

#[test]
fn a_sub_node_hangs_from_one_parent() {
    let mut graph = valid_graph();
    graph.nodes.push(node("other", AGENT_TOOLS));
    graph.edges.push(edge("agent", "other"));
    graph
        .attachments
        .push(attach("other", "model", ScenarioSlot::Model));
    has(&graph, &rule(ScenarioIssueRule::MultipleParents, "model"));
}

#[test]
fn a_missing_or_blank_required_parameter_is_named() {
    let mut graph = valid_graph();
    graph.nodes[2] = with_params(
        node("model", MODEL_CHAT),
        json!({ "providerId": "p", "modelId": "  " }),
    );
    assert_eq!(
        issues(&graph),
        [ScenarioIssueDto::param("model", "modelId")]
    );

    let mut graph = valid_graph();
    graph.nodes[0] = node("trigger", TRIGGER_SCHEDULE);
    assert_eq!(issues(&graph), [ScenarioIssueDto::param("trigger", "cron")]);
}

#[test]
fn a_tool_sub_node_leaves_its_input_to_the_model_but_a_trunk_step_does_not() {
    let mut graph = valid_graph();
    assert!(graph.nodes[4].params.is_empty());
    assert_eq!(issues(&graph), []);
    graph.nodes.push(node("step", SEARCH));
    graph.edges.push(edge("agent", "step"));
    assert_eq!(issues(&graph), [ScenarioIssueDto::param("step", "query")]);
}

#[test]
fn a_scenario_needs_exactly_one_trigger() {
    let mut graph = valid_graph();
    graph.nodes.remove(0);
    graph.edges.clear();
    assert_eq!(
        issues(&graph),
        [ScenarioIssueDto::graph(ScenarioIssueRule::NoTrigger)]
    );

    let mut graph = valid_graph();
    graph.nodes.push(node("mail", TRIGGER_MAIL));
    graph.edges.push(edge("mail", "agent"));
    assert_eq!(
        issues(&graph),
        [rule(ScenarioIssueRule::MultipleTriggers, "mail")]
    );
}

#[test]
fn a_node_no_trigger_leads_to_is_unreachable() {
    let mut graph = valid_graph();
    graph.nodes.push(node("island", AGENT_TOOLS));
    graph.nodes.push(ScenarioNodeDto {
        id: "island-model".to_owned(),
        ..model()
    });
    graph
        .attachments
        .push(attach("island", "island-model", ScenarioSlot::Model));
    graph.nodes.push(node("loose", MEMORY_WINDOW));
    assert_eq!(
        issues(&graph),
        [
            rule(ScenarioIssueRule::Unreachable, "island"),
            rule(ScenarioIssueRule::Unreachable, "island-model"),
            rule(ScenarioIssueRule::Unreachable, "loose"),
        ]
    );
}

#[test]
fn the_diff_names_added_removed_and_changed_nodes_and_counts_connections() {
    let before = valid_graph();
    let mut after = valid_graph();
    after.nodes.retain(|node| node.id != "memory");
    after
        .attachments
        .retain(|attachment| attachment.child_node_id != "memory");
    after.nodes[1]
        .params
        .insert("maxIterations".to_owned(), json!(8));
    after.nodes[0].position = ScenarioPositionDto { x: 99.5, y: 0.5 };
    after.nodes.push(node("reply", SEARCH));
    after.edges.push(edge("agent", "reply"));
    let summary = diff_graphs(&before, &after);
    assert_eq!(summary.nodes_added, ["reply"]);
    assert_eq!(summary.nodes_removed, ["memory"]);
    assert_eq!(summary.nodes_changed, ["agent"]);
    assert_eq!((summary.edges_added, summary.edges_removed), (1, 0));
    assert_eq!(
        (summary.attachments_added, summary.attachments_removed),
        (0, 1)
    );
}

struct Host {
    _database: TemporaryDatabase,
    clock: Arc<FakeClock>,
    service: ScenarioService,
}

fn host() -> Host {
    let database = temporary_database();
    let clock = Arc::new(FakeClock::default());
    let service = ScenarioService::new(ScenarioServiceOptions {
        clock: clock.clone(),
        ..ScenarioServiceOptions::new(database.shared(), Arc::new(NodeRegistry::new()))
    });
    Host {
        _database: database,
        clock,
        service,
    }
}

fn builtin_graph() -> ScenarioGraphDto {
    let mut graph = valid_graph();
    graph.nodes.retain(|node| node.id != "search");
    graph
        .attachments
        .retain(|attachment| attachment.child_node_id != "search");
    graph
}

fn create(host: &Host, graph: ScenarioGraphDto) -> zvs_contract::ScenarioDetailDto {
    host.service
        .create(&CreateScenarioInput {
            name: "  Триаж  ".to_owned(),
            tags: Some(vec!["support".to_owned(), " support ".to_owned()]),
            graph: Some(graph),
            note: Some("Первая версия".to_owned()),
        })
        .expect("creates")
}

#[test]
fn creating_a_scenario_writes_version_one_and_round_trips_the_attachments() {
    let host = host();
    let created = create(&host, builtin_graph());
    assert_eq!(created.scenario.name, "Триаж");
    assert_eq!(created.scenario.tags, ["support"]);
    assert_eq!(created.scenario.version, 1);
    assert!(!created.scenario.active);
    assert_eq!(created.issues, []);
    let fetched = host.service.get(&created.scenario.id).expect("reads");
    assert_eq!(fetched.graph, builtin_graph());
    assert_eq!(fetched.graph.attachments, builtin_graph().attachments);
    assert_eq!(
        fetched.scenario.trigger.map(|trigger| trigger.kind),
        Some(ScenarioTriggerKind::Chat)
    );
    let versions = host.service.versions(&created.scenario.id).expect("lists");
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].note.as_deref(), Some("Первая версия"));
    assert_eq!(versions[0].summary.nodes_added.len(), 4);
}

#[test]
fn saving_a_changed_graph_adds_a_version_and_keeps_the_old_one() {
    let host = host();
    let created = create(&host, builtin_graph());
    let first = created.scenario.current_version_id;
    host.clock.advance(1000);
    let mut next = builtin_graph();
    next.nodes.retain(|node| node.id != "memory");
    next.attachments
        .retain(|attachment| attachment.child_node_id != "memory");
    let updated = host
        .service
        .update(&UpdateScenarioInput {
            id: created.scenario.id,
            name: None,
            tags: None,
            graph: Some(next.clone()),
            note: Some("Без памяти".to_owned()),
        })
        .expect("updates");
    assert_eq!(updated.scenario.version, 2);
    assert_ne!(updated.scenario.current_version_id, first);
    assert_eq!(updated.graph, next);
    let versions = host.service.versions(&created.scenario.id).expect("lists");
    assert_eq!(
        versions
            .iter()
            .map(|version| version.version)
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(versions[0].summary.nodes_removed, ["memory"]);
    let (pinned, graph) = host.service.version(&first).expect("the old version");
    assert_eq!(pinned.version, 1);
    assert_eq!(graph, builtin_graph());

    let renamed = host
        .service
        .update(&UpdateScenarioInput {
            id: created.scenario.id,
            name: Some("Новый триаж".to_owned()),
            tags: None,
            graph: Some(next),
            note: None,
        })
        .expect("renames");
    assert_eq!(renamed.scenario.name, "Новый триаж");
    assert_eq!(renamed.scenario.version, 2);
    assert_eq!(
        host.service
            .versions(&created.scenario.id)
            .expect("lists")
            .len(),
        2
    );
}

fn issue_list(error: &zvs_contract::AppError) -> Vec<ScenarioIssueDto> {
    serde_json::from_value(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("issues"))
            .cloned()
            .expect("issues are attached"),
    )
    .expect("issues parse")
}

#[test]
fn an_invalid_scenario_cannot_be_activated_and_the_error_names_the_node() {
    let host = host();
    let mut graph = builtin_graph();
    graph.nodes.retain(|node| node.id != "model");
    graph
        .attachments
        .retain(|attachment| attachment.slot != ScenarioSlot::Model);
    let created = create(&host, graph);
    assert_eq!(created.scenario.issue_count, 1);
    let error = host
        .service
        .set_active(&created.scenario.id, true)
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, SCENARIO_INVALID);
    assert_eq!(
        issue_list(&error),
        [ScenarioIssueDto::slot(
            ScenarioIssueRule::MissingAttachment,
            "agent",
            ScenarioSlot::Model
        )]
    );
    assert!(
        !host
            .service
            .get(&created.scenario.id)
            .expect("reads")
            .scenario
            .active
    );
}

#[test]
fn an_active_scenario_is_found_by_its_trigger_and_refuses_an_invalid_save() {
    let host = host();
    let created = create(&host, builtin_graph());
    let active = host
        .service
        .set_active(&created.scenario.id, true)
        .expect("activates");
    assert!(active.active);
    assert_eq!(
        host.service
            .active_for_trigger(ScenarioTriggerKind::Chat)
            .expect("lists")
            .len(),
        1
    );
    let error = host
        .service
        .update(&UpdateScenarioInput {
            id: created.scenario.id,
            name: None,
            tags: None,
            graph: Some(ScenarioGraphDto::default()),
            note: None,
        })
        .expect_err("refused");
    assert_eq!(error.message, ACTIVE_SCENARIO_INVALID);
    assert_eq!(
        issue_list(&error),
        [ScenarioIssueDto::graph(ScenarioIssueRule::NoTrigger)]
    );
    assert_eq!(
        host.service
            .versions(&created.scenario.id)
            .expect("lists")
            .len(),
        1
    );
    host.service
        .set_active(&created.scenario.id, false)
        .expect("deactivates");
    assert!(
        host.service
            .active_for_trigger(ScenarioTriggerKind::Chat)
            .expect("lists")
            .is_empty()
    );
}

#[test]
fn removing_a_scenario_removes_its_history() {
    let host = host();
    let created = create(&host, builtin_graph());
    let version = created.scenario.current_version_id;
    host.service.remove(&created.scenario.id).expect("removes");
    assert_eq!(
        host.service
            .get(&created.scenario.id)
            .expect_err("gone")
            .code,
        AppErrorCode::NotFound
    );
    assert_eq!(
        host.service.version(&version).expect_err("gone").code,
        AppErrorCode::NotFound
    );
    assert_eq!(
        host.service
            .remove(&created.scenario.id)
            .expect_err("gone")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn the_channels_create_validate_activate_and_list_versions_in_the_contract_shape() {
    let services = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let tool = services
        .nodes
        .list()
        .into_iter()
        .find_map(|node| node.permission().tool_key().map(str::to_owned))
        .expect("a registered tool node");
    let mut server =
        IpcServer::new(services.shared(), silent_logger()).with_output_validation(true);
    crate::ipc::scenarios::register(&mut server);
    let graph = json!({
        "nodes": [
            { "id": "trigger", "type": TRIGGER_CHAT, "position": { "x": 0.5, "y": 0.5 }, "params": {} },
            { "id": "agent", "type": AGENT_TOOLS, "position": { "x": 200.5, "y": 0.5 }, "params": {} },
            { "id": "tool", "type": tool, "position": { "x": 200.5, "y": 200.5 }, "params": {} }
        ],
        "edges": [{ "from": "trigger", "to": "agent", "fromPort": "main", "toPort": "main" }],
        "attachments": [{ "parentNodeId": "agent", "childNodeId": "tool", "slot": "tool" }]
    });
    let validated = server
        .dispatch("scenarios.validate", json!({ "graph": graph }))
        .await;
    assert_eq!(validated["data"]["valid"], json!(false), "{validated}");
    assert_eq!(
        validated["data"]["issues"],
        json!([{ "rule": "missingAttachment", "nodeId": "agent", "slot": "model" }])
    );
    let created = server
        .dispatch(
            "scenarios.create",
            json!({ "name": "Триаж", "graph": graph }),
        )
        .await;
    assert_eq!(created["ok"], json!(true), "{created}");
    let id = created["data"]["id"].clone();
    let refused = server
        .dispatch("scenarios.activate", json!({ "id": id, "active": true }))
        .await;
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(refused["error"]["code"], json!("VALIDATION_FAILED"));
    assert_eq!(
        refused["error"]["details"]["issues"][0]["nodeId"],
        json!("agent")
    );
    let tools = services
        .tools
        .list(&ToolListFilter::default())
        .expect("lists tools");
    let referenced = tools
        .iter()
        .find(|row| row.key == tool)
        .expect("the tool is catalogued");
    let references = services
        .tools
        .references(&referenced.id.to_string())
        .expect("references");
    assert!(
        references.iter().any(|reference| reference.name == "Триаж"),
        "{references:?}"
    );
    let listed = server.dispatch("scenarios.list", json!(null)).await;
    assert_eq!(listed["data"][0]["issueCount"], json!(1), "{listed}");
    let versions = server
        .dispatch("scenarios.versions", json!({ "id": id }))
        .await;
    assert_eq!(versions["data"][0]["version"], json!(1), "{versions}");
    let removed = server
        .dispatch("scenarios.remove", json!({ "id": id }))
        .await;
    assert_eq!(
        removed["data"],
        json!({ "id": id, "removed": true }),
        "{removed}"
    );
}
