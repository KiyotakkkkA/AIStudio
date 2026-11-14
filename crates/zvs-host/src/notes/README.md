# notes

Dialog notes: named Markdown files under `chat:notes/` (physically
`conversations/<id>/work/notes/`) that keep substantial, reusable knowledge for one dialog.
The module root is `../notes.rs`; this folder holds the write path and the tests. Nothing here
depends on Tauri, and the clock and the id generator are injected.

## Files

| File          | Contents                                                                                           |
| ------------- | -------------------------------------------------------------------------------------------------- |
| `../notes.rs` | Format (`parse_draft`, `parse`, `render`), `note_name`, `scan`, the `Notes` service for the UI IPC |
| `writes.rs`   | `prepare` (host-managed identity and dates) and `check_plan` for `fs.apply`                        |
| `tests.rs`    | Format, limits, identity rules, scanning and the service                                           |

## Format

```markdown
---
schema_version: 1
id: 0199aa11-2222-7111-8111-000000000001
title: Background process shutdown behavior
created_at: 2026-11-14T12:00:00Z
updated_at: 2026-11-14T12:00:00Z
context: How the knowledge was obtained.
read_when: Before changing how background processes stop.
update_when: When the launcher changes.
delete_when: After the mechanism is replaced.
---

# Body
```

`parse_draft` accepts what the model writes: frontmatter between `---` lines, the five
person-facing fields (trimmed, required, limits from `zvs_contract::notes`), a non-empty body,
and optional managed fields. YAML goes through `serde_yaml_ng` into a mapping, so duplicate keys
fail; unknown keys, non-scalar values and a `schema_version` other than 1 are refused. `parse`
additionally requires every managed field: a valid `id` and UTC dates (`is_note_time`). `render`
writes the canonical form — keys in a fixed order, then a blank line and the body — and is
stable: rendering a parsed rendering gives the same text. Errors are `VALIDATION_FAILED` with a
Russian message and, where it applies, a `field` detail; `describe` joins the two for lists.

## Identity (`writes::prepare`)

| Field        | New note  | Edit of a valid note                               | Move target (same id deleted in the batch) | Repaired invalid file              |
| ------------ | --------- | -------------------------------------------------- | ------------------------------------------ | ---------------------------------- |
| `id`         | generated | kept                                               | inherited                                  | its own, if well formed and unique |
| `created_at` | now       | kept                                               | inherited                                  | its own, if well formed            |
| `updated_at` | now       | now only if a field or the body changed, else kept | same rule against the source               | now                                |

The model can never choose the id of a new note, and an id already used by another note in the
folder is replaced.

## `fs.apply` (`writes::check_plan`)

Runs on the plan of every batch, for each chat base it touches, before the quota check and the
approval. Planned files whose first component is `notes` are in the namespace: a write of a
nested path, a non-Markdown name or a `Notes` spelling fails with `NOTE_BAD_PATH`, a create whose
name differs only in case from another note on disk fails with `NOTE_NAME_TAKEN`, and every
written note is decoded, prepared and re-encoded with its original line endings; the diff of
that file is recomputed (`plan::replace_after`). Deletes are not checked. One invalid note fails
the whole batch, and the error carries the path.

## Scanning

`scan(folder)` reads the folder fresh on every call: no cache, no index. A missing folder is an
empty list and is not created. Entries whose names are not note names are ignored; files over
128 KiB are listed as invalid without being read. An invalid file stays in the list with its
error (`Scanned::summary`), is left out of the prompt directory, and is never rewritten.

## Service (`Notes`)

`list`, `read` (by name, case-insensitively, or by id), `save` (refuses with `CONFLICT` when the
file's SHA-256 differs from the hash the editor loaded, then validates like `fs.apply` and
checks the dialog quota), `rename` (refuses another note's name, case-insensitively; keeps the
content) and `remove` (final). A missing note is `NOT_FOUND`.
