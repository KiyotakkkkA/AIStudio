use std::path::{Path, PathBuf};

use zvs_contract::{AppError, AppErrorCode, NOTES_FOLDER, NoteId};

use super::{
    NOTE_BAD_PATH, NOTE_BINARY, NOTE_NAME_TAKEN, Note, is_note_time, note_at, note_time, parse,
    parse_draft, render, scan, text_of, work_folder,
};
use crate::files::plan::{Plan, replace_after};
use crate::files::text::{Decoded, decode, encode};
use crate::platform::clock::SharedClock;
use crate::platform::ids::IdGenerator;

pub struct Prior<'a> {
    pub previous: Option<&'a Note>,
    pub existed: bool,
    pub source: Option<&'a Note>,
}

pub fn prepare(
    text: &str,
    prior: &Prior<'_>,
    taken: &dyn Fn(&NoteId) -> bool,
    now: &str,
    ids: &IdGenerator,
) -> Result<String, AppError> {
    text_of(text.as_bytes())?;
    let draft = parse_draft(text)?;
    let baseline = prior.previous.or(prior.source);
    let id = baseline
        .map(|note| note.id)
        .or_else(|| {
            draft
                .id
                .as_deref()
                .filter(|_| prior.existed)
                .and_then(NoteId::parse)
                .filter(|id| !taken(id))
        })
        .unwrap_or_else(|| NoteId::new(ids()));
    let created_at = baseline
        .map(|note| note.created_at.clone())
        .or_else(|| {
            draft
                .created_at
                .clone()
                .filter(|value| prior.existed && is_note_time(value))
        })
        .unwrap_or_else(|| now.to_owned());
    let updated_at = match baseline {
        Some(note) if note.fields.same(&draft.fields) => note.updated_at.clone(),
        _ => now.to_owned(),
    };
    render(&Note {
        id,
        created_at,
        updated_at,
        fields: draft.fields,
    })
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn located(message: &str, path: &Path) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
        .with_detail("path", path.to_string_lossy().into_owned())
}

fn with_path(error: AppError, path: &Path) -> AppError {
    error.with_detail("path", path.to_string_lossy().into_owned())
}

pub fn check_plan(
    plan: &mut Plan,
    bases: &[PathBuf],
    clock: &SharedClock,
    ids: &IdGenerator,
) -> Result<(), AppError> {
    let now = note_time(clock.now());
    for base in bases {
        let targets: Vec<(usize, Option<String>)> = plan
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| note_at(base, &file.path).map(|name| (index, name)))
            .collect();
        if targets.is_empty() {
            continue;
        }
        let on_disk = scan(&work_folder(base).join(NOTES_FOLDER));
        let deleted: Vec<String> = plan
            .files
            .iter()
            .filter(|file| file.after.is_none())
            .map(|file| key(&file.path))
            .collect();
        let sources: Vec<Note> = plan
            .files
            .iter()
            .filter(|file| file.after.is_none())
            .filter_map(|file| file.before.as_deref().and_then(|bytes| parse(bytes).ok()))
            .collect();
        let mut assigned: Vec<(String, NoteId)> = Vec::new();
        for (index, name) in targets {
            let file = &plan.files[index];
            let Some(after) = file.after.as_deref() else {
                continue;
            };
            let path = file.path.clone();
            let Some(name) = name else {
                return Err(located(NOTE_BAD_PATH, &path));
            };
            if file.before.is_none()
                && on_disk.iter().any(|entry| {
                    entry.name != name
                        && entry.name.to_lowercase() == name.to_lowercase()
                        && !deleted.contains(&key(&entry.path))
                })
            {
                return Err(located(NOTE_NAME_TAKEN, &path));
            }
            let (text, format) = match decode(after) {
                Decoded::Text { text, format } => (text, format),
                Decoded::Binary => return Err(located(NOTE_BINARY, &path)),
            };
            let previous = file.before.as_deref().and_then(|bytes| parse(bytes).ok());
            let source = if previous.is_none() {
                parse_draft(&text)
                    .ok()
                    .and_then(|draft| draft.id)
                    .and_then(|id| sources.iter().find(|note| note.id.to_string() == id))
            } else {
                None
            };
            let own = key(&path);
            let taken = |id: &NoteId| {
                on_disk.iter().any(|entry| {
                    key(&entry.path) != own
                        && !deleted.contains(&key(&entry.path))
                        && entry.note.as_ref().is_ok_and(|note| note.id == *id)
                }) || assigned
                    .iter()
                    .any(|(other, known)| *other != own && known == id)
            };
            let rendered = prepare(
                &text,
                &Prior {
                    previous: previous.as_ref(),
                    existed: file.before.is_some(),
                    source,
                },
                &taken,
                &now,
                ids,
            )
            .map_err(|error| with_path(error, &path))?;
            if let Ok(note) = parse(rendered.as_bytes()) {
                assigned.push((own, note.id));
            }
            replace_after(plan, index, encode(&rendered, format));
        }
    }
    Ok(())
}
