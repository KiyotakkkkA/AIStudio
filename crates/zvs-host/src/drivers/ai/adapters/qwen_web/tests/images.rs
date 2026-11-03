use super::*;
use crate::drivers::ai::transport::{SharedTransport, Transport};

type Put = (String, Vec<(String, String)>, Vec<u8>);
struct ObjectRecorder {
    inner: SharedTransport,
    puts: Mutex<Vec<Put>>,
    fail: bool,
}

#[async_trait]
impl Transport for ObjectRecorder {
    fn base_url(&self) -> &str {
        self.inner.base_url()
    }
    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        self.inner.request(spec, cancel).await
    }
    async fn stream(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<crate::drivers::ai::transport::ByteStream, AppError> {
        self.inner.stream(spec, cancel).await
    }
    async fn put_object(
        &self,
        url: &str,
        headers: &[(String, String)],
        bytes: Arc<[u8]>,
        _cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        self.puts
            .lock()
            .unwrap()
            .push((url.into(), headers.to_vec(), bytes.to_vec()));
        if self.fail {
            Err(crate::drivers::ai::transport::upload::upload_failed())
        } else {
            Ok(())
        }
    }
}

fn image() -> crate::images::ImageInput {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=").unwrap();
    crate::images::ImageInput {
        attachment: zvs_contract::ImageAttachmentDto {
            id: "b222dc0d-1910-427d-b25e-291a8570e556".into(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            size_bytes: bytes.len() as u64,
        },
        bytes: Arc::from(bytes),
    }
}

#[tokio::test]
async fn uploads_pixels_before_completion_and_reuses_the_remote_chain() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    server.reply(
        &route(zvs_qwen_web::images::UPLOAD_PATH),
        Reply::json(fixture("qwenUploadGrant.json")),
    );
    let transport = Arc::new(ObjectRecorder {
        inner: transport_for(&server.url(BASE)),
        puts: Mutex::new(Vec::new()),
        fail: false,
    });
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        transport: transport.clone(),
        ..options(&server)
    });
    let mut request = prompt();
    request.messages[0].images.push(image());
    let answer = qwen.generate(&request, &token()).await.unwrap();
    let completion = server.last_body();
    assert_eq!(
        completion["messages"][0]["files"][0]["id"],
        "fixture-image-id"
    );
    assert_eq!(
        completion["messages"][0]["files"][0]["file_class"],
        "vision"
    );
    {
        let puts = transport.puts.lock().unwrap();
        assert_eq!(puts.len(), 1);
        assert_eq!(puts[0].2, &*image().bytes);
        assert!(!puts[0].0.contains('?'));
        assert!(
            puts[0].1.iter().any(
                |(key, value)| key == "authorization" && value.starts_with("OSS4-HMAC-SHA256 ")
            )
        );
        assert!(!puts[0].1.iter().any(|(key, _)| key == "cookie"));
    }
    request.messages.push(ChatMessage::assistant(answer.text));
    request.messages.push(ChatMessage::user("Follow up"));
    qwen.generate(&request, &token()).await.unwrap();
    assert_eq!(transport.puts.lock().unwrap().len(), 1);
    assert_eq!(server.last_body()["messages"][0]["files"], json!([]));
}

#[tokio::test]
async fn a_failed_object_upload_never_sends_a_text_only_completion() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    server.reply(
        &route(zvs_qwen_web::images::UPLOAD_PATH),
        Reply::json(fixture("qwenUploadGrant.json")),
    );
    let transport = Arc::new(ObjectRecorder {
        inner: transport_for(&server.url(BASE)),
        puts: Mutex::new(Vec::new()),
        fail: true,
    });
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        transport,
        ..options(&server)
    });
    let mut request = prompt();
    request.messages[0].images.push(image());
    assert_eq!(
        qwen.generate(&request, &token()).await.unwrap_err().message,
        crate::images::UPLOAD_FAILED
    );
    assert!(!server.paths().contains(&route(QWEN_COMPLETIONS_PATH)));
}

#[test]
fn image_identity_is_part_of_the_chain_key() {
    let mut first = prompt();
    first.messages[0].images.push(image());
    let mut second = first.clone();
    second.messages[0].images[0].attachment.id = "0199bb11-4444-7111-8111-000000000002".into();
    assert_ne!(request_key(&first), request_key(&second));
}
