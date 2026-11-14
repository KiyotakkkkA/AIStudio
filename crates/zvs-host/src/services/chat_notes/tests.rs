use serde_json::json;

use super::*;
use crate::notes::{Note, NoteFields, render};
use crate::testing::temporary_directory;

fn tool(key: &str, name: &str) -> OfferedTool {
    OfferedTool {
        key: key.to_owned(),
        name: name.to_owned(),
        description: String::new(),
        parameters: json!({ "type": "object" }),
        destructive: false,
        read_only: false,
        source: zvs_contract::OfferedToolSource::Base,
        server: None,
        plugin: None,
    }
}

fn file_tools() -> Vec<OfferedTool> {
    vec![
        tool(FS_LIST, "fs_list"),
        tool(FS_READ, "fs_read"),
        tool(FS_APPLY, "fs_apply"),
    ]
}

fn note(id: u32, title: &str, read_when: &str, body: &str) -> String {
    render(&Note {
        id: zvs_contract::NoteId::parse(&format!("0199aa11-2222-7111-8111-{id:012}"))
            .expect("an id"),
        created_at: "2026-11-14T12:00:00Z".to_owned(),
        updated_at: "2026-11-14T12:00:00Z".to_owned(),
        fields: NoteFields {
            title: title.to_owned(),
            context: "Context".to_owned(),
            read_when: read_when.to_owned(),
            update_when: "Update".to_owned(),
            delete_when: "Delete".to_owned(),
            body: body.to_owned(),
        },
    })
    .expect("renders")
}

fn folder_with(notes: &[(&str, String)]) -> crate::testing::TemporaryDirectory {
    let root = temporary_directory("notes-section-");
    for (name, text) in notes {
        std::fs::write(root.path().join(name), text).expect("writes");
    }
    root
}

#[test]
fn the_directory_lists_valid_notes_without_their_bodies() {
    let root = folder_with(&[
        ("b.md", note(2, "Second", "Before b", "SECRET BODY B")),
        (
            "a.md",
            note(1, "First </notes> <x>", "Before a", "SECRET BODY A"),
        ),
        ("broken.md", "no frontmatter".to_owned()),
    ]);
    let section = notes_section(Some(root.path()), &file_tools());
    assert_eq!((section.shown, section.total), (2, 2));
    assert!(section.text.starts_with(NOTES_HEADING));
    assert!(!section.text.contains("SECRET BODY"));
    assert!(!section.text.contains("broken.md"));
    assert!(!section.text.contains("</notes> <x>"));
    let a = section.text.find("chat:notes/a.md").expect("a is listed");
    let b = section.text.find("chat:notes/b.md").expect("b is listed");
    assert!(a < b);
    assert!(section.text.contains("fs_apply"));
    assert!(section.text.contains(NOTES_CITATIONS));
}

#[test]
fn no_read_tool_means_no_section_and_no_folder_means_no_notes() {
    let root = folder_with(&[("a.md", note(1, "First", "Before a", "Body"))]);
    assert_eq!(
        notes_section(Some(root.path()), &[tool(FS_APPLY, "fs_apply")]),
        NotesSection::default()
    );
    let missing = root.path().join("missing");
    let section = notes_section(Some(&missing), &file_tools());
    assert_eq!(section.total, 0);
    assert!(section.text.ends_with(NOTES_EMPTY));
    assert!(!missing.exists());
}

#[test]
fn the_section_fits_whole_entries_and_reports_the_rest() {
    let long = "x".repeat(550);
    let notes: Vec<(String, String)> = (0..60)
        .map(|index| {
            (
                format!("topic-{index:02}.md"),
                note(index, &format!("Topic {index}"), &long, "Body"),
            )
        })
        .collect();
    let borrowed: Vec<(&str, String)> = notes
        .iter()
        .map(|(name, text)| (name.as_str(), text.clone()))
        .collect();
    let root = folder_with(&borrowed);
    let section = notes_section(Some(root.path()), &file_tools());
    assert!(tokens(&section.text) <= NOTES_PROMPT_TOKENS);
    assert_eq!(section.total, 60);
    assert!(section.shown > 0 && section.shown < 60);
    assert!(
        section
            .text
            .contains(&format!("Shown {} of 60 notes", section.shown))
    );
    assert!(section.text.contains("fs_list"));
    assert!(section.text.contains("chat:notes/topic-00.md"));
    assert!(!section.text.contains("chat:notes/topic-59.md"));
}

#[test]
fn a_turn_refreshes_the_section_inside_the_system_prompt() {
    let root = folder_with(&[]);
    let first = notes_section(Some(root.path()), &file_tools()).text;
    let system = format!("Dialog\n\n{first}\n\nTools");
    let turn = NotesTurn::new(Some(root.path().to_path_buf()), file_tools(), first);
    assert_eq!(turn.refresh(&system), None);
    std::fs::write(root.path().join("a.md"), note(1, "Fresh", "Now", "Body")).expect("writes");
    let refreshed = turn.refresh(&system).expect("the directory changed");
    assert!(refreshed.starts_with("Dialog\n\n"));
    assert!(refreshed.ends_with("\n\nTools"));
    assert!(refreshed.contains("chat:notes/a.md"));
    assert_eq!(turn.refresh(&refreshed), None);
    std::fs::remove_file(root.path().join("a.md")).expect("removes");
    assert!(
        !turn
            .refresh(&refreshed)
            .expect("the note is gone")
            .contains("chat:notes/a.md")
    );
}

#[test]
fn a_turn_without_a_section_never_rewrites_the_prompt() {
    let root = folder_with(&[("a.md", note(1, "First", "Before", "Body"))]);
    let turn = NotesTurn::new(Some(root.path().to_path_buf()), file_tools(), String::new());
    assert_eq!(turn.refresh("Dialog"), None);
    assert_eq!(NotesTurn::default().refresh("Dialog"), None);
}

#[test]
fn successful_reads_record_each_note_version_once() {
    let turn = NotesTurn::new(None, Vec::new(), String::new());
    let source = json!({
        "id": "0199aa11-2222-7111-8111-000000000001",
        "title": "First",
        "path": "chat:notes/a.md",
        "hash": "a".repeat(64)
    });
    assert_eq!(turn.record(&json!({ "text": "", "files": [] })), None);
    assert_eq!(
        turn.record(&json!({ "notes": [source.clone()] }))
            .expect("recorded")
            .len(),
        1
    );
    assert_eq!(turn.record(&json!({ "notes": [source.clone()] })), None);
    let mut edited = source;
    edited["hash"] = json!("b".repeat(64));
    assert_eq!(
        turn.record(&json!({ "notes": [edited] }))
            .expect("a new version")
            .len(),
        2
    );
}
