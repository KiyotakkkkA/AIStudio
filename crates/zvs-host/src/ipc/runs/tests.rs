use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{HostEventDraft, RunId, ToolTier};

use super::register;
use crate::ipc::{IpcServer, system};
use crate::kernel::{FnNode, PermissionRequirement, register_job_nodes};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};

fn serve() -> (TestServices, IpcServer) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    system::register(&mut server);
    (host, server)
}

fn run_id(response: &Value) -> RunId {
    RunId::parse(response["data"]["id"].as_str().expect("an id")).expect("a run id")
}

#[tokio::test]
async fn every_run_channel_validates_outputs_lists_persisted_runs_and_handles_missing_ids() {
    let (host, server) = serve();
    let started = server
        .dispatch(
            "runs.start",
            json!({ "kind": "chat", "graph": { "nodes": [] } }),
        )
        .await;
    assert_eq!(started["ok"], json!(true), "{started}");
    assert!(started["data"]["streamId"].is_string());
    let id = run_id(&started);
    host.runs.wait(&id).await;

    let get = server.dispatch("runs.get", json!({ "id": id })).await;
    assert_eq!(get["data"]["status"], json!("succeeded"));
    assert_eq!(get["data"]["graph"], json!({ "nodes": [] }));
    assert_eq!(get["data"]["input"], Value::Null);

    let page = server.dispatch("runs.list", json!({})).await;
    assert_eq!(page["ok"], json!(true), "{page}");
    assert_eq!(page["data"]["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(page["data"]["counts"]["byStatus"]["succeeded"], json!(1));
    assert_eq!(page["data"]["counts"]["byKind"]["browser"], json!(0));

    let detail = server.dispatch("runs.detail", json!({ "id": id })).await;
    assert_eq!(
        detail["data"]["summary"]["progress"],
        json!({ "done": 0, "total": 0 })
    );
    assert!(
        detail["data"]["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty())
    );
    assert_eq!(
        server.dispatch("runs.steps", json!({ "id": id })).await,
        json!({ "ok": true, "data": [] })
    );
    assert_eq!(
        server.dispatch("runs.cancel", json!({ "id": id })).await,
        json!({ "ok": true })
    );
    assert_eq!(
        server.dispatch("runs.retry", json!({ "id": id })).await["error"]["code"],
        json!("CONFLICT")
    );
    assert_eq!(
        server.dispatch("runs.clearFinished", Value::Null).await,
        json!({ "ok": true, "data": { "removed": 1 } })
    );
    assert_eq!(
        server.dispatch("runs.list", json!({})).await["data"]["items"],
        json!([])
    );
    let missing = server
        .dispatch(
            "runs.get",
            json!({ "id": "0199aa11-1111-7111-8111-111111111111" }),
        )
        .await;
    assert_eq!(missing["error"]["code"], json!("NOT_FOUND"));
    assert_eq!(missing["error"]["message"], json!("Запуск не найден"));
    for (channel, payload) in [
        ("runs.get", json!({ "id": "nope" })),
        ("runs.list", json!({ "limit": 0 })),
        (
            "runs.start",
            json!({ "kind": "job", "graph": { "nodes": [] }, "concurrency": 65 }),
        ),
        ("runs.approve", json!({ "id": id, "approvalId": "nope" })),
        ("runs.deny", json!({ "id": id })),
    ] {
        assert_eq!(
            server.dispatch(channel, payload).await["error"]["code"],
            json!("VALIDATION_FAILED"),
            "{channel}"
        );
    }
}

#[tokio::test]
async fn approvals_travel_over_ipc_and_stale_decisions_conflict() {
    let (host, server) = serve();
    host.nodes
        .register(FnNode::new(
            "tool",
            PermissionRequirement::tool("mail.send"),
            true,
            |_context, _input: Value| async { Ok(json!("sent")) },
        ))
        .expect("registers");
    let started = server
        .dispatch(
            "runs.start",
            json!({ "kind": "job", "graph": { "nodes": [{ "id": "a", "type": "tool" }] } }),
        )
        .await;
    let id = run_id(&started);
    let approval = loop {
        let page = server.dispatch("runs.list", json!({ "live": true })).await;
        if let Some(approval) = page["data"]["items"][0].get("approval") {
            break approval.clone();
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    };
    assert_eq!(approval["subject"], json!("mail.send"));
    assert_eq!(
        server
            .dispatch(
                "runs.approve",
                json!({ "id": id, "approvalId": approval["id"] })
            )
            .await,
        json!({ "ok": true })
    );
    host.runs.wait(&id).await;
    assert_eq!(
        server.dispatch("runs.get", json!({ "id": id })).await["data"]["status"],
        json!("succeeded")
    );
    assert_eq!(
        server
            .dispatch(
                "runs.deny",
                json!({ "id": id, "approvalId": approval["id"] })
            )
            .await["error"]["code"],
        json!("CONFLICT")
    );
}

#[tokio::test]
async fn the_demo_job_runs_one_sidecar_job_through_the_kernel_with_progress() {
    let (host, server) = serve();
    register_job_nodes(&host.nodes).expect("registers");
    host.runs
        .permissions()
        .grant("job.run", "global", ToolTier::Auto, "user")
        .expect("grants");
    let started = server
        .dispatch("system.demoJob", json!({ "steps": 3, "intervalMs": 10 }))
        .await;
    assert_eq!(started["ok"], json!(true), "{started}");
    let id = run_id(&started);
    host.runs.wait(&id).await;
    let run = server.dispatch("runs.get", json!({ "id": id })).await;
    assert_eq!(run["data"]["status"], json!("succeeded"));
    assert_eq!(run["data"]["kind"], json!("job"));
    assert_eq!(run["data"]["title"], json!("Демонстрационная задача"));
    assert_eq!(run["data"]["concurrency"], json!(1));
    assert_eq!(
        host.jobs.calls(),
        [(
            "job.sleep".to_owned(),
            json!({ "steps": 3, "intervalMs": 10 })
        )]
    );
    let steps = server.dispatch("runs.steps", json!({ "id": id })).await;
    assert_eq!(steps["data"][0]["output"], json!({ "slept": 3 }));
    let progress: Vec<(u64, u64)> = host
        .run_events
        .events()
        .into_iter()
        .filter_map(|event| match event.body {
            HostEventDraft::Progress { done, total } => Some((done, total)),
            _ => None,
        })
        .collect();
    for expected in [(1, 3), (2, 3), (3, 3)] {
        assert!(progress.contains(&expected), "{progress:?}");
    }
    let refused = server
        .dispatch("system.demoJob", json!({ "steps": 0 }))
        .await;
    assert_eq!(refused["error"]["code"], json!("VALIDATION_FAILED"));
}

#[tokio::test]
async fn without_a_grant_the_demo_job_waits_for_approval() {
    let (host, server) = serve();
    register_job_nodes(&host.nodes).expect("registers");
    let id = run_id(
        &server
            .dispatch("system.demoJob", json!({ "steps": 1 }))
            .await,
    );
    loop {
        let run = server.dispatch("runs.get", json!({ "id": id })).await;
        if run["data"]["status"] == json!("blocked") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(host.jobs.calls().is_empty());
    assert_eq!(
        server.dispatch("runs.cancel", json!({ "id": id })).await,
        json!({ "ok": true })
    );
    assert_eq!(
        server.dispatch("runs.get", json!({ "id": id })).await["data"]["status"],
        json!("cancelled")
    );
}
