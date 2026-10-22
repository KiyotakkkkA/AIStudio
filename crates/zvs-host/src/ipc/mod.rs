pub mod accounts;
pub mod chat;
pub mod downloads;
pub mod graph_relations;
pub mod mcp;
pub mod providers;
pub mod runs;
pub mod runtimes;
pub mod secrets;
mod server;
pub mod settings;
pub mod skills;
pub mod system;
pub mod toolchain;
pub mod tools;
pub mod vector_stores;

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
    skills::register(server);
    toolchain::register(server);
    chat::register(server);
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
                "chat.cancel",
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
                "chat.conversations.updateTools",
                "chat.evaluate",
                "chat.send",
                "downloads.cancel",
                "downloads.catalogue",
                "downloads.disk",
                "downloads.list",
                "downloads.pause",
                "downloads.prioritise",
                "downloads.remove",
                "downloads.resume",
                "downloads.start",
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
                "secrets.create",
                "secrets.get",
                "secrets.list",
                "secrets.remove",
                "secrets.types",
                "secrets.update",
                "settings.get",
                "settings.set",
                "skills.create",
                "skills.export",
                "skills.get",
                "skills.import",
                "skills.list",
                "skills.remove",
                "skills.run",
                "skills.test",
                "skills.update",
                "system.demoJob",
                "system.demoStream",
                "system.device",
                "system.nativePing",
                "system.ping",
                "system.resources",
                "toolchain.install",
                "toolchain.list",
                "toolchain.remove",
                "toolchain.scan",
                "toolchain.select",
                "toolchain.update",
                "tools.defaults",
                "tools.get",
                "tools.invoke",
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
                "vectorStores.update"
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
