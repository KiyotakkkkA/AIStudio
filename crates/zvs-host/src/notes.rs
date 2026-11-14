use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_yaml_ng::{Mapping, Value as Yaml};
use zvs_contract::{
    AppError, AppErrorCode, ConversationId, NOTE_CONDITION_MAX, NOTE_CONTEXT_MAX,
    NOTE_FILE_MAX_BYTES, NOTE_READ_WHEN_MAX, NOTE_SCHEMA_VERSION, NOTE_TITLE_MAX, NOTES_FOLDER,
    NOTES_LIST_MAX, NoteDto, NoteId, NoteMetaDto, NoteReadInput, NoteRef, NoteRenameInput,
    NoteSaveInput, NoteSourceDto, NoteSummaryDto, Timestamp, is_note_name, note_path,
};

use crate::chat_files::within_quota;
use crate::conversations::{ConversationFolders, WORK_DIR};
use crate::files::commit::replace;
use crate::files::text::{Decoded, content_hash, decode, encode, normalize};
use crate::platform::clock::SharedClock;
use crate::platform::ids::IdGenerator;
use crate::platform::long_path::extended;
use crate::platform::paths::utc_iso;

pub mod writes;

#[cfg(test)]
mod tests;

pub const NOTE_NO_FRONTMATTER: &str =
    "В начале заметки требуется заголовок YAML между строками ---";
pub const NOTE_BAD_YAML: &str = "Заголовок заметки содержит некорректный YAML";
pub const NOTE_UNKNOWN_FIELD: &str = "Заголовок заметки содержит неизвестное поле";
pub const NOTE_BAD_VALUE: &str = "Поле заголовка заметки содержит недопустимый тип значения";
pub const NOTE_SCHEMA_UNKNOWN: &str = "Версия формата заметки не поддерживается";
pub const NOTE_FIELD_MISSING: &str = "Не заполнено обязательное поле заметки";
pub const NOTE_FIELD_TOO_LONG: &str = "Поле заметки превышает допустимую длину";
pub const NOTE_BODY_EMPTY: &str = "Текст заметки пуст";
pub const NOTE_TOO_LARGE: &str = "Размер файла заметки превышает 128 КиБ";
pub const NOTE_BINARY: &str = "Формат заметки не поддерживается: требуется текст в кодировке UTF-8";
pub const NOTE_BAD_ID: &str = "Некорректный идентификатор заметки";
pub const NOTE_BAD_TIME: &str = "Некорректная дата в заголовке заметки";
pub const NOTE_BAD_PATH: &str = "Заметки допускаются только в папке chat:notes/, без вложенных папок, в файлах с расширением .md";
pub const NOTE_NAME_TAKEN: &str = "Заметка с таким именем уже существует";
pub const NOTE_MISSING: &str = "Заметка не найдена";
pub const NOTE_CONFLICT: &str = "Сохранение отклонено: заметка изменена после открытия";
pub const NOTE_UNREADABLE: &str = "Не удалось прочитать заметку";
pub const NOTE_WRITE_FAILED: &str = "Не удалось сохранить заметку";

const SCHEMA_VERSION: &str = "schema_version";
const ID: &str = "id";
const TITLE: &str = "title";
const CREATED_AT: &str = "created_at";
const UPDATED_AT: &str = "updated_at";
const CONTEXT: &str = "context";
const READ_WHEN: &str = "read_when";
const UPDATE_WHEN: &str = "update_when";
const DELETE_WHEN: &str = "delete_when";
const FIELDS: [&str; 9] = [
    SCHEMA_VERSION,
    ID,
    TITLE,
    CREATED_AT,
    UPDATED_AT,
    CONTEXT,
    READ_WHEN,
    UPDATE_WHEN,
    DELETE_WHEN,
];
const DELIMITER: &str = "---";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFields {
    pub title: String,
    pub context: String,
    pub read_when: String,
    pub update_when: String,
    pub delete_when: String,
    pub body: String,
}

impl NoteFields {
    pub fn same(&self, other: &Self) -> bool {
        self.title == other.title
            && self.context == other.context
            && self.read_when == other.read_when
            && self.update_when == other.update_when
            && self.delete_when == other.delete_when
            && self.body.trim() == other.body.trim()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub schema_version: bool,
    pub fields: NoteFields,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: NoteId,
    pub created_at: String,
    pub updated_at: String,
    pub fields: NoteFields,
}

impl Note {
    pub fn meta(&self) -> NoteMetaDto {
        NoteMetaDto {
            id: self.id,
            title: self.fields.title.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            context: self.fields.context.clone(),
            read_when: self.fields.read_when.clone(),
            update_when: self.fields.update_when.clone(),
            delete_when: self.fields.delete_when.clone(),
        }
    }
}

fn refused(message: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
}

fn field_error(message: &str, field: &str) -> AppError {
    refused(message).with_detail("field", field)
}

pub fn describe(error: &AppError) -> String {
    match error
        .details
        .as_ref()
        .and_then(|details| details.get("field"))
    {
        Some(serde_json::Value::String(field)) => format!("{}: {field}", error.message),
        _ => error.message.clone(),
    }
}

fn split(text: &str) -> Result<(&str, &str), AppError> {
    let rest = text
        .strip_prefix("---\n")
        .ok_or_else(|| refused(NOTE_NO_FRONTMATTER))?;
    if let Some(body) = rest.strip_prefix("---\n") {
        return Ok(("", body));
    }
    if rest == DELIMITER {
        return Ok(("", ""));
    }
    if let Some(end) = rest.find("\n---\n") {
        return Ok((&rest[..end], &rest[end + 5..]));
    }
    rest.strip_suffix("\n---")
        .map(|yaml| (yaml, ""))
        .ok_or_else(|| refused(NOTE_NO_FRONTMATTER))
}

fn scalar(map: &Mapping, field: &str) -> Result<Option<String>, AppError> {
    match map.get(field) {
        None | Some(Yaml::Null) => Ok(None),
        Some(Yaml::String(text)) => Ok(Some(text.trim().to_owned())),
        Some(Yaml::Number(number)) => Ok(Some(number.to_string())),
        Some(Yaml::Bool(flag)) => Ok(Some(flag.to_string())),
        Some(_) => Err(field_error(NOTE_BAD_VALUE, field)),
    }
}

fn required(map: &Mapping, field: &str, max: usize) -> Result<String, AppError> {
    let value = scalar(map, field)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| field_error(NOTE_FIELD_MISSING, field))?;
    if value.encode_utf16().count() > max {
        return Err(field_error(NOTE_FIELD_TOO_LONG, field));
    }
    Ok(value)
}

pub fn parse_draft(text: &str) -> Result<Draft, AppError> {
    let text = normalize(text);
    let (yaml, body) = split(&text)?;
    let map = if yaml.trim().is_empty() {
        Mapping::new()
    } else {
        serde_yaml_ng::from_str::<Mapping>(yaml)
            .map_err(|error| refused(NOTE_BAD_YAML).with_detail("raw", error.to_string()))?
    };
    for key in map.keys() {
        match key {
            Yaml::String(name) if FIELDS.contains(&name.as_str()) => {}
            Yaml::String(name) => return Err(field_error(NOTE_UNKNOWN_FIELD, name)),
            _ => return Err(refused(NOTE_BAD_YAML)),
        }
    }
    let schema_version = match map.get(SCHEMA_VERSION) {
        None | Some(Yaml::Null) => false,
        Some(Yaml::Number(number)) if number.as_u64() == Some(u64::from(NOTE_SCHEMA_VERSION)) => {
            true
        }
        Some(_) => return Err(field_error(NOTE_SCHEMA_UNKNOWN, SCHEMA_VERSION)),
    };
    let fields = NoteFields {
        title: required(&map, TITLE, NOTE_TITLE_MAX)?,
        context: required(&map, CONTEXT, NOTE_CONTEXT_MAX)?,
        read_when: required(&map, READ_WHEN, NOTE_READ_WHEN_MAX)?,
        update_when: required(&map, UPDATE_WHEN, NOTE_CONDITION_MAX)?,
        delete_when: required(&map, DELETE_WHEN, NOTE_CONDITION_MAX)?,
        body: body.to_owned(),
    };
    if fields.body.trim().is_empty() {
        return Err(refused(NOTE_BODY_EMPTY));
    }
    Ok(Draft {
        id: scalar(&map, ID)?,
        created_at: scalar(&map, CREATED_AT)?,
        updated_at: scalar(&map, UPDATED_AT)?,
        schema_version,
        fields,
    })
}

pub fn is_note_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    let digits = |range: std::ops::Range<usize>| bytes[range].iter().all(u8::is_ascii_digit);
    let Some(rest) = value.strip_suffix('Z') else {
        return false;
    };
    let fraction = rest.get(19..).unwrap_or_default();
    rest.len() >= 19
        && digits(0..4)
        && bytes[4] == b'-'
        && digits(5..7)
        && bytes[7] == b'-'
        && digits(8..10)
        && bytes[10] == b'T'
        && digits(11..13)
        && bytes[13] == b':'
        && digits(14..16)
        && bytes[16] == b':'
        && digits(17..19)
        && (fraction.is_empty()
            || (fraction.len() > 1
                && fraction.starts_with('.')
                && fraction[1..].bytes().all(|byte| byte.is_ascii_digit())))
}

pub fn note_time(at: Timestamp) -> String {
    let iso = utc_iso(at);
    format!("{}Z", &iso[..19])
}

pub fn text_of(bytes: &[u8]) -> Result<String, AppError> {
    if bytes.len() > NOTE_FILE_MAX_BYTES {
        return Err(refused(NOTE_TOO_LARGE));
    }
    match decode(bytes) {
        Decoded::Text { text, .. } => Ok(text),
        Decoded::Binary => Err(refused(NOTE_BINARY)),
    }
}

pub fn parse(bytes: &[u8]) -> Result<Note, AppError> {
    let draft = parse_draft(&text_of(bytes)?)?;
    if !draft.schema_version {
        return Err(field_error(NOTE_FIELD_MISSING, SCHEMA_VERSION));
    }
    let id = draft
        .id
        .as_deref()
        .ok_or_else(|| field_error(NOTE_FIELD_MISSING, ID))
        .and_then(|id| NoteId::parse(id).ok_or_else(|| field_error(NOTE_BAD_ID, ID)))?;
    let time = |value: Option<String>, field: &str| {
        let value = value.ok_or_else(|| field_error(NOTE_FIELD_MISSING, field))?;
        if is_note_time(&value) {
            Ok(value)
        } else {
            Err(field_error(NOTE_BAD_TIME, field))
        }
    };
    Ok(Note {
        id,
        created_at: time(draft.created_at, CREATED_AT)?,
        updated_at: time(draft.updated_at, UPDATED_AT)?,
        fields: draft.fields,
    })
}

#[derive(Serialize)]
struct Canonical<'a> {
    schema_version: u32,
    id: String,
    title: &'a str,
    created_at: &'a str,
    updated_at: &'a str,
    context: &'a str,
    read_when: &'a str,
    update_when: &'a str,
    delete_when: &'a str,
}

pub fn render(note: &Note) -> Result<String, AppError> {
    let fields = &note.fields;
    let yaml = serde_yaml_ng::to_string(&Canonical {
        schema_version: NOTE_SCHEMA_VERSION,
        id: note.id.to_string(),
        title: &fields.title,
        created_at: &note.created_at,
        updated_at: &note.updated_at,
        context: &fields.context,
        read_when: &fields.read_when,
        update_when: &fields.update_when,
        delete_when: &fields.delete_when,
    })
    .map_err(|error| refused(NOTE_BAD_YAML).with_detail("raw", error.to_string()))?;
    let body = fields.body.trim_end();
    let lead = if body.starts_with('\n') { "" } else { "\n" };
    let text = format!("{DELIMITER}\n{yaml}{DELIMITER}\n{lead}{body}\n");
    if text.len() > NOTE_FILE_MAX_BYTES {
        return Err(refused(NOTE_TOO_LARGE));
    }
    Ok(text)
}

pub fn note_name(relative: &Path) -> Option<Option<String>> {
    let mut components = relative.components();
    match components.next() {
        Some(Component::Normal(first))
            if first.to_string_lossy().eq_ignore_ascii_case(NOTES_FOLDER) => {}
        _ => return None,
    }
    let first = relative.components().next()?;
    let exact = matches!(first, Component::Normal(name) if name == NOTES_FOLDER);
    let rest: Vec<String> = components
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(match rest.as_slice() {
        [name] if exact && is_note_name(name) => Some(name.clone()),
        _ => None,
    })
}

pub fn work_folder(base: &Path) -> PathBuf {
    if base.file_name().is_some_and(|name| name == WORK_DIR) {
        base.to_path_buf()
    } else {
        base.join(WORK_DIR)
    }
}

pub fn note_at(base: &Path, path: &Path) -> Option<Option<String>> {
    path.strip_prefix(work_folder(base))
        .ok()
        .and_then(note_name)
}

#[derive(Debug, Clone)]
pub struct Scanned {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified_at: Timestamp,
    pub bytes: Option<Vec<u8>>,
    pub note: Result<Note, AppError>,
}

impl Scanned {
    pub fn summary(&self) -> NoteSummaryDto {
        let (id, title, updated_at, error) = match &self.note {
            Ok(note) => (
                Some(note.id),
                Some(note.fields.title.clone()),
                Some(note.updated_at.clone()),
                None,
            ),
            Err(error) => (None, None, None, Some(describe(error))),
        };
        NoteSummaryDto {
            name: self.name.clone(),
            path: note_path(&self.name),
            size: self.size,
            modified_at: self.modified_at,
            id,
            title,
            updated_at,
            error,
        }
    }

    pub fn dto(&self) -> Result<NoteDto, AppError> {
        let bytes = self
            .bytes
            .as_deref()
            .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOTE_TOO_LARGE))?;
        let content = match decode(bytes) {
            Decoded::Text { text, .. } => text,
            Decoded::Binary => String::new(),
        };
        let (meta, body, error) = match &self.note {
            Ok(note) => (Some(note.meta()), Some(note.fields.body.clone()), None),
            Err(error) => (None, None, Some(describe(error))),
        };
        Ok(NoteDto {
            name: self.name.clone(),
            path: note_path(&self.name),
            hash: content_hash(bytes),
            content,
            modified_at: self.modified_at,
            meta,
            body,
            error,
        })
    }

    pub fn source(&self) -> Option<NoteSourceDto> {
        let note = self.note.as_ref().ok()?;
        Some(NoteSourceDto {
            id: note.id,
            title: note.fields.title.clone(),
            path: note_path(&self.name),
            hash: content_hash(self.bytes.as_deref()?),
        })
    }
}

fn modified(metadata: &std::fs::Metadata) -> Timestamp {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| {
            Timestamp::try_from(elapsed.as_millis()).unwrap_or(0)
        })
}

pub fn scan_file(path: &Path, name: &str) -> Option<Scanned> {
    let metadata = std::fs::metadata(extended(path)).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let size = metadata.len();
    let bytes = if usize::try_from(size).unwrap_or(usize::MAX) > NOTE_FILE_MAX_BYTES {
        None
    } else {
        std::fs::read(extended(path)).ok()
    };
    let note = match &bytes {
        Some(bytes) => parse(bytes),
        None if usize::try_from(size).unwrap_or(usize::MAX) > NOTE_FILE_MAX_BYTES => {
            Err(refused(NOTE_TOO_LARGE))
        }
        None => Err(refused(NOTE_UNREADABLE)),
    };
    Some(Scanned {
        name: name.to_owned(),
        path: path.to_path_buf(),
        size,
        modified_at: modified(&metadata),
        bytes,
        note,
    })
}

pub fn scan(folder: &Path) -> Vec<Scanned> {
    let Ok(entries) = std::fs::read_dir(extended(folder)) else {
        return Vec::new();
    };
    let mut found: Vec<Scanned> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_note_name(&name) {
                return None;
            }
            scan_file(&folder.join(&name), &name)
        })
        .take(NOTES_LIST_MAX)
        .collect();
    found.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    found
}

fn missing() -> AppError {
    AppError::new(AppErrorCode::NotFound, NOTE_MISSING)
}

fn find<'a>(scanned: &'a [Scanned], name: &str) -> Option<&'a Scanned> {
    scanned.iter().find(|entry| entry.name == name).or_else(|| {
        scanned
            .iter()
            .find(|entry| entry.name.to_lowercase() == name.to_lowercase())
    })
}

fn write_failed(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::NativeError, NOTE_WRITE_FAILED)
        .with_detail("raw", error.to_string())
}

#[derive(Clone)]
pub struct Notes {
    folders: ConversationFolders,
    clock: SharedClock,
    ids: IdGenerator,
}

impl Notes {
    pub const fn new(folders: ConversationFolders, clock: SharedClock, ids: IdGenerator) -> Self {
        Self {
            folders,
            clock,
            ids,
        }
    }

    pub fn work(&self, conversation: &str) -> Option<PathBuf> {
        self.folders.work(conversation)
    }

    pub fn folder(&self, conversation: &str) -> Option<PathBuf> {
        self.work(conversation).map(|work| work.join(NOTES_FOLDER))
    }

    pub fn scan(&self, conversation: &str) -> Vec<Scanned> {
        self.folder(conversation)
            .map(|folder| scan(&folder))
            .unwrap_or_default()
    }

    pub fn list(&self, conversation: &ConversationId) -> Vec<NoteSummaryDto> {
        self.scan(&conversation.to_string())
            .iter()
            .map(Scanned::summary)
            .collect()
    }

    pub fn read(&self, input: &NoteReadInput) -> Result<NoteDto, AppError> {
        let scanned = self.scan(&input.conversation_id.to_string());
        let found = match (&input.name, input.id) {
            (Some(name), _) => find(&scanned, name),
            (None, Some(id)) => scanned
                .iter()
                .find(|entry| entry.note.as_ref().is_ok_and(|note| note.id == id)),
            (None, None) => None,
        };
        found.ok_or_else(missing)?.dto()
    }

    pub fn save(&self, input: &NoteSaveInput) -> Result<NoteDto, AppError> {
        let conversation = input.conversation_id.to_string();
        let scanned = self.scan(&conversation);
        let entry = find(&scanned, &input.name).ok_or_else(missing)?;
        let current = entry
            .bytes
            .as_deref()
            .ok_or_else(|| refused(NOTE_TOO_LARGE))?;
        if content_hash(current) != input.expected_hash {
            return Err(AppError::new(AppErrorCode::Conflict, NOTE_CONFLICT));
        }
        let format = match decode(current) {
            Decoded::Text { format, .. } => format,
            Decoded::Binary => crate::files::text::TextFormat::default(),
        };
        let taken = |id: &NoteId| {
            scanned.iter().any(|other| {
                other.path != entry.path && other.note.as_ref().is_ok_and(|note| note.id == *id)
            })
        };
        let rendered = writes::prepare(
            &input.content,
            &writes::Prior {
                previous: entry.note.as_ref().ok(),
                existed: true,
                source: None,
            },
            &taken,
            &note_time(self.clock.now()),
            &self.ids,
        )?;
        let bytes = encode(&rendered, format);
        if let Some(work) = self.work(&conversation) {
            within_quota(&work, &[(entry.path.clone(), Some(bytes.len() as u64))])?;
        }
        replace(&entry.path, &bytes).map_err(|error| write_failed(&error))?;
        scan_file(&entry.path, &entry.name)
            .ok_or_else(missing)?
            .dto()
    }

    pub fn rename(&self, input: &NoteRenameInput) -> Result<NoteDto, AppError> {
        let conversation = input.conversation_id.to_string();
        let folder = self.folder(&conversation).ok_or_else(missing)?;
        let scanned = scan(&folder);
        let entry = find(&scanned, &input.name).ok_or_else(missing)?;
        if entry.name == input.to {
            return entry.dto();
        }
        if scanned.iter().any(|other| {
            other.path != entry.path && other.name.to_lowercase() == input.to.to_lowercase()
        }) {
            return Err(AppError::new(AppErrorCode::Conflict, NOTE_NAME_TAKEN));
        }
        let target = folder.join(&input.to);
        std::fs::rename(extended(&entry.path), extended(&target))
            .map_err(|error| write_failed(&error))?;
        scan_file(&target, &input.to).ok_or_else(missing)?.dto()
    }

    pub fn remove(&self, input: &NoteRef) -> Result<(), AppError> {
        let scanned = self.scan(&input.conversation_id.to_string());
        let entry = find(&scanned, &input.name).ok_or_else(missing)?;
        std::fs::remove_file(extended(&entry.path)).map_err(|error| write_failed(&error))
    }
}
