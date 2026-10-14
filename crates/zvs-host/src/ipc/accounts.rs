use std::sync::Arc;

use zvs_contract::{AppError, Json};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server.handle(
        "accounts.list",
        |_services: Arc<Services>, _input: ()| async move { Ok::<Vec<Json>, AppError>(Vec::new()) },
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn accounts_list_answers_an_empty_list_until_linking_is_ported() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        assert_eq!(
            server.dispatch("accounts.list", Value::Null).await,
            json!({ "ok": true, "data": [] })
        );
    }
}
