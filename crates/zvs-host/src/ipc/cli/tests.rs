use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::FolderGrantScope;

use super::*;
use crate::cli::{CLI_INSTALL_FAILED, CLI_NOT_BUNDLED};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::platform::shell::TRAY_SETTING_KEY;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};

fn registered() -> (TestServices, IpcServer) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    (host, server)
}

fn data(envelope: Value) -> Value {
    assert_eq!(envelope["ok"], json!(true), "{envelope}");
    envelope["data"].clone()
}

#[tokio::test]
async fn opening_a_folder_reports_whether_an_always_grant_covers_it() {
    let (host, server) = registered();
    let outside = tempfile::tempdir().expect("dir");
    let project = outside.path().join("project");
    std::fs::create_dir_all(project.join("src")).expect("creates");
    let cwd = project.join("src").to_string_lossy().into_owned();

    let session = data(server.dispatch("cli.open", json!({ "cwd": cwd })).await);
    assert_eq!(session["access"], json!("missing"));
    let folder = session["folder"].as_str().expect("folder").to_owned();

    let services = host.shared();
    services
        .folders
        .grant(&project, FolderGrantScope::Always, None)
        .expect("grants");
    let session = data(server.dispatch("cli.open", json!({ "cwd": cwd })).await);
    assert_eq!(session["access"], json!("granted"));
    assert_eq!(session["folder"], json!(folder));
}

#[tokio::test]
async fn opening_a_missing_folder_fails() {
    let (_host, server) = registered();
    let outside = tempfile::tempdir().expect("dir");
    let cwd = outside.path().join("absent").to_string_lossy().into_owned();
    let envelope = server.dispatch("cli.open", json!({ "cwd": cwd })).await;
    assert_eq!(envelope["error"]["code"], json!("NOT_FOUND"));
}

#[tokio::test]
async fn installing_puts_the_command_on_the_path_and_turns_the_tray_on() {
    let (host, server) = registered();
    let status = data(server.dispatch("cli.status", Value::Null).await);
    assert_eq!(
        status,
        json!({ "available": true, "installed": false, "command": "aistudio" })
    );

    let status = data(server.dispatch("cli.install", Value::Null).await);
    assert_eq!(status["installed"], json!(true));
    assert_eq!(host.shell.tray(), [true]);
    let tray = host
        .shared()
        .settings
        .get(TRAY_SETTING_KEY)
        .expect("reads")
        .expect("stored");
    assert_eq!(tray.value, json!(true));

    let status = data(server.dispatch("cli.uninstall", Value::Null).await);
    assert_eq!(status["installed"], json!(false));
    assert_eq!(host.shell.tray(), [true]);
}

#[tokio::test]
async fn a_failed_install_leaves_the_tray_alone() {
    let (host, server) = registered();
    host.cli.failing.store(true, Ordering::SeqCst);
    let envelope = server.dispatch("cli.install", Value::Null).await;
    assert_eq!(envelope["error"]["code"], json!("NATIVE_ERROR"));
    assert_eq!(envelope["error"]["message"], json!(CLI_INSTALL_FAILED));
    assert!(host.shell.tray().is_empty());
}

#[tokio::test]
async fn a_build_without_the_cli_cannot_install_it() {
    let (host, server) = registered();
    host.cli.available.store(false, Ordering::SeqCst);
    host.cli.installed.store(true, Ordering::SeqCst);
    let status = data(server.dispatch("cli.status", Value::Null).await);
    assert_eq!(status["installed"], json!(false));
    let envelope = server.dispatch("cli.install", Value::Null).await;
    assert_eq!(envelope["error"]["message"], json!(CLI_NOT_BUNDLED));
}

#[tokio::test]
async fn the_cli_theme_defaults_to_ember_and_keeps_a_saved_choice() {
    let (_host, server) = registered();
    let theme = data(server.dispatch("cli.theme.get", Value::Null).await);
    assert_eq!(theme, json!({ "theme": "ember" }));

    let saved = data(
        server
            .dispatch("cli.theme.set", json!({ "theme": "frost" }))
            .await,
    );
    assert_eq!(saved, json!({ "theme": "frost" }));
    let theme = data(server.dispatch("cli.theme.get", Value::Null).await);
    assert_eq!(theme, json!({ "theme": "frost" }));

    let envelope = server
        .dispatch("cli.theme.set", json!({ "theme": "neon" }))
        .await;
    assert_eq!(envelope["error"]["code"], json!("VALIDATION_FAILED"));
}

#[tokio::test]
async fn the_console_adds_an_mcp_server_with_literal_values() {
    let (host, server) = registered();
    let created = data(
        server
            .dispatch(
                "cli.mcp.create",
                json!({
                    "name": "Docs",
                    "transport": "http",
                    "url": "https://mcp.example.com/mcp",
                    "headers": { "X-Team": { "kind": "literal", "value": "core" } }
                }),
            )
            .await,
    );
    assert_eq!(created["name"], json!("Docs"));
    assert_eq!(created["transport"], json!("http"));
    assert_eq!(created["status"], json!("stopped"));
    let listed = host.shared().mcp.list().expect("lists");
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn the_console_cannot_point_a_server_at_a_stored_secret() {
    let (host, server) = registered();
    let envelope = server
        .dispatch(
            "cli.mcp.create",
            json!({
                "name": "Leak",
                "transport": "http",
                "url": "https://attacker.example/mcp",
                "headers": { "Authorization": {
                    "kind": "secret", "secretId": "0199aa11-3333-7111-8111-111111111111"
                } }
            }),
        )
        .await;
    assert_eq!(envelope["error"]["code"], json!("PERMISSION_DENIED"));
    assert_eq!(
        envelope["error"]["message"],
        json!(crate::cli::CLI_MCP_SECRET_REFUSED)
    );
    assert!(host.shared().mcp.list().expect("lists").is_empty());
}

fn seed_run(
    host: &TestServices,
    kind: zvs_contract::RunKind,
    subject: Option<&str>,
    parent: Option<&str>,
) -> String {
    let id = crate::platform::ids::new_id().hyphenated().to_string();
    host.database
        .with_connection(|connection| {
            crate::data::Repositories::new(connection).runs.create(
                &crate::data::repositories::RunDraft {
                    id: id.clone(),
                    kind,
                    subject_id: subject.map(str::to_owned),
                    title: None,
                    retry_of_id: None,
                    parent_run_id: parent.map(str::to_owned),
                    status: zvs_contract::RunStatus::Running,
                    graph: zvs_contract::RunGraph::new(Vec::new()),
                    input: Value::Null,
                    stream_id: crate::platform::ids::new_id().hyphenated().to_string(),
                    concurrency: 1,
                    created_at: 1,
                    started_at: None,
                },
            )?;
            Ok(())
        })
        .expect("seeds a run");
    id
}

fn seed_conversation(host: &TestServices) -> String {
    let provider =
        host.database
            .with_connection(|connection| {
                let repositories = crate::data::Repositories::new(connection);
                let provider = repositories.providers.create(
                    &crate::data::repositories::ProviderDraft::new(
                        zvs_contract::ProviderKind::Ollama,
                        "Fake",
                        "http://localhost:11434",
                        vec![zvs_contract::ProviderCapability::Text],
                        1,
                    ),
                )?;
                repositories.models.replace_for_provider(
                    &provider.id,
                    &[crate::data::repositories::ModelDraft::new(
                        "fake-model",
                        "Fake",
                        1,
                    )],
                    1,
                )?;
                Ok(provider.id)
            })
            .expect("seeds a provider");
    host.shared()
        .chat
        .create(&zvs_contract::CreateConversationInput::new(
            zvs_contract::ProviderId::parse(&provider).expect("a provider id"),
            "fake-model".to_owned(),
        ))
        .expect("creates")
        .id
        .to_string()
}

#[tokio::test]
async fn the_console_stops_only_a_subagent_of_a_reachable_dialog() {
    let (host, server) = registered();
    let conversation = seed_conversation(&host);
    let turn = seed_run(
        &host,
        zvs_contract::RunKind::Chat,
        Some(&conversation),
        None,
    );
    let child = seed_run(
        &host,
        zvs_contract::RunKind::Subagent,
        Some(&conversation),
        Some(&turn),
    );
    let job = seed_run(&host, zvs_contract::RunKind::Job, None, None);
    let stray = seed_run(
        &host,
        zvs_contract::RunKind::Subagent,
        Some(&conversation),
        Some(&job),
    );
    let gone = seed_run(&host, zvs_contract::RunKind::Chat, Some("missing"), None);
    let orphan = seed_run(
        &host,
        zvs_contract::RunKind::Subagent,
        Some("missing"),
        Some(&gone),
    );

    for refused in [
        &turn,
        &job,
        &stray,
        &orphan,
        &crate::platform::ids::new_id().hyphenated().to_string(),
    ] {
        let envelope = server
            .dispatch("cli.subagents.cancel", json!({ "runId": refused }))
            .await;
        assert_eq!(
            envelope["error"]["code"],
            json!("PERMISSION_DENIED"),
            "{refused}"
        );
        assert_eq!(
            envelope["error"]["message"],
            json!(crate::cli::CLI_NOT_A_SUBAGENT)
        );
    }
    data(
        server
            .dispatch("cli.subagents.cancel", json!({ "runId": child }))
            .await,
    );
}
