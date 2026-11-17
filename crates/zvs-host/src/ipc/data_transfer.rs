use std::sync::Arc;

use zvs_contract::{
    TransferChunkInput, TransferImportInput, TransferRef, TransferSelection, TransferUploadInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle("dataTransfer.inventory", |services: Arc<Services>, _: ()| async move { services.data_transfer.inventory().await })
        .handle("dataTransfer.export", |services: Arc<Services>, input: TransferSelection| async move { services.data_transfer.export(input, services.picker.clone()).await })
        .handle("dataTransfer.beginImport", |services: Arc<Services>, input: TransferUploadInput| async move { services.data_transfer.begin(input) })
        .handle("dataTransfer.appendImport", |services: Arc<Services>, input: TransferChunkInput| async move { services.data_transfer.append(input) })
        .handle("dataTransfer.inspectImport", |services: Arc<Services>, input: TransferRef| async move { services.data_transfer.inspect(input).await })
        .handle("dataTransfer.import", |services: Arc<Services>, input: TransferImportInput| async move {
            let result = services.data_transfer.import(input).await?;
            if result.categories.iter().any(|category| category.category == zvs_contract::TransferCategory::Settings) {
                services.settings.apply_appearance(services.theme.as_ref())?;
            }
            Ok(result)
        })
        .handle("dataTransfer.disposeImport", |services: Arc<Services>, input: TransferRef| async move { services.data_transfer.dispose(input); Ok(()) });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ipc::Caller;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn only_the_studio_can_transfer_data() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        for caller in [
            Caller::Cli { connection: 1 },
            Caller::BrowserUi,
            Caller::Panel,
        ] {
            for channel in server.channels() {
                let result = server
                    .dispatch(caller, channel, serde_json::Value::Null)
                    .await;
                assert_eq!(result["error"]["code"], "PERMISSION_DENIED", "{channel}");
            }
        }
        let result = server
            .dispatch(
                Caller::Studio,
                "dataTransfer.inventory",
                serde_json::Value::Null,
            )
            .await;
        assert_eq!(result["ok"], true, "{result}");
    }
}
