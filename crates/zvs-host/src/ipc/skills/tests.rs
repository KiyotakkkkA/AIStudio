use std::time::Duration;

use serde_json::json;
use zvs_contract::ToolSource;

use super::*;
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
    host.tools
        .register(&ToolDescriptor {
            read_only: true,
            ..ToolDescriptor::new("mail.read", ToolSource::Builtin)
        })
        .expect("registers");
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    (host, server)
}

#[tokio::test]
async fn the_channels_create_list_test_and_remove_a_skill_in_the_contract_shape() {
    let (host, server) = registered();
    let created = server
        .dispatch(
            "skills.create",
            json!({
                "key": "inbox",
                "name": "Почта",
                "description": "Разбирает входящие письма",
                "instructions": "Прочитайте письма.",
                "allowedTools": ["mail.read"],
                "files": [{ "path": "references/rules.md", "content": "Правила" }]
            }),
        )
        .await;
    assert_eq!(created["ok"], json!(true), "{created}");
    assert_eq!(
        created["data"]["files"][0]["path"],
        json!("references/rules.md")
    );
    let id = created["data"]["id"].clone();
    let listed = server.dispatch("skills.list", json!(null)).await;
    assert_eq!(listed["data"][0]["key"], json!("inbox"), "{listed}");
    let tested = server
        .dispatch(
            "skills.test",
            json!({ "message": "разбери входящие письма" }),
        )
        .await;
    assert_eq!(
        tested["data"]["matches"][0]["triggered"],
        json!(true),
        "{tested}"
    );
    host.picker.answer(&[]);
    let exported = server.dispatch("skills.export", json!({ "id": id })).await;
    assert_eq!(exported["data"]["path"], json!(null), "{exported}");
    let rejected = server
        .dispatch(
            "skills.update",
            json!({ "id": id, "allowedTools": ["mail.send"] }),
        )
        .await;
    assert_eq!(rejected["ok"], json!(false));
    assert_eq!(rejected["error"]["code"], json!("VALIDATION_FAILED"));
    assert_eq!(
        rejected["error"]["details"]["issues"],
        json!([{ "field": "allowedTools.0", "rule": "unknownTool" }])
    );
    let removed = server.dispatch("skills.remove", json!({ "id": id })).await;
    assert_eq!(
        removed["data"],
        json!({ "id": id, "removed": true }),
        "{removed}"
    );
    let gone = server.dispatch("skills.get", json!({ "id": id })).await;
    assert_eq!(gone["error"]["code"], json!("NOT_FOUND"));
}
