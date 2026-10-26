use zvs_contract::{BindFolderInput, BindFolderResultDto, FolderGrantScope};

use super::*;
use crate::testing::temporary_directory;

fn bind(harness: &Harness, conversation: ConversationId, path: &std::path::Path) -> String {
    match harness
        .chat
        .bind_folder(&BindFolderInput {
            id: conversation,
            path: path.to_string_lossy().into_owned(),
            grant: Some(FolderGrantScope::Conversation),
        })
        .expect("binds")
    {
        BindFolderResultDto::Bound { conversation } => {
            conversation.working_folder.expect("a working folder")
        }
        BindFolderResultDto::NeedsGrant { .. } => panic!("a grant was offered"),
    }
}

fn system(harness: &Harness, index: usize) -> String {
    harness.driver.requests()[index]
        .system
        .clone()
        .unwrap_or_default()
}

#[test]
fn the_section_names_the_folder_or_its_absence_and_a_change() {
    let bound = working_folder_section(Some(r"D:\work"), None);
    assert_eq!(
        bound,
        format!(
            "{WORKING_FOLDER_HEADING}: {}\n{WORKING_FOLDER_RULES}",
            r"D:\work"
        )
    );
    assert_eq!(working_folder_section(None, None), NO_WORKING_FOLDER_NOTE);
    assert_eq!(
        working_folder_section(Some(r"D:\work"), Some(Some(r"D:\work"))),
        bound
    );
    assert!(
        working_folder_section(Some(r"D:\site"), Some(Some(r"D:\work")))
            .ends_with(&format!("{WORKING_FOLDER_CHANGED}; it was {}.", r"D:\work"))
    );
    assert!(
        working_folder_section(None, Some(Some(r"D:\work"))).starts_with(NO_WORKING_FOLDER_NOTE)
    );
    assert!(
        working_folder_section(Some(r"D:\work"), Some(None))
            .ends_with(&format!("{WORKING_FOLDER_CHANGED}; there was none."))
    );
}

#[tokio::test]
async fn each_turn_carries_the_working_folder_and_notes_when_it_changed() {
    let harness = Harness::new();
    let first = temporary_directory("folder-a-");
    let second = temporary_directory("folder-b-");
    let conversation = harness.conversation();

    within(harness.turn(conversation, "no folder")).await;
    assert!(!system(&harness, 0).contains(WORKING_FOLDER_HEADING));

    let bound = bind(&harness, conversation, first.path());
    within(harness.turn(conversation, "with a folder")).await;
    let prompt = system(&harness, 1);
    assert!(prompt.contains(&format!("{WORKING_FOLDER_HEADING}: {bound}")));
    assert!(prompt.contains(WORKING_FOLDER_RULES));
    assert!(prompt.contains(&format!("{WORKING_FOLDER_CHANGED}; there was none.")));

    within(harness.turn(conversation, "same folder")).await;
    assert!(!system(&harness, 2).contains(WORKING_FOLDER_CHANGED));

    let moved = bind(&harness, conversation, second.path());
    within(harness.turn(conversation, "another folder")).await;
    let prompt = system(&harness, 3);
    assert!(prompt.contains(&format!("{WORKING_FOLDER_HEADING}: {moved}")));
    assert!(prompt.contains(&format!("{WORKING_FOLDER_CHANGED}; it was {bound}.")));

    harness.chat.unbind_folder(&conversation).expect("unbinds");
    within(harness.turn(conversation, "folder removed")).await;
    let prompt = system(&harness, 4);
    assert!(prompt.contains(NO_WORKING_FOLDER_NOTE));
    assert!(prompt.contains(&format!("{WORKING_FOLDER_CHANGED}; it was {moved}.")));
}

#[test]
fn the_file_tools_section_names_the_offered_file_tools_and_the_rules() {
    let tool = |key: &str, name: &str| OfferedTool {
        key: key.to_owned(),
        name: name.to_owned(),
        description: String::new(),
        parameters: serde_json::json!({}),
        destructive: false,
        read_only: true,
        source: zvs_contract::OfferedToolSource::Base,
        server: None,
    };
    assert!(file_tools_section(&[tool("demo.x", "demo__x")]).is_empty());
    assert_eq!(
        file_tools_section(&[tool("fs.apply", "fs__apply"), tool("fs.read", "fs__read")]),
        format!("{FILE_TOOLS_HEADING}: fs__read, fs__apply\n{FILE_TOOLS_RULES}")
    );
}
