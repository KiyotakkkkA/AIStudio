use std::sync::Arc;

use zvs_contract::{
    AppErrorCode, ConversationId, NOTE_READ_WHEN_MAX, NoteId, NoteReadInput, NoteRef,
    NoteRenameInput, NoteSaveInput,
};

use super::writes::{Prior, prepare};
use super::*;
use crate::conversations::ConversationFolders;
use crate::files::text::content_hash;
use crate::platform::clock::FakeClock;
use crate::testing::temporary_directory;

const CONVERSATION: &str = "0199aa11-1111-7111-8111-111111111111";
const FIRST_ID: &str = "0199aa11-2222-7111-8111-000000000001";
const SECOND_ID: &str = "0199aa11-2222-7111-8111-000000000002";
const START: i64 = 1_763_121_600_000;

fn draft(title: &str, body: &str) -> String {
    format!(
        "---\ntitle: {title}\ncontext: >\n  Testing background build cancellation showed\n  that children survive.\nread_when: Before changing how processes stop.\nupdate_when: When the launcher changes.\ndelete_when: After the mechanism is replaced.\n---\n\n{body}\n"
    )
}

fn ids(values: &[&str]) -> IdGenerator {
    let queue: Vec<uuid::Uuid> = values
        .iter()
        .map(|value| uuid::Uuid::parse_str(value).expect("a uuid"))
        .collect();
    let next = std::sync::Mutex::new(queue.into_iter());
    Arc::new(move || {
        next.lock()
            .expect("the id queue")
            .next()
            .expect("an id is left")
    })
}

fn never_taken(_: &NoteId) -> bool {
    false
}

fn fresh(text: &str, generator: &IdGenerator) -> Note {
    let rendered = prepare(
        text,
        &Prior {
            previous: None,
            existed: false,
            source: None,
        },
        &never_taken,
        "2026-11-14T12:00:00Z",
        generator,
    )
    .expect("a valid note");
    parse(rendered.as_bytes()).expect("parses back")
}

#[test]
fn a_model_draft_gets_host_managed_identity_and_dates() {
    let note = fresh(
        &draft("Shutdown", "# Shutdown\n\nDetails."),
        &ids(&[FIRST_ID]),
    );
    assert_eq!(note.id.to_string(), FIRST_ID);
    assert_eq!(note.created_at, "2026-11-14T12:00:00Z");
    assert_eq!(note.updated_at, "2026-11-14T12:00:00Z");
    assert_eq!(
        note.fields.context,
        "Testing background build cancellation showed that children survive."
    );
    assert_eq!(note.fields.body.trim(), "# Shutdown\n\nDetails.");
}

#[test]
fn a_new_note_never_takes_an_id_or_date_from_the_model() {
    let text = format!(
        "---\nid: {SECOND_ID}\ncreated_at: 2020-01-01T00:00:00Z\n{}",
        draft("T", "Body").trim_start_matches("---\n")
    );
    let note = fresh(&text, &ids(&[FIRST_ID]));
    assert_eq!(note.id.to_string(), FIRST_ID);
    assert_eq!(note.created_at, "2026-11-14T12:00:00Z");
}

#[test]
fn rendering_is_stable_and_round_trips() {
    let note = fresh(
        &draft("'Title: with colon'", "Body with --- inside"),
        &ids(&[FIRST_ID]),
    );
    let once = render(&note).expect("renders");
    let again = render(&parse(once.as_bytes()).expect("parses")).expect("renders");
    assert_eq!(once, again);
    assert!(once.starts_with("---\nschema_version: 1\nid: "));
}

#[test]
fn an_edit_keeps_identity_and_moves_updated_at_only_when_content_changes() {
    let original = fresh(&draft("T", "Body"), &ids(&[FIRST_ID]));
    let prior = Prior {
        previous: Some(&original),
        existed: true,
        source: None,
    };
    let same = parse(
        prepare(
            &draft("T", "Body"),
            &prior,
            &never_taken,
            "2026-11-15T09:00:00Z",
            &ids(&[]),
        )
        .expect("valid")
        .as_bytes(),
    )
    .expect("parses");
    assert_eq!(same.id, original.id);
    assert_eq!(same.updated_at, original.updated_at);
    let changed = parse(
        prepare(
            &draft("T", "Other body"),
            &prior,
            &never_taken,
            "2026-11-15T09:00:00Z",
            &ids(&[]),
        )
        .expect("valid")
        .as_bytes(),
    )
    .expect("parses");
    assert_eq!(changed.id, original.id);
    assert_eq!(changed.created_at, original.created_at);
    assert_eq!(changed.updated_at, "2026-11-15T09:00:00Z");
}

#[test]
fn a_repaired_note_keeps_its_own_unique_id() {
    let text = format!(
        "---\nschema_version: 1\nid: {SECOND_ID}\ncreated_at: 2026-01-01T00:00:00Z\n{}",
        draft("T", "Body").trim_start_matches("---\n")
    );
    let prior = Prior {
        previous: None,
        existed: true,
        source: None,
    };
    let kept = parse(
        prepare(
            &text,
            &prior,
            &never_taken,
            "2026-11-15T09:00:00Z",
            &ids(&[]),
        )
        .expect("valid")
        .as_bytes(),
    )
    .expect("parses");
    assert_eq!(kept.id.to_string(), SECOND_ID);
    assert_eq!(kept.created_at, "2026-01-01T00:00:00Z");
    let taken = |id: &NoteId| id.to_string() == SECOND_ID;
    let reassigned = parse(
        prepare(
            &text,
            &prior,
            &taken,
            "2026-11-15T09:00:00Z",
            &ids(&[FIRST_ID]),
        )
        .expect("valid")
        .as_bytes(),
    )
    .expect("parses");
    assert_eq!(reassigned.id.to_string(), FIRST_ID);
}

fn refusal(text: &str) -> AppError {
    parse_draft(text).expect_err("the draft is refused")
}

#[test]
fn malformed_drafts_are_refused_with_the_reason() {
    assert_eq!(refusal("# no frontmatter").message, NOTE_NO_FRONTMATTER);
    assert_eq!(
        refusal("---\ntitle: [unclosed\n---\nBody").message,
        NOTE_BAD_YAML
    );
    assert_eq!(
        refusal(&draft("T", "Body").replacen("title: T", "title: T\ntitle: U", 1)).message,
        NOTE_BAD_YAML
    );
    assert_eq!(
        refusal(&draft("T", "Body").replacen("title: T", "title: T\ntags: x", 1)).message,
        NOTE_UNKNOWN_FIELD
    );
    assert_eq!(
        refusal(&format!(
            "---\nschema_version: 2\n{}",
            draft("T", "B").trim_start_matches("---\n")
        ))
        .message,
        NOTE_SCHEMA_UNKNOWN
    );
    let missing = refusal(&draft("T", "Body").replacen(
        "read_when: Before changing how processes stop.\n",
        "",
        1,
    ));
    assert_eq!(missing.message, NOTE_FIELD_MISSING);
    assert_eq!(
        describe(&missing),
        format!("{NOTE_FIELD_MISSING}: read_when")
    );
    assert_eq!(
        refusal(&draft("T", "Body").replacen(
            "Before changing how processes stop.",
            &"x".repeat(NOTE_READ_WHEN_MAX + 1),
            1
        ))
        .message,
        NOTE_FIELD_TOO_LONG
    );
    assert_eq!(refusal(&draft("T", "   ")).message, NOTE_BODY_EMPTY);
    assert_eq!(
        refusal(&draft("T", "Body").replacen("title: T", "title:\n  - a", 1)).message,
        NOTE_BAD_VALUE
    );
}

#[test]
fn a_stored_note_needs_every_managed_field() {
    let draft_only = draft("T", "Body");
    assert_eq!(
        parse(draft_only.as_bytes()).expect_err("refused").message,
        NOTE_FIELD_MISSING
    );
    let note = fresh(&draft("T", "Body"), &ids(&[FIRST_ID]));
    let rendered = render(&note).expect("renders");
    assert_eq!(
        parse(rendered.replace(FIRST_ID, "not-an-id").as_bytes())
            .expect_err("refused")
            .message,
        NOTE_BAD_ID
    );
    assert_eq!(
        parse(
            rendered
                .replace("2026-11-14T12:00:00Z", "yesterday")
                .as_bytes()
        )
        .expect_err("refused")
        .message,
        NOTE_BAD_TIME
    );
    assert_eq!(
        parse(&[0, 159, 146, 150]).expect_err("refused").message,
        NOTE_BINARY
    );
    assert_eq!(
        parse(&vec![b'a'; NOTE_FILE_MAX_BYTES + 1])
            .expect_err("refused")
            .message,
        NOTE_TOO_LARGE
    );
}

#[test]
fn only_flat_markdown_files_under_notes_are_notes() {
    assert_eq!(
        note_name(Path::new("notes/topic.md")),
        Some(Some("topic.md".to_owned()))
    );
    assert_eq!(note_name(Path::new("notes/a/b.md")), Some(None));
    assert_eq!(note_name(Path::new("notes/a.txt")), Some(None));
    assert_eq!(note_name(Path::new("notes")), Some(None));
    assert_eq!(note_name(Path::new("Notes/a.md")), Some(None));
    assert_eq!(note_name(Path::new("other/a.md")), None);
}

#[test]
fn note_times_are_utc_seconds() {
    assert_eq!(note_time(START), "2025-11-14T12:00:00Z");
    assert!(is_note_time("2026-11-14T12:00:00Z"));
    assert!(is_note_time("2026-11-14T12:00:00.250Z"));
    assert!(!is_note_time("2026-11-14 12:00:00Z"));
    assert!(!is_note_time("2026-11-14T12:00:00+03:00"));
}

struct Fixture {
    _root: crate::testing::TemporaryDirectory,
    notes: Notes,
    conversation: ConversationId,
    folder: PathBuf,
}

fn fixture(generated: &[&str]) -> Fixture {
    let root = temporary_directory("notes-");
    let folders = ConversationFolders::new(root.path().to_path_buf());
    let notes = Notes::new(folders, Arc::new(FakeClock::new(START)), ids(generated));
    let conversation = ConversationId::parse(CONVERSATION).expect("an id");
    let folder = notes.folder(CONVERSATION).expect("a folder");
    Fixture {
        _root: root,
        notes,
        conversation,
        folder,
    }
}

fn store(fixture: &Fixture, name: &str, text: &str) -> PathBuf {
    std::fs::create_dir_all(&fixture.folder).expect("creates the folder");
    let path = fixture.folder.join(name);
    std::fs::write(&path, text).expect("writes");
    path
}

#[test]
fn a_missing_folder_lists_nothing_and_creates_nothing() {
    let fixture = fixture(&[]);
    assert!(fixture.notes.list(&fixture.conversation).is_empty());
    assert!(!fixture.folder.exists());
    assert!(!fixture.folder.parent().expect("work").exists());
}

#[test]
fn an_invalid_file_stays_listed_with_its_error_and_is_never_rewritten() {
    let fixture = fixture(&[]);
    let valid = render(&fresh(&draft("Valid", "Body"), &ids(&[FIRST_ID]))).expect("renders");
    store(&fixture, "valid.md", &valid);
    let broken = store(&fixture, "broken.md", "no frontmatter");
    store(&fixture, "ignored.txt", "x");
    let listed = fixture.notes.list(&fixture.conversation);
    assert_eq!(
        listed
            .iter()
            .map(|note| note.name.as_str())
            .collect::<Vec<_>>(),
        ["broken.md", "valid.md"]
    );
    assert_eq!(listed[0].error.as_deref(), Some(NOTE_NO_FRONTMATTER));
    assert_eq!(listed[1].title.as_deref(), Some("Valid"));
    assert_eq!(
        std::fs::read_to_string(broken).expect("reads"),
        "no frontmatter"
    );
}

#[test]
fn a_note_reads_by_name_or_by_id() {
    let fixture = fixture(&[]);
    let valid = render(&fresh(&draft("Valid", "Body"), &ids(&[FIRST_ID]))).expect("renders");
    store(&fixture, "valid.md", &valid);
    let by_id = fixture
        .notes
        .read(&NoteReadInput {
            conversation_id: fixture.conversation,
            name: None,
            id: NoteId::parse(FIRST_ID),
        })
        .expect("reads by id");
    assert_eq!(by_id.name, "valid.md");
    assert_eq!(by_id.hash, content_hash(valid.as_bytes()));
    let gone = fixture
        .notes
        .read(&NoteReadInput {
            conversation_id: fixture.conversation,
            name: None,
            id: NoteId::parse(SECOND_ID),
        })
        .expect_err("a deleted note");
    assert_eq!(gone.code, AppErrorCode::NotFound);
}

#[test]
fn an_editor_save_checks_the_loaded_hash() {
    let fixture = fixture(&[]);
    let valid = render(&fresh(&draft("Valid", "Body"), &ids(&[FIRST_ID]))).expect("renders");
    let path = store(&fixture, "valid.md", &valid);
    let save = |content: &str, hash: &str| {
        fixture.notes.save(&NoteSaveInput {
            conversation_id: fixture.conversation,
            name: "valid.md".to_owned(),
            content: content.to_owned(),
            expected_hash: hash.to_owned(),
        })
    };
    let edited = valid.replace("Body", "Edited body");
    let saved = save(&edited, &content_hash(valid.as_bytes())).expect("saves");
    let meta = saved.meta.expect("valid");
    assert_eq!(meta.id.to_string(), FIRST_ID);
    assert_eq!(meta.updated_at, note_time(START));
    let conflict = save(&valid, &content_hash(valid.as_bytes())).expect_err("stale");
    assert_eq!(conflict.code, AppErrorCode::Conflict);
    assert_eq!(conflict.message, NOTE_CONFLICT);
    let invalid = save("broken", &saved.hash).expect_err("invalid content");
    assert_eq!(invalid.message, NOTE_NO_FRONTMATTER);
    assert!(
        std::fs::read_to_string(path)
            .expect("reads")
            .contains("Edited body")
    );
}

#[test]
fn rename_refuses_another_notes_name_and_delete_is_final() {
    let fixture = fixture(&[]);
    let first = render(&fresh(&draft("First", "Body"), &ids(&[FIRST_ID]))).expect("renders");
    let second = render(&fresh(&draft("Second", "Body"), &ids(&[SECOND_ID]))).expect("renders");
    store(&fixture, "first.md", &first);
    store(&fixture, "second.md", &second);
    let rename = |to: &str| {
        fixture.notes.rename(&NoteRenameInput {
            conversation_id: fixture.conversation,
            name: "first.md".to_owned(),
            to: to.to_owned(),
        })
    };
    assert_eq!(
        rename("SECOND.md").expect_err("taken").message,
        NOTE_NAME_TAKEN
    );
    let renamed = rename("renamed.md").expect("renames");
    assert_eq!(renamed.meta.expect("valid").id.to_string(), FIRST_ID);
    assert_eq!(renamed.content, first);
    fixture
        .notes
        .remove(&NoteRef {
            conversation_id: fixture.conversation,
            name: "renamed.md".to_owned(),
        })
        .expect("removes");
    assert_eq!(
        fixture
            .notes
            .list(&fixture.conversation)
            .iter()
            .map(|note| note.name.as_str())
            .collect::<Vec<_>>(),
        ["second.md"]
    );
}
