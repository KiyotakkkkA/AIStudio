pub mod accounts;
pub mod agentic;
pub mod artifacts;
pub mod chat;
pub mod documents;
pub mod downloads;
pub mod folders;
pub mod graph_relations;
pub mod mcp;
pub mod memory;
pub mod notifications;
pub mod providers;
pub mod runs;
pub mod runtimes;
pub mod scenarios;
pub mod secrets;
mod server;
pub mod settings;
pub mod sites;
pub mod skills;
pub mod storage;
pub mod system;
pub mod terminal;
pub mod toolchain;
pub mod tools;
pub mod vector_stores;
pub mod workspace;

pub use server::{Coverage, INTERNAL_MESSAGE, IpcServer};

pub fn register(server: &mut IpcServer) {
    system::register(server);
    settings::register(server);
    secrets::register(server);
    providers::register(server);
    accounts::register(server);
    vector_stores::register(server);
    graph_relations::register(server);
    runs::register(server);
    downloads::register(server);
    runtimes::register(server);
    tools::register(server);
    mcp::register(server);
    notifications::register(server);
    skills::register(server);
    sites::register(server);
    scenarios::register(server);
    toolchain::register(server);
    chat::register(server);
    folders::register(server);
    memory::register(server);
    agentic::register(server);
    workspace::register(server);
    terminal::register(server);
    documents::register(server);
    artifacts::register(server);
    storage::register(server);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::{TestServices, test_services};

    fn registered() -> (TestServices, IpcServer) {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server = IpcServer::new(host.shared(), silent_logger());
        register(&mut server);
        (host, server)
    }

    #[test]
    fn the_ported_channels_are_registered() {
        let (_host, server) = registered();
        assert_eq!(
            server.channels(),
            [
                "accounts.cancelLink",
                "accounts.link",
                "accounts.list",
                "accounts.refresh",
                "accounts.unlink",
                "agentic.cancel",
                "agentic.create",
                "agentic.extend",
                "agentic.get",
                "agentic.instruct",
                "agentic.list",
                "agentic.remove",
                "agentic.setMode",
                "agentic.updatePlan",
                "artifacts.application",
                "artifacts.browsers",
                "artifacts.open",
                "artifacts.openInApp",
                "artifacts.openInBrowser",
                "artifacts.reveal",
                "artifacts.save",
                "chat.answer",
                "chat.attach",
                "chat.bindFolder",
                "chat.cancel",
                "chat.commands",
                "chat.context",
                "chat.conversations.create",
                "chat.conversations.get",
                "chat.conversations.list",
                "chat.conversations.remove",
                "chat.conversations.rename",
                "chat.conversations.truncate",
                "chat.conversations.updateRelations",
                "chat.conversations.updateRetrieval",
                "chat.conversations.updateStores",
                "chat.evaluate",
                "chat.pickFolder",
                "chat.send",
                "chat.skip",
                "chat.unbindFolder",
                "documents.close",
                "documents.list",
                "documents.open",
                "documents.page",
                "documents.reveal",
                "documents.save",
                "downloads.cancel",
                "downloads.catalogue",
                "downloads.disk",
                "downloads.list",
                "downloads.pause",
                "downloads.prioritise",
                "downloads.remove",
                "downloads.resume",
                "downloads.start",
                "folders.answer",
                "folders.list",
                "folders.revoke",
                "graphRelations.addEdge",
                "graphRelations.kHopSubgraph",
                "graphRelations.list",
                "graphRelations.listEdges",
                "graphRelations.neighbors",
                "graphRelations.reconcile",
                "graphRelations.removeEdge",
                "graphRelations.shortestPath",
                "mcp.cancelSignIn",
                "mcp.create",
                "mcp.get",
                "mcp.health",
                "mcp.list",
                "mcp.remove",
                "mcp.signIn",
                "mcp.signOut",
                "mcp.start",
                "mcp.stop",
                "mcp.test",
                "mcp.update",
                "memory.clear",
                "memory.create",
                "memory.list",
                "memory.remove",
                "memory.update",
                "notifications.clear",
                "notifications.pending",
                "providers.adapters",
                "providers.create",
                "providers.get",
                "providers.list",
                "providers.probe",
                "providers.refreshAll",
                "providers.remove",
                "providers.setDefaultModel",
                "providers.update",
                "runs.approve",
                "runs.cancel",
                "runs.clearFinished",
                "runs.deny",
                "runs.detail",
                "runs.get",
                "runs.list",
                "runs.retry",
                "runs.start",
                "runs.steps",
                "runtimes.install",
                "runtimes.list",
                "runtimes.remove",
                "runtimes.stop",
                "scenarios.activate",
                "scenarios.catalogue",
                "scenarios.create",
                "scenarios.discardDraft",
                "scenarios.draft",
                "scenarios.get",
                "scenarios.list",
                "scenarios.remove",
                "scenarios.retryStep",
                "scenarios.run",
                "scenarios.runNode",
                "scenarios.saveDraft",
                "scenarios.update",
                "scenarios.validate",
                "scenarios.versions",
                "secrets.create",
                "secrets.get",
                "secrets.list",
                "secrets.remove",
                "secrets.types",
                "secrets.update",
                "settings.get",
                "settings.set",
                "sites.icon",
                "sites.open",
                "skills.create",
                "skills.duplicate",
                "skills.export",
                "skills.get",
                "skills.import",
                "skills.list",
                "skills.readFile",
                "skills.remove",
                "skills.setEnabled",
                "skills.update",
                "skills.updateAllBundled",
                "skills.updateBundled",
                "storage.clearCache",
                "storage.usage",
                "system.demoJob",
                "system.demoStream",
                "system.device",
                "system.nativePing",
                "system.ping",
                "system.resources",
                "terminal.attach",
                "terminal.clear",
                "terminal.close",
                "terminal.create",
                "terminal.list",
                "terminal.rename",
                "terminal.resize",
                "terminal.restart",
                "terminal.shells",
                "terminal.write",
                "toolchain.install",
                "toolchain.list",
                "toolchain.remove",
                "toolchain.scan",
                "toolchain.select",
                "toolchain.update",
                "tools.defaults",
                "tools.get",
                "tools.list",
                "tools.setDefaults",
                "tools.setEnabled",
                "tools.setTier",
                "tools.usage",
                "vectorStores.chunks",
                "vectorStores.clear",
                "vectorStores.create",
                "vectorStores.documents.list",
                "vectorStores.documents.remove",
                "vectorStores.eval.add",
                "vectorStores.eval.list",
                "vectorStores.eval.remove",
                "vectorStores.eval.run",
                "vectorStores.get",
                "vectorStores.index",
                "vectorStores.list",
                "vectorStores.reconcile",
                "vectorStores.remove",
                "vectorStores.search",
                "vectorStores.searchTimed",
                "vectorStores.sources.add",
                "vectorStores.sources.list",
                "vectorStores.sources.pick",
                "vectorStores.sources.remove",
                "vectorStores.update",
                "workspace.list",
                "workspace.read",
                "workspace.unwatch",
                "workspace.watch",
            ]
        );
    }

    #[test]
    fn every_contract_channel_is_served() {
        let (_host, server) = registered();
        let coverage = server.coverage();
        assert!(
            coverage.is_complete(),
            "{} channels answer NOT_IMPLEMENTED: {:?}",
            coverage.missing.len(),
            coverage.missing
        );
        assert_eq!(server.channels(), zvs_contract::CHANNELS);
        assert!(server.finish().is_complete());
    }
}
