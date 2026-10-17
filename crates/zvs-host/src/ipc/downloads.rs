use std::sync::Arc;

use zvs_contract::{
    AppError, CatalogueFilter, DiskUsageDto, DownloadListFilter, DownloadRef, DownloadsDiskInput,
    DownloadsRemoveOutput, PrioritiseDownloadInput, StartDownloadInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "downloads.list",
            |services: Arc<Services>, filter: DownloadListFilter| async move {
                services.downloads.list(&filter)
            },
        )
        .handle(
            "downloads.start",
            |services: Arc<Services>, input: StartDownloadInput| async move {
                services.downloads.start(&input).await
            },
        )
        .handle(
            "downloads.pause",
            |services: Arc<Services>, input: DownloadRef| async move {
                services.downloads.pause(&input.id.to_string()).await
            },
        )
        .handle(
            "downloads.resume",
            |services: Arc<Services>, input: DownloadRef| async move {
                services.downloads.resume(&input.id.to_string())
            },
        )
        .handle(
            "downloads.cancel",
            |services: Arc<Services>, input: DownloadRef| async move {
                services.downloads.cancel(&input.id.to_string()).await
            },
        )
        .handle(
            "downloads.prioritise",
            |services: Arc<Services>, input: PrioritiseDownloadInput| async move {
                services
                    .downloads
                    .prioritise(&input.id.to_string(), input.priority)
            },
        )
        .handle(
            "downloads.remove",
            |services: Arc<Services>, input: DownloadRef| async move {
                services.downloads.remove(&input.id.to_string()).await?;
                Ok::<DownloadsRemoveOutput, AppError>(DownloadsRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "downloads.catalogue",
            |services: Arc<Services>, filter: CatalogueFilter| async move {
                services.downloads.catalogue(&filter).await
            },
        )
        .handle(
            "downloads.disk",
            |services: Arc<Services>, input: DownloadsDiskInput| async move {
                Ok::<DiskUsageDto, AppError>(services.downloads.disk(input.refresh()).await)
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};
    use zvs_contract::{CatalogueItemDto, DownloadDto, DownloadStatus};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn every_download_channel_reaches_the_service() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        host.download_jobs
            .serve("https://example.test/alpha", b"0123456789", None);
        host.downloads
            .catalogue_source()
            .offer(crate::downloads::CatalogueItem {
                size_bytes: 10,
                url: "https://example.test/alpha".to_owned(),
                file_name: "alpha.bin".to_owned(),
                ..crate::downloads::CatalogueItem::new(
                    "curated:model:alpha",
                    zvs_contract::DownloadItemKind::Model,
                    zvs_contract::CatalogueSource::Curated,
                    "alpha",
                )
            });
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);

        let catalogue = server.dispatch("downloads.catalogue", json!({})).await;
        assert_eq!(catalogue["ok"], json!(true), "{catalogue}");
        let rows: Vec<CatalogueItemDto> =
            serde_json::from_value(catalogue["data"].clone()).expect("catalogue rows");
        assert!(
            rows.iter()
                .any(|row| row.item_ref == "curated:embedding:bge-m3-f16")
        );

        let started = server
            .dispatch("downloads.start", json!({ "ref": "curated:model:alpha" }))
            .await;
        assert_eq!(started["ok"], json!(true), "{started}");
        let id = started["data"]["id"].as_str().expect("an id").to_owned();
        let mut finished = Value::Null;
        for _ in 0..600 {
            let listed = server.dispatch("downloads.list", json!({})).await;
            let rows: Vec<DownloadDto> =
                serde_json::from_value(listed["data"].clone()).expect("rows");
            if rows
                .first()
                .is_some_and(|row| row.status == DownloadStatus::Succeeded)
            {
                finished = listed;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            finished["data"][0]["status"],
            json!("succeeded"),
            "{finished}"
        );

        let disk = server
            .dispatch("downloads.disk", json!({ "refresh": true }))
            .await;
        assert_eq!(disk["ok"], json!(true), "{disk}");
        assert_eq!(
            disk["data"]["root"],
            json!(host.downloads_dir().to_string_lossy())
        );
        assert_eq!(
            server
                .dispatch("downloads.pause", json!({ "id": id }))
                .await["error"]["code"],
            json!("CONFLICT")
        );
        assert_eq!(
            server
                .dispatch("downloads.prioritise", json!({ "id": id, "priority": 9 }))
                .await["error"]["code"],
            json!("CONFLICT")
        );
        assert_eq!(
            server
                .dispatch("downloads.cancel", json!({ "id": id }))
                .await["data"]["status"],
            json!("succeeded")
        );
        assert_eq!(
            server
                .dispatch("downloads.resume", json!({ "id": id }))
                .await["error"]["code"],
            json!("CONFLICT")
        );
        assert_eq!(
            server
                .dispatch("downloads.remove", json!({ "id": id }))
                .await,
            json!({ "ok": true, "data": { "id": id, "removed": true } })
        );
        assert_eq!(
            server.dispatch("downloads.list", json!({})).await,
            json!({ "ok": true, "data": [] })
        );
        assert_eq!(
            server
                .dispatch("downloads.start", json!({ "ref": "ab" }))
                .await["error"]["code"],
            json!("VALIDATION_FAILED")
        );
    }
}
