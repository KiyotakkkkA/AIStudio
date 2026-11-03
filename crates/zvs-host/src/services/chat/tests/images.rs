use super::*;
use crate::artifacts::ArtifactStore;
use crate::data::repositories::ProviderPatch;
use crate::images::ImageLoader;

#[tokio::test]
async fn images_are_persisted_and_loaded_again_in_followup_history() {
    use base64::Engine;
    let root = crate::testing::temporary_directory("chat-images");
    let loader = ImageLoader::new(ArtifactStore::new(root.path().join("conversations")));
    let harness = Harness::with(HarnessOptions {
        images: Some(loader.clone()),
        context_window: 100_000,
        ..HarnessOptions::default()
    });
    harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).providers.update(
                &harness.provider_id.to_string(),
                &ProviderPatch {
                    adapter: Some(zvs_contract::AdapterFamily::QwenWeb),
                    ..ProviderPatch::default()
                },
                1,
            )?;
            Ok(())
        })
        .unwrap();
    let conversation = harness.conversation();
    let source = root.path().join("image.png");
    let bytes = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=").unwrap();
    std::fs::write(&source, &bytes).unwrap();
    let images = loader
        .import(
            &conversation.to_string(),
            &[source.to_string_lossy().into_owned()],
        )
        .unwrap();
    let handle = harness
        .chat
        .send(&ChatSendInput {
            images: images.clone(),
            ..ChatSendInput::new(conversation, "")
        })
        .unwrap();
    harness.runs.wait(&handle.id).await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let saved = harness.messages(conversation);
    assert_eq!(saved[0].images, images);
    assert_eq!(
        harness.chat.get(&conversation).unwrap().conversation.title,
        images[0].name
    );
    std::fs::remove_file(source).unwrap();
    let handle = harness.turn(conversation, "Describe again").await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let requests = harness.driver.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        let image = request
            .messages
            .iter()
            .flat_map(|message| &message.images)
            .next()
            .unwrap();
        assert_eq!(&*image.bytes, bytes);
    }
}

#[test]
fn unsupported_providers_refuse_images_before_creating_a_run_or_message() {
    let harness = Harness::new();
    let conversation = harness.conversation();
    let image = zvs_contract::ImageAttachmentDto {
        id: new_id().to_string(),
        name: "image.png".into(),
        mime_type: "image/png".into(),
        size_bytes: 68,
    };
    let error = harness
        .chat
        .send(&ChatSendInput {
            images: vec![image],
            ..ChatSendInput::new(conversation, "Describe")
        })
        .unwrap_err();
    assert_eq!(error.message, crate::images::VISION_UNSUPPORTED);
    assert!(harness.messages(conversation).is_empty());
    assert!(harness.driver.requests().is_empty());
}

#[test]
fn image_context_is_budgeted_and_old_images_are_windowed_before_loading() {
    let reference = zvs_contract::ImageAttachmentDto {
        id: new_id().to_string(),
        name: "image.png".into(),
        mime_type: "image/png".into(),
        size_bytes: 68,
    };
    let current = ChatMessage::user("").with_images(vec![reference.clone()]);
    let too_small = window_chat_history("", &[], &current, 1000, 100, &[], &[]).unwrap_err();
    assert_eq!(too_small.message, BUDGET_EXCEEDED);
    let history: Vec<_> = (0..6)
        .flat_map(|_| {
            [
                ChatMessage::user("Earlier image").with_images(vec![reference.clone()]),
                ChatMessage::assistant("Earlier answer"),
            ]
        })
        .collect();
    let prepared = window_chat_history("", &history, &current, 100_000, 1000, &[], &[]).unwrap();
    assert!(prepared.trimmed > 0);
    assert_eq!(
        prepared
            .messages
            .iter()
            .map(|message| message.images.len())
            .sum::<usize>(),
        zvs_contract::images::IMAGES_MAX
    );
    assert!(prepared.tokens_in >= crate::images::IMAGE_TOKEN_ESTIMATE * 5);
}
