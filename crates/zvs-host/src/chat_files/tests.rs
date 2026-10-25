use super::*;

const CHAT: &str = "01a17431-1de8-713c-9981-dbd5ddd5b80b";

#[test]
fn underscores_mark_working_files() {
    assert!(is_working(Path::new("_spec.json")));
    assert!(is_working(Path::new("_work/table.csv")));
    assert!(!is_working(Path::new("report.html")));
    assert!(!is_working(Path::new("data/report_v2.csv")));
}

#[test]
fn a_chat_folder_belongs_to_one_conversation_and_goes_with_it() {
    let root = tempfile::tempdir().expect("the data folder");
    let store = ArtifactStore::new(root.path().join("artifacts"));
    let folder = chat_dir(&store, CHAT).expect("the chat folder");
    assert_eq!(
        folder,
        root.path().join("artifacts").join(CHAT_FOLDERS).join(CHAT)
    );
    std::fs::create_dir_all(folder.join("notes")).expect("a nested folder");
    std::fs::write(folder.join("_spec.json"), "{}").expect("writes");
    std::fs::write(folder.join("notes").join("summary.md"), "# Итоги").expect("writes");
    assert_eq!(
        chat_files(&folder),
        [
            ("_spec.json".to_owned(), 2),
            ("notes/summary.md".to_owned(), "# Итоги".len() as u64)
        ]
    );
    assert_eq!(
        chat_dir(&store, "").expect_err("no conversation").message,
        NO_CHAT
    );
    assert!(chat_dir(&store, "../..").is_err());
    remove_chat_dir(&store, CHAT);
    assert!(!folder.exists());
    remove_chat_dir(&store, "../..");
    assert!(root.path().join("artifacts").exists());
}

#[test]
fn the_quota_counts_the_folder_after_the_changes() {
    let root = tempfile::tempdir().expect("the data folder");
    let folder = root.path().to_path_buf();
    std::fs::write(folder.join("a.txt"), "abc").expect("writes");
    within_quota(&folder, &[(folder.join("b.txt"), Some(10))]).expect("fits");
    within_quota(&folder, &[(folder.join("a.txt"), None)]).expect("a delete fits");
    let full = within_quota(
        &folder,
        &[(folder.join("big.bin"), Some(CHAT_FOLDER_MAX_BYTES))],
    )
    .expect_err("too much");
    assert_eq!(full.code, AppErrorCode::LimitExceeded);
    assert_eq!(full.message, CHAT_FOLDER_FULL);
    within_quota(
        &folder,
        &[(folder.join("a.txt"), Some(CHAT_FOLDER_MAX_BYTES))],
    )
    .expect("a rewrite replaces the old size");
}
