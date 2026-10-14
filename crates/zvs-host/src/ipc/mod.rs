mod server;
pub mod settings;
pub mod system;

pub use server::{Coverage, INTERNAL_MESSAGE, IpcServer};

pub fn register(server: &mut IpcServer) {
    system::register(server);
    settings::register(server);
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
                "settings.get",
                "settings.set",
                "system.demoStream",
                "system.ping"
            ]
        );
    }

    #[test]
    #[ignore = "becomes strict at TASK_076, when every channel is ported"]
    fn every_contract_channel_is_served() {
        let (_host, server) = registered();
        let coverage = server.coverage();
        assert!(
            coverage.is_complete(),
            "{} channels answer NOT_IMPLEMENTED: {:?}",
            coverage.missing.len(),
            coverage.missing
        );
    }
}
