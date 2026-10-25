use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};

fn fake_server() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("test")
        .join("helpers")
        .join("fakeMcpServer.mjs")
        .to_string_lossy()
        .into_owned()
}

fn registered() -> (TestServices, IpcServer) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    crate::ipc::tools::register(&mut server);
    (host, server)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pages_channels_round_trip_through_the_contract() {
    let (host, server) = registered();
    let created = server
        .dispatch(
            "mcp.create",
            json!({
                "name": "qdrant",
                "transport": "stdio",
                "command": "node",
                "args": [fake_server()],
            }),
        )
        .await;
    assert_eq!(created["ok"], json!(true), "{created}");
    let id = created["data"]["id"].clone();
    let listed = server.dispatch("mcp.list", Value::Null).await;
    assert_eq!(listed["data"].as_array().map(Vec::len), Some(1));
    let tested = server.dispatch("mcp.test", json!({ "id": id })).await;
    assert_eq!(
        (&tested["data"]["ok"], &tested["data"]["toolCount"]),
        (&json!(true), &json!(6)),
        "{tested}"
    );
    let detail = server.dispatch("mcp.get", json!({ "id": id })).await;
    assert_eq!(
        detail["data"]["capabilities"]
            .as_array()
            .map(|rows| rows.iter().filter(|row| row["kind"] == "tool").count()),
        Some(6)
    );
    let health = server.dispatch("mcp.health", json!({ "id": id })).await;
    assert_eq!(
        (&health["data"]["status"], &health["data"]["restarting"]),
        (&json!("connected"), &json!(false))
    );
    let stopped = server.dispatch("mcp.stop", json!({ "id": id })).await;
    assert_eq!(stopped["data"]["status"], json!("stopped"), "{stopped}");
    assert!(stopped["data"].get("capabilities").is_none());
    let started = server.dispatch("mcp.start", json!({ "id": id })).await;
    assert_eq!(started["data"]["status"], json!("connected"), "{started}");
    let updated = server
        .dispatch("mcp.update", json!({ "id": id, "enabled": false }))
        .await;
    assert_eq!(updated["data"]["status"], json!("disabled"), "{updated}");
    let refused = server.dispatch("mcp.start", json!({ "id": id })).await;
    assert_eq!(refused["data"]["status"], json!("disabled"), "{refused}");
    let removed = server.dispatch("mcp.remove", json!({ "id": id })).await;
    assert_eq!(
        removed,
        json!({ "ok": true, "data": { "id": id, "removed": true } })
    );
    assert_eq!(
        server.dispatch("mcp.list", Value::Null).await["data"],
        json!([])
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tier_set_through_the_tools_channel_is_what_the_registry_holds_for_an_mcp_tool() {
    let (host, server) = registered();
    let created = server
        .dispatch(
            "mcp.create",
            json!({
                "name": "qdrant",
                "transport": "stdio",
                "command": "node",
                "args": [fake_server()],
            }),
        )
        .await;
    let id = created["data"]["id"].clone();
    server.dispatch("mcp.test", json!({ "id": id })).await;
    let tools = server
        .dispatch("tools.list", json!({ "source": "mcp" }))
        .await;
    let search = tools["data"]
        .as_array()
        .and_then(|tools| tools.iter().find(|tool| tool["key"] == "qdrant.search"))
        .cloned()
        .expect("qdrant.search");
    assert_eq!(search["sourceRef"], id);
    server
        .dispatch(
            "tools.setTier",
            json!({ "id": search["id"], "tier": "off" }),
        )
        .await;
    server
        .dispatch(
            "tools.setEnabled",
            json!({ "id": search["id"], "enabled": false }),
        )
        .await;
    server.dispatch("mcp.test", json!({ "id": id })).await;
    let after = server
        .dispatch("tools.get", json!({ "id": search["id"] }))
        .await;
    assert_eq!(
        (
            &after["data"]["tier"],
            &after["data"]["enabled"],
            &after["data"]["available"]
        ),
        (&json!("off"), &json!(false), &json!(true))
    );
    server
        .dispatch(
            "tools.setTier",
            json!({ "id": search["id"], "tier": "auto" }),
        )
        .await;
    server
        .dispatch(
            "tools.setEnabled",
            json!({ "id": search["id"], "enabled": true }),
        )
        .await;
    let raised = server
        .dispatch("tools.get", json!({ "id": search["id"] }))
        .await;
    assert_eq!(
        (&raised["data"]["tier"], &raised["data"]["enabled"]),
        (&json!("auto"), &json!(true))
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_without_a_command_or_an_address_is_refused_with_the_field() {
    let (_host, server) = registered();
    let stdio = server
        .dispatch("mcp.create", json!({ "name": "x", "transport": "stdio" }))
        .await;
    assert_eq!(stdio["error"]["code"], json!("VALIDATION_FAILED"));
    assert_eq!(stdio["error"]["details"]["field"], json!("command"));
    let http = server
        .dispatch("mcp.create", json!({ "name": "y", "transport": "http" }))
        .await;
    assert_eq!(http["error"]["details"]["field"], json!("url"));
}
