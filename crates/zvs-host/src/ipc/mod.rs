mod server;
pub mod system;

pub use server::{Coverage, INTERNAL_MESSAGE, IpcServer};

pub fn register(server: &mut IpcServer) {
    system::register(server);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::{Services, SystemService};

    fn registered() -> IpcServer {
        let services = Arc::new(Services {
            system: SystemService::new(
                Arc::new(FakeClock::default()),
                EventBus::default(),
                Duration::ZERO,
            ),
        });
        let mut server = IpcServer::new(services, silent_logger());
        register(&mut server);
        server
    }

    #[test]
    fn the_ported_channels_are_registered() {
        let server = registered();
        assert_eq!(server.channels(), ["system.demoStream", "system.ping"]);
    }

    #[test]
    #[ignore = "becomes strict at TASK_076, when every channel is ported"]
    fn every_contract_channel_is_served() {
        let coverage = registered().coverage();
        assert!(
            coverage.is_complete(),
            "{} channels answer NOT_IMPLEMENTED: {:?}",
            coverage.missing.len(),
            coverage.missing
        );
    }
}
