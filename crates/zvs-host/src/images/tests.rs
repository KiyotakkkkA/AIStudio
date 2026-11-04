use super::*;

fn png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=").expect("PNG")
}

#[test]
fn browser_screenshots_have_owned_paths_immutable_previews_and_branch_copies() {
    let root = crate::testing::temporary_directory("browser-images");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let loader = ImageLoader::new(store.clone());
    let (path, image) = loader.save_browser("first", "/9j/2Q==", 1234).unwrap();
    assert!(image.name.starts_with("screen-"));
    assert!(image.name.ends_with("-1234.jpg"));
    assert_eq!(std::fs::read(&path).unwrap(), [255, 216, 255, 217]);
    assert_eq!(
        Path::new(&path).parent().unwrap().file_name().unwrap(),
        "browser"
    );
    assert!(loader.browser_path("first", &path).unwrap().is_some());
    assert!(loader.browser_path("other", &path).unwrap().is_none());
    assert!(
        loader
            .browser_path("first", &format!("{path}/../{}", image.name))
            .is_err()
    );
    crate::chat_files::copy_chat_dir(&store, "first", "branch").unwrap();
    crate::chat_files::remove_chat_dir(&store, "first");
    assert_eq!(
        &*loader.load("branch", &image).unwrap().bytes,
        [255, 216, 255, 217]
    );
    assert!(
        root.path()
            .join("conversations/branch/attachments/browser")
            .join(&image.name)
            .is_file()
    );
    assert!(loader.save_browser("../escape", "/9j/2Q==", 1234).is_err());
    assert!(loader.save_browser("branch", "bad pixels", 1234).is_err());
}

#[test]
fn image_references_are_text_only_when_the_provider_does_not_support_pixels() {
    let image = ImageAttachmentDto {
        id: uuid::Uuid::new_v4().to_string(),
        name: "source.png".into(),
        mime_type: "image/png".into(),
        size_bytes: 68,
    };
    let mut messages = vec![ChatMessage::user("Describe").with_images(vec![image.clone()])];
    text_references(&mut messages);
    assert!(messages[0].images.is_empty());
    assert!(messages[0].content.contains(&image.name));
    assert!(messages[0].content.starts_with("Describe"));
    assert!(supports_adapter("qwen-web"));
    assert!(supports_adapter("zai-web"));
    assert!(!supports_adapter("openai-compatible"));
}

#[test]
fn image_window_keeps_the_latest_five_and_screenshots_are_not_automatically_attached() {
    let images: Vec<_> = (0..7)
        .map(|index| ImageAttachmentDto {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{index}.png"),
            mime_type: "image/png".into(),
            size_bytes: 68,
        })
        .collect();
    let mut messages = vec![
        ChatMessage::user("old").with_images(images[..3].to_vec()),
        ChatMessage::user("new").with_images(images[3..].to_vec()),
    ];
    bound_images(&mut messages);
    assert_eq!(messages[0].images.len(), 1);
    assert_eq!(messages[1].images.len(), 4);
    let meta = serde_json::json!({ "images": [images[0].clone()] });
    assert!(tool_images("browser.screenshot", Some(&meta)).is_empty());
    assert!(tool_images("external.fake", Some(&meta)).is_empty());
    assert_eq!(tool_images("fs.read", Some(&meta)), [images[0].clone()]);
}

#[test]
fn provider_specific_tool_schemas_expose_image_viewing_only_when_supported() {
    use crate::services::chat_tools::OfferedTool;
    let tool: OfferedTool = serde_json::from_value(serde_json::json!({ "key": "fs.read", "name": "fs__read", "description": crate::files::nodes::READ_DESCRIPTION, "parameters": crate::kernel::registry::input_schema::<crate::files::nodes::FsReadInput>(), "destructive": false })).unwrap();
    let mut supported = vec![tool.clone()];
    configure_tools(&mut supported, true);
    assert!(supported[0].parameters.to_string().contains("viewImage"));
    assert_eq!(
        supported[0].description,
        crate::files::nodes::READ_IMAGE_DESCRIPTION
    );
    let mut unsupported = vec![tool];
    configure_tools(&mut unsupported, false);
    assert!(!unsupported[0].parameters.to_string().contains("viewImage"));
    assert!(viewing_section(&unsupported, false).is_empty());
    assert_eq!(viewing_section(&supported, true), IMAGE_VIEW_RULES);
    configure_tools(&mut supported, false);
    assert_eq!(
        supported[0].description,
        crate::files::nodes::READ_DESCRIPTION
    );
    assert!(!supported[0].parameters.to_string().contains("viewImage"));
    configure_tools(&mut supported, true);
    assert!(supported[0].parameters.to_string().contains("viewImage"));
}

#[test]
fn imports_survive_restart_and_branch_and_detect_tampering() {
    let root = crate::testing::temporary_directory("vision");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let source = root.path().join("source.png");
    std::fs::write(&source, png()).unwrap();
    let images = ImageLoader::new(store.clone())
        .import("first", &[source.to_string_lossy().into_owned()])
        .unwrap();
    std::fs::remove_file(&source).unwrap();
    let loader = ImageLoader::new(store.clone());
    assert_eq!(&*loader.load("first", &images[0]).unwrap().bytes, png());
    crate::chat_files::copy_chat_dir(&store, "first", "branch").unwrap();
    assert!(loader.load("branch", &images[0]).is_ok());
    assert!(loader.load("other", &images[0]).is_err());
    let mut forged = images[0].clone();
    forged.id = "../source.png".into();
    assert!(loader.load("first", &forged).is_err());
    forged = images[0].clone();
    forged.name = "forged.png".into();
    assert!(loader.load("first", &forged).is_err());
    std::fs::write(
        loader.folder("first").unwrap().join(&images[0].id),
        b"changed",
    )
    .unwrap();
    assert!(loader.load("first", &images[0]).is_err());
    assert!(loader.load("branch", &images[0]).is_ok());
}

#[test]
fn moves_copies_from_the_legacy_work_subfolder_into_attachments() {
    let root = crate::testing::temporary_directory("vision");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let loader = ImageLoader::new(store);
    let source = root.path().join("source.png");
    std::fs::write(&source, png()).unwrap();
    let images = loader
        .import("chat", &[source.to_string_lossy().into_owned()])
        .unwrap();
    let attachments = loader.folder("chat").unwrap();
    assert_eq!(
        attachments.file_name().unwrap(),
        crate::conversations::ATTACHMENTS_DIR
    );
    let legacy = root.path().join("conversations/chat/work/_images");
    std::fs::create_dir_all(&legacy).unwrap();
    for entry in std::fs::read_dir(&attachments).unwrap().flatten() {
        std::fs::rename(entry.path(), legacy.join(entry.file_name())).unwrap();
    }
    std::fs::remove_dir(&attachments).unwrap();
    assert_eq!(&*loader.load("chat", &images[0]).unwrap().bytes, png());
    assert!(!legacy.exists());
    assert!(attachments.join(&images[0].id).is_file());
}

#[test]
fn rejects_non_images_and_too_many_before_persisting_a_batch() {
    let root = crate::testing::temporary_directory("vision");
    let store = ArtifactStore::new(root.path().join("conversations"));
    let loader = ImageLoader::new(store);
    let source = root.path().join("fake.png");
    std::fs::write(&source, "not an image").unwrap();
    let path = source.to_string_lossy().into_owned();
    assert_eq!(
        loader
            .import("chat", std::slice::from_ref(&path))
            .unwrap_err()
            .message,
        IMAGE_INVALID
    );
    assert_eq!(
        loader
            .import("chat", &vec![path; IMAGES_MAX + 1])
            .unwrap_err()
            .message,
        IMAGES_FULL
    );
    assert!(
        std::fs::read_dir(loader.folder("chat").unwrap())
            .unwrap()
            .next()
            .is_none()
    );
}

#[tokio::test]
async fn loading_obeys_cancellation_and_never_prints_pixels() {
    let root = crate::testing::temporary_directory("vision");
    let loader = ImageLoader::new(ArtifactStore::new(root.path().join("conversations")));
    let attachment = ImageAttachmentDto {
        id: uuid::Uuid::new_v4().to_string(),
        name: "a.png".into(),
        mime_type: "image/png".into(),
        size_bytes: 33,
    };
    let mut messages = vec![ChatMessage::user("").with_images(vec![attachment])];
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        loader
            .load_messages("chat", &mut messages, &cancel)
            .await
            .is_err()
    );
    assert!(!format!("{:?}", messages[0].images).contains("a.png"));
}
