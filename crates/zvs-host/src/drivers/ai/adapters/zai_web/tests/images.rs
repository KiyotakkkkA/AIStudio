use super::*;

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
async fn multipart_upload_attaches_the_file_to_its_message_without_changing_the_signature() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    server.reply(
        &route(zvs_zai_web::images::UPLOAD_PATH),
        Reply::json(fixture("zaiImageUploaded.json")),
    );
    let mut request = prompt();
    request.messages[0].images.push(image());
    adapter(&server).generate(&request, &token()).await.unwrap();
    let upload = server
        .calls()
        .into_iter()
        .find(|call| call.path == route(zvs_zai_web::images::UPLOAD_PATH))
        .unwrap();
    assert!(
        upload
            .header("content-type")
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );
    assert!(
        upload
            .body
            .windows(image().bytes.len())
            .any(|part| part == &*image().bytes)
    );
    let body = server.last_body();
    assert_eq!(
        body["messages"][0]["content"][1]["image_url"]["url"],
        "fixture-image-id"
    );
    assert_eq!(body["files"][0]["id"], "fixture-image-id");
    assert_eq!(body["signature_prompt"], request.messages[0].content);
}

#[tokio::test]
async fn failed_uploads_and_malformed_file_ids_abort_before_completion() {
    for reply in [
        Reply::json(json!({"error":"upload failed"})).with_status(503),
        Reply::json(json!({"id":"../escape"})),
    ] {
        let server = zai_server(fixture_frames("zaiStream.sse")).await;
        server.reply(&route(zvs_zai_web::images::UPLOAD_PATH), reply);
        let mut request = prompt();
        request.messages[0].images.push(image());
        assert!(adapter(&server).generate(&request, &token()).await.is_err());
        assert!(completions(&server).is_empty());
    }
}
