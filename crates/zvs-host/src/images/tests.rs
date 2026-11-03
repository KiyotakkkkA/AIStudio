use super::*;

fn png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=").expect("PNG")
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
