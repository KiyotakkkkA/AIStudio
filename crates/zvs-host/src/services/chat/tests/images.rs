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
    harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).providers.update(
                &harness.provider_id.to_string(),
                &ProviderPatch {
                    adapter: Some(zvs_contract::AdapterFamily::OpenaiCompatible),
                    ..ProviderPatch::default()
                },
                2,
            )?;
            Ok(())
        })
        .unwrap();
    let handle = harness.turn(conversation, "Continue without vision").await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let requests = harness.driver.requests();
    let request = requests.last().unwrap();
    assert!(
        request
            .messages
            .iter()
            .all(|message| message.images.is_empty())
    );
    assert!(
        request
            .messages
            .iter()
            .any(|message| message.content.contains(&images[0].name))
    );
    assert_eq!(harness.messages(conversation)[0].images, images);
}

#[tokio::test]
async fn file_tool_pixels_reach_the_next_round_and_history_and_survive_provider_switches() {
    use super::tool_loop::{Scripted, call};
    use crate::kernel::{NodeRegistry, register_core_nodes};
    use crate::tools::{ToolRegistry, ToolRegistryOptions};
    let root = crate::testing::temporary_directory("chat-tool-images");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let loader = ImageLoader::new(store.clone());
    let database = crate::testing::temporary_database();
    let tools = ToolRegistry::new(ToolRegistryOptions::new(database.shared()));
    let registry = Arc::new(NodeRegistry::new());
    register_core_nodes(&registry).unwrap();
    let driver = Scripted::new();
    let harness = Harness::with(HarnessOptions {
        database: Some(database.shared()),
        tools: Some(tools.clone()),
        registry: Some(registry.clone()),
        providers: Some(Arc::new(OneDriver(driver.clone()))),
        images: Some(loader.clone()),
        context_window: 100_000,
        ..HarnessOptions::default()
    });
    let folders = crate::workspace::FolderAccessService::new(
        database.shared(),
        harness.runs.approvals(),
        crate::workspace::ForbiddenFolders::new(&[], &[], Arc::new(Vec::new)),
        Arc::new(crate::testing::FakeClock::default()),
    );
    crate::files::register_file_nodes(
        &registry,
        &folders,
        crate::files::FileToolOptions {
            roots: crate::files::VirtualRoots::new(Some(store.clone()), None),
            ..crate::files::FileToolOptions::new(store.root().to_path_buf())
        },
    )
    .unwrap();
    crate::tools::register_kernel_tools(&registry, &tools).unwrap();
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
    let work = crate::chat_files::chat_dir(&store, &conversation.to_string()).unwrap();
    std::fs::write(work.join("source.jpg"), [255, 216, 255, 217]).unwrap();
    driver.round(|request| {
        let tool = request
            .tools
            .iter()
            .find(|tool| tool.name == "fs__read")
            .unwrap();
        assert!(tool.parameters.to_string().contains("viewImage"));
        assert!(
            request
                .system
                .as_deref()
                .unwrap()
                .contains(crate::images::IMAGE_VIEW_RULES)
        );
        vec![call(
            "fs__read",
            &json!({ "paths": [{ "path": "chat:source.jpg", "viewImage": true }] }),
        )]
    });
    driver.round(|request| {
        let images: Vec<_> = request
            .messages
            .iter()
            .flat_map(|message| &message.images)
            .collect();
        assert_eq!(images.len(), 1);
        assert_eq!(&*images[0].bytes, [255, 216, 255, 217]);
        vec![TextDelta::text("An image.")]
    });
    let handle = harness.turn(conversation, "Inspect source.jpg").await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let saved = harness.last(conversation);
    assert_eq!(
        crate::images::preview_images(saved.tool_calls[0].meta.as_ref()).len(),
        1
    );
    std::fs::remove_file(work.join("source.jpg")).unwrap();
    let handle = harness.turn(conversation, "Describe it again").await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    assert!(
        driver
            .requests()
            .last()
            .unwrap()
            .messages
            .iter()
            .any(|message| !message.images.is_empty())
    );
    harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).providers.update(
                &harness.provider_id.to_string(),
                &ProviderPatch {
                    adapter: Some(zvs_contract::AdapterFamily::OpenaiCompatible),
                    ..ProviderPatch::default()
                },
                2,
            )?;
            Ok(())
        })
        .unwrap();
    driver.round(|_| vec![call("fs__read", &json!({ "imageViewing": true, "paths": [{ "path": "chat:source.jpg", "viewImage": true }] }))]);
    let handle = harness.turn(conversation, "Continue with text").await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let rejected = harness.last(conversation);
    assert_eq!(
        rejected.tool_calls[0].status,
        zvs_contract::ChatToolCallStatus::Failed
    );
    assert_eq!(
        rejected.tool_calls[0].error.as_deref(),
        Some(crate::images::VISION_UNSUPPORTED)
    );
    let requests = driver.requests();
    let request = requests.last().unwrap();
    assert!(
        request
            .messages
            .iter()
            .all(|message| message.images.is_empty())
    );
    assert!(
        !request
            .system
            .as_deref()
            .unwrap()
            .contains(crate::images::IMAGE_VIEW_RULES)
    );
    assert!(
        !request
            .tools
            .iter()
            .find(|tool| tool.name == "fs__read")
            .unwrap()
            .parameters
            .to_string()
            .contains("viewImage")
    );
    let branch = harness.chat.branch(&conversation, &saved.id).unwrap();
    crate::chat_files::copy_chat_dir(&store, &conversation.to_string(), &branch.id.to_string())
        .unwrap();
    let branched = harness.last(branch.id);
    let meta = branched.tool_calls[0].meta.as_ref().unwrap();
    assert_eq!(meta["imageOwner"], json!(branch.id.to_string()));
    let images = crate::images::preview_images(Some(meta));
    assert_eq!(
        &*loader
            .load(&branch.id.to_string(), &images[0])
            .unwrap()
            .bytes,
        [255, 216, 255, 217]
    );
}

#[test]
fn unowned_images_are_refused_before_creating_a_run_or_message() {
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
    assert_eq!(error.message, crate::images::IMAGE_MISSING);
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
