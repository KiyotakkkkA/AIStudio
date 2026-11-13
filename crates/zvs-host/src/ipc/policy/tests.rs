use std::sync::Arc;
use std::time::Duration;

use zvs_contract::CHANNELS;

use super::*;
use crate::ipc::{IpcServer, register};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::test_services;

const CONSOLE_CHANNELS: &[&str] = &[
    "assistant.get",
    "assistant.set",
    "chat.answer",
    "chat.attach",
    "chat.bindFolder",
    "chat.cancel",
    "chat.compact",
    "chat.compactions.undo",
    "chat.context",
    "chat.conversations.create",
    "chat.conversations.get",
    "chat.conversations.list",
    "chat.conversations.updateStores",
    "chat.goal.continue",
    "chat.goal.stop",
    "chat.queue.add",
    "chat.queue.edit",
    "chat.queue.flush",
    "chat.queue.list",
    "chat.queue.remove",
    "chat.send",
    "chat.skip",
    "cli.accounts.cancelLink",
    "cli.accounts.link",
    "cli.images.import",
    "cli.mcp.create",
    "cli.open",
    "cli.subagents.cancel",
    "execPolicy.allowRule",
    "execPolicy.get",
    "execPolicy.rules",
    "execPolicy.set",
    "folders.answer",
    "mcp.cancelSignIn",
    "mcp.get",
    "mcp.list",
    "mcp.signIn",
    "mcp.start",
    "mcp.stop",
    "providers.get",
    "providers.list",
    "providers.usage",
    "runs.approve",
    "runs.deny",
    "shells.kill",
    "shells.list",
    "shells.output",
    "skills.list",
    "skills.setEnabled",
    "system.ping",
    "vectorStores.list",
];

#[test]
fn every_contract_channel_has_exactly_one_row_in_order() {
    let rows: Vec<&str> = POLICY.iter().map(|row| row.channel).collect();
    assert_eq!(rows, CHANNELS);
}

#[test]
fn every_registered_channel_has_a_row() {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let mut server = IpcServer::new(host.shared(), silent_logger());
    register(&mut server);
    for channel in server.channels() {
        assert!(policy(channel).is_some(), "{channel}");
    }
}

#[test]
fn the_console_keeps_the_channels_it_had() {
    assert_eq!(console_channels().collect::<Vec<_>>(), CONSOLE_CHANNELS);
    for studio_only in [
        "cli.install",
        "cli.uninstall",
        "cli.connections.list",
        "cli.connections.disconnect",
        "cli.consents.list",
        "cli.consents.answer",
        "secrets.get",
        "settings.set",
        "accounts.link",
        "providers.create",
        "mcp.create",
        "execPolicy.removeRule",
        "execPolicy.confirmPack",
        "runs.cancel",
        "subagents.set",
        "chat.images.pick",
    ] {
        let row = policy(studio_only).expect("a row");
        assert!(!row.callers.cli, "{studio_only}");
        assert!(row.callers.studio, "{studio_only}");
    }
}

#[test]
fn only_the_studio_and_the_console_dispatch() {
    for row in POLICY {
        assert!(row.callers.studio, "{}", row.channel);
        assert!(!row.callers.admits(Caller::BrowserUi), "{}", row.channel);
        assert!(!row.callers.admits(Caller::Panel), "{}", row.channel);
    }
}

#[test]
fn loosening_console_channels_are_named() {
    let loosening: Vec<&str> = POLICY
        .iter()
        .filter(|row| row.callers.cli && row.effect == Effect::Loosen)
        .map(|row| row.channel)
        .collect();
    assert_eq!(
        loosening,
        [
            "chat.conversations.get",
            "execPolicy.allowRule",
            "execPolicy.set",
            "folders.answer",
            "runs.approve",
        ]
    );
}

#[test]
fn console_channels_that_name_a_dialog_or_run_are_scoped() {
    for channel in [
        "chat.answer",
        "chat.skip",
        "chat.cancel",
        "runs.approve",
        "runs.deny",
        "folders.answer",
        "execPolicy.allowRule",
        "cli.subagents.cancel",
        "chat.send",
        "chat.attach",
        "chat.goal.stop",
        "chat.goal.continue",
        "chat.queue.add",
        "chat.queue.list",
        "chat.queue.flush",
        "chat.conversations.get",
        "cli.images.import",
        "shells.kill",
        "shells.list",
    ] {
        assert_ne!(
            policy(channel).expect("a row").scope,
            Scope::Any,
            "{channel}"
        );
    }
}

#[test]
fn an_unknown_channel_has_no_row() {
    assert!(policy("system.nope").is_none());
}
