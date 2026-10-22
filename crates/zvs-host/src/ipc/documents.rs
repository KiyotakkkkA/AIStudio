use std::sync::Arc;

use zvs_contract::{DocumentHandle, DocumentPageInput, DocumentRef};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "documents.open",
            |services: Arc<Services>, input: DocumentRef| async move {
                services.documents.open(input).await
            },
        )
        .handle(
            "documents.page",
            |services: Arc<Services>, input: DocumentPageInput| async move {
                services.documents.page(&input.id, input.index).await
            },
        )
        .handle(
            "documents.close",
            |services: Arc<Services>, input: DocumentHandle| async move {
                services.documents.close(&input.id)
            },
        )
        .handle(
            "documents.list",
            |services: Arc<Services>, (): ()| async move { services.documents.list() },
        )
        .handle(
            "documents.reveal",
            |services: Arc<Services>, input: DocumentHandle| async move {
                services.documents.reveal(&input.id)
            },
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
    use crate::testing::{TestServices, test_services};

    fn server() -> (TestServices, IpcServer) {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        (host, server)
    }

    async fn call(server: &IpcServer, channel: &str, payload: Value) -> Value {
        let response = server.dispatch(channel, payload).await;
        assert_eq!(response["ok"], json!(true), "{channel}: {response}");
        response["data"].clone()
    }

    async fn failure(server: &IpcServer, channel: &str, payload: Value) -> Value {
        let response = server.dispatch(channel, payload).await;
        assert_eq!(response["ok"], json!(false), "{channel}: {response}");
        response["error"]["code"].clone()
    }

    #[tokio::test]
    async fn a_document_opens_pages_lists_and_closes_through_the_channels() {
        let (_host, server) = server();
        let folder = tempfile::tempdir().expect("a temporary folder");
        let path = folder.path().join("notes.md");
        std::fs::write(&path, "# План\n").expect("writes");

        let opened = call(
            &server,
            "documents.open",
            json!({ "source": "path", "path": path.to_string_lossy() }),
        )
        .await;
        assert_eq!(opened["kind"], json!("markdown"));
        assert_eq!(opened["open"], json!(true));
        let id = opened["id"].clone();

        let page = call(&server, "documents.page", json!({ "id": id, "index": 0 })).await;
        assert_eq!(page["content"], json!("text"));
        assert_eq!(page["text"], json!("# План\n"));
        assert_eq!(page["untrusted"], json!(true));

        let listed = call(&server, "documents.list", Value::Null).await;
        assert_eq!(listed[0]["id"], id);
        call(&server, "documents.close", json!({ "id": id })).await;
        let listed = call(&server, "documents.list", Value::Null).await;
        assert_eq!(listed[0]["open"], json!(false));
        assert_eq!(
            failure(&server, "documents.page", json!({ "id": id, "index": 0 })).await,
            json!("NOT_FOUND")
        );
        call(&server, "documents.reveal", json!({ "id": id })).await;
    }

    #[tokio::test]
    async fn an_unsupported_file_is_refused_with_its_code() {
        let (_host, server) = server();
        let folder = tempfile::tempdir().expect("a temporary folder");
        let path = folder.path().join("letter.docx");
        std::fs::write(&path, b"PK\x03\x04word/document.xml").expect("writes");
        assert_eq!(
            failure(
                &server,
                "documents.open",
                json!({ "source": "path", "path": path.to_string_lossy() }),
            )
            .await,
            json!("UNSUPPORTED_FORMAT")
        );
        assert_eq!(
            failure(
                &server,
                "documents.open",
                json!({ "source": "url", "url": "x" })
            )
            .await,
            json!("VALIDATION_FAILED")
        );
    }
}
