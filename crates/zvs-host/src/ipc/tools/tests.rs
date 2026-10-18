use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{RunStatus, ToolDto, ToolSource};

use super::*;
use crate::kernel::{FnNode, PermissionRequirement};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};
use crate::tools::ToolDescriptor;

fn registered() -> (TestServices, IpcServer) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    host.nodes
        .register(FnNode::new(
            "tool",
            PermissionRequirement::tool("mail.send"),
            true,
            |_context, _input: Value| async { Ok("executed".to_owned()) },
        ))
        .expect("registers");
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    (host, server)
}

fn mail(host: &TestServices) -> ToolDto {
    host.tools
        .register(&ToolDescriptor {
            read_only: true,
            ..ToolDescriptor::new("mail.send", ToolSource::Builtin)
        })
        .expect("registers")
}

#[tokio::test]
async fn the_channels_expose_the_registry_and_match_the_contract() {
    let (host, server) = registered();
    let tool = mail(&host);
    let id = tool.id.to_string();
    let listed = server.dispatch("tools.list", json!({})).await;
    assert_eq!(listed["ok"], json!(true), "{listed}");
    assert!(
        listed["data"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["key"] == "mail.send"))
    );
    let tiered = server
        .dispatch("tools.setTier", json!({ "id": id, "tier": "off" }))
        .await;
    assert_eq!(tiered["data"]["tier"], json!("off"), "{tiered}");
    let disabled = server
        .dispatch("tools.setEnabled", json!({ "id": id, "enabled": false }))
        .await;
    assert_eq!(disabled["data"]["enabled"], json!(false));
    let detail = server.dispatch("tools.get", json!({ "id": id })).await;
    assert_eq!(
        (
            &detail["data"]["key"],
            &detail["data"]["tier"],
            &detail["data"]["enabled"],
            &detail["data"]["references"]
        ),
        (
            &json!("mail.send"),
            &json!("off"),
            &json!(false),
            &json!([])
        )
    );
    let usage = server.dispatch("tools.usage", json!({ "id": id })).await;
    assert_eq!(usage["data"]["callCount"], json!(0));
    let missing = server
        .dispatch(
            "tools.get",
            json!({ "id": "0199aa11-1111-7111-8111-111111111111" }),
        )
        .await;
    assert_eq!(missing["error"]["code"], json!("NOT_FOUND"));
}

#[tokio::test]
async fn setting_a_tool_to_off_through_the_channel_fails_the_run_at_admission() {
    let (host, server) = registered();
    let tool = mail(&host);
    server
        .dispatch(
            "tools.setTier",
            json!({ "id": tool.id.to_string(), "tier": "off" }),
        )
        .await;
    let handle = host
        .runs
        .start(
            zvs_contract::parse(json!({
                "kind": "job",
                "graph": { "nodes": [{ "id": "a", "type": "tool" }] },
            }))
            .expect("a start input"),
        )
        .expect("starts");
    host.runs.wait(&handle.id).await;
    let run = host.runs.get(&handle.id).expect("a run");
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(
        run.outcome.and_then(|outcome| outcome.code),
        Some(zvs_contract::AppErrorCode::PermissionDenied)
    );
}

#[tokio::test]
async fn the_try_it_and_defaults_channels_match_the_contract() {
    let (host, server) = registered();
    let tool = mail(&host);
    let invoked = server
        .dispatch("tools.invoke", json!({ "id": tool.id.to_string() }))
        .await;
    assert_eq!(invoked["data"]["ok"], json!(true), "{invoked}");
    assert_eq!(invoked["data"]["output"], json!("executed"));
    let set = server
        .dispatch("tools.setDefaults", json!({ "mcp": "off" }))
        .await;
    assert_eq!(set["data"]["mcp"], json!("off"), "{set}");
    assert_eq!(set["data"]["builtin"], json!("inferred"));
    let defaults = server.dispatch("tools.defaults", Value::Null).await;
    assert_eq!(defaults["data"]["mcp"], json!("off"), "{defaults}");
    let refused = server
        .dispatch(
            "tools.invoke",
            json!({ "id": tool.id.to_string(), "input": {}, "confirmed": null }),
        )
        .await;
    assert_eq!(refused["error"]["code"], json!("VALIDATION_FAILED"));
}

#[tokio::test]
async fn the_kernel_tools_are_registered_once_in_the_composed_host() {
    let (host, server) = registered();
    let listed = server
        .dispatch("tools.list", json!({ "source": "builtin" }))
        .await;
    let keys: Vec<&str> = listed["data"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|tool| tool["key"].as_str())
        .collect();
    assert!(keys.contains(&"vector.index"), "{keys:?}");
    assert!(keys.contains(&"download.fetch"), "{keys:?}");
    assert!(!keys.contains(&"llm.generate"));
    drop(host);
}
