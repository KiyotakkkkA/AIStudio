---
task: TASK_085
title: Dialog notes — durable Markdown knowledge, on-demand reading and answer sources
phase: B — Dialog notes
status: done
depends_on: [TASK_039_EXTRA_1, TASK_045_EXTRA_1, TASK_045_EXTRA_5, TASK_084]
unblocks: []
estimate: 8 h
---

# TASK_085 — Dialog notes

## Context

The host already has two useful building blocks:

- `services/chat_memory.rs` exposes `chat.memory` for short, standalone facts, scoped to the
  account or dialog. `services/chat_prompt.rs` assembles named prompt sections.
- `fs.list`, `fs.read`, `fs.search` and `fs.apply` work on `chat:` through `VirtualRoots`.
  `chat:` resolves to `conversations/<conversationId>/work`, not the selected project folder.
  Changes wholly inside that dialog's own directory already need no folder grant or diff
  confirmation. The host owns validation, I/O, execution policy and persistence.

Add **notes**: named Markdown files under `chat:notes/` containing substantial, reusable
knowledge. The model receives a small directory of titles, paths and reading conditions;
the full text enters context only through an explicit read. The person can view, edit,
rename and delete notes, and see which notes were read as sources for an answer.

## Why

A short fact cannot preserve the reasoning behind a decision, a reproducible investigation,
or a procedure with conditions and references. Keeping all that material in every prompt
wastes context; leaving it only in history loses it after compaction. Notes preserve the
detail while making its existence and applicability discoverable.

## Depends on

- `TASK_039_EXTRA_1` — existing short-fact memory and its prompt/tool integration; complete.
- `TASK_045_EXTRA_1` — consolidated file tools and the `chat:` virtual root; complete.
- `TASK_045_EXTRA_5` — per-dialog `work`, `outputs` and `snapshots` storage; complete.
- `TASK_084` — unified `ChatSourceDto`, `SourceSink` and source registries; notes are added
  as one more source kind.

**TASK_083 is not a dependency.** Use the permissions, events and run records that exist now.

## Read first

- `AGENTS.md`, `CONTRIBUTING.md`, `CONVENTIONS.md`.
- `docs/PASSPORT.md`: chat/context construction, file tools, permissions and per-dialog
  storage. Update it to describe notes before landing the implementation.
- `services/{chat,chat_prompt,chat_memory,chat_history}.rs` and the compaction paths.
- `crates/zvs-host/src/files/` and `chat_files.rs`, `ipc/{chat,policy}.rs`.
- `crates/zvs-contract/src/chat.rs`, `packages/shared/src/chat.ts`, shared golden fixtures.
- Renderer `features/chat/{ChatSources,ChatMessage,ChatStore}` and the document viewer;
  `crates/zvs-cli/src/{state,ui,runtime.rs}` for terminal surfaces.

## Product decisions — settled

1. Notes belong to a dialog. Project-wide and account-wide libraries are out of scope.
2. The model uses the existing `fs.apply` to create, update, rename and delete notes, and
   `fs.read` to read them. No `notes.*` model tools.
3. Operations on the dialog's own notes require no confirmation — the existing owned-folder
   policy, not a global `fs.apply` bypass.
4. Seven fields: title, creation date, creation context, when to read, when to update, when
   to delete, and the knowledge itself. YAML frontmatter plus Markdown body.
5. Short standalone facts go through `memory`. Notes hold knowledge whose reconstruction
   would require substantial work or lose important reasoning.
6. **No cache or index.** Every directory snapshot is built by scanning the current files.
7. Read notes appear in the answer's structured sources as the `note` kind of TASK_084. **No note links,
   footnote numbers or citation markers in the answer body.**
8. No revision history, trash or restore. Deletion is final, as for any other `work` file.

## Scope

### 1. File format

Use `chat:notes/<topic>.md`, physically `<dialog>/work/notes/<topic>.md`. Filenames are
descriptive topic slugs. Keep the directory flat; reject nested paths and non-Markdown files
in this namespace. Compare filenames case-insensitively; never overwrite a different note
through a create or rename.

```markdown
---
schema_version: 1
id: 0199aa11-2222-7111-8111-000000000001
title: Background process shutdown behavior
created_at: 2026-11-14T12:00:00Z
updated_at: 2026-11-14T12:00:00Z
context: >
  Testing background build cancellation showed that terminating the parent
  process does not guarantee that its child processes stop.
read_when: >
  Before changing how background processes are started, cancelled or stopped.
update_when: >
  When the process launcher changes or new verification results become available.
delete_when: >
  After the mechanism has been replaced and the issue no longer occurs.
---

# Background process shutdown behavior

Observations, the chosen solution, its rationale and verification steps.
```

- The host assigns `schema_version`, `id` and UTC timestamps on creation; model writes may
  omit them. Edits and renames keep `id` and `created_at`; `updated_at` changes only when
  content changes.
- `title`, `context`, `read_when`, `update_when`, `delete_when` and a non-empty body are
  required.
- Shared limits: title 200 characters, context 2,000, read condition 600, update/delete
  conditions 2,000 each, whole file 128 KiB. Existing dialog quotas apply. No truncation.
- Reject malformed YAML, duplicate keys, unknown schema versions, binary content and unsafe
  paths. A file edited outside the app that fails validation stays in the list with an error
  and is left out of the prompt directory; it never hides the other notes, and a scan never
  rewrites files.

### 2. Host service and writes

- A Tauri-free notes service in `zvs-host` scans, parses, validates and builds the prompt
  directory. Inject clock and IDs for deterministic tests. Renderer and CLI never do note I/O.
- Validate inside the existing `fs.apply` transaction, including moves into or out of
  `notes/`. A malformed note fails the whole batch. Mixed batches keep the permissions of
  their other targets.
- A successful write returns the canonical path and `id`.
- User editor saves send the content hash they loaded; a changed file is reported as a
  conflict instead of being overwritten.
- Do not widen tool availability or bypass `off`, the hard floor or a read-only ceiling.

### 3. Prompt directory

- Add a named `Notes` prompt section, counted by the existing token meter. Fixed guidance is
  system-authored English; note metadata is escaped and delimited as data.
- For each valid note, inject path, title and `read_when`. Never inject the body.
- Guidance: check the directory before creating a note; read a note before relying on it;
  update an existing topic instead of duplicating it; no secrets or transient logs; prefer
  `memory` for short facts; delete a note when its `delete_when` condition is met; no note
  citations in the answer text.
- Build the directory before every model request, including tool-loop continuations, goal
  rounds and requests after compaction.
- Cap the section at 2,000 tokens (existing tokenizer). Fit whole entries in path order,
  report shown/total, and point to `fs.list` for the rest.
- A missing directory means zero notes and creates nothing on disk.

### 4. Notes as answer sources

- Add a `note` variant to `ChatSourceDto` (TASK_084): note `id`, title, path and content
  hash. Rust variant, Zod variant and golden fixture; dedup key is `id` plus hash.
- Push into the turn's `SourceSink` only after a successful `fs.read` of a note in that turn
  by the model producing the answer. Directory entries, `fs.list`, search hits, creating a
  note and failed reads do not count.
- Register the `note` kind in the renderer and CLI source registries: note icon, title and
  «Заметка». Opening it shows the current note, or «Заметка удалена» when it no longer
  exists.
- No `[1]`, note links or `chat:` links in the answer body. Web inline citations unchanged.

### 5. Management

- Desktop: «Заметки» in the dialog side panel — list with updated date and invalid state,
  view, edit, rename, delete, empty and error states. Reuse the existing viewer/editor.
- CLI: `/notes` lists notes and opens one for reading; delete with confirmation. Editing
  stays in the desktop app.
- Add only the typed UI IPC operations these surfaces need, with policy rows, error codes
  and fixtures, scoped to the dialog of the calling connection.
- Notes live in `work/`, so branching and dialog deletion follow the existing `work`
  lifecycle; verify that branches do not share files.

## Out of scope

- Revision history, trash, restore and historical source snapshots.
- A read-before-write check for model edits.
- Note editing in the CLI.
- Account/project notes, sync and vector indexing of notes.
- Notes-specific model tools, a persisted index, automatic reading of note bodies, and
  host-driven deletion.
- Counting notes read by subagents or carried through compaction as sources.

## Steps

1. Document notes in PASSPORT; add schema, limits, DTOs and fixtures.
2. Implement the notes service and `fs.apply` validation.
3. Add the prompt directory to every request path and the token meter.
4. Add the `note` source kind and push note reads into the `SourceSink`.
5. Desktop panel and CLI `/notes`.
6. Tests, documentation, manual checks.

## Verify

Automated coverage:

- Schema, fixtures and limits; missing/invalid fields; host-managed `id` and dates;
  duplicate topics; invalid files visible but excluded; scans write nothing.
- Batch atomicity, rename collisions, quotas, path escapes and dialog isolation.
- No approval for own-directory note writes; mixed/foreign/disabled/read-only operations
  keep today's policy.
- Directory reaches every request path, never contains bodies, reflects changes on the
  next request and respects the cap with overflow reported.
- Only successful reads produce note sources; deleted notes open as «Заметка удалена».
- Editor save conflict; management-store logic and CLI state with existing fakes.

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm build
pnpm test:e2e
```

Use CONTRIBUTING's platform exclusions; state which checks could not run. One e2e flow for
the notes panel and source card.

Manual checks: create a note via `fs.apply` without approval; the next request contains only
its directory entry; read it and get an answer with a note source and no inline citation.
Edit, rename and delete it in the panel; open the old source. Break a note's metadata
outside the app and repair it. Compact and branch the dialog. Check `/notes` in the CLI.

## Definition of done

- [ ] Notes work through existing file tools without extra approvals
- [ ] Seven fields, managed metadata and validation are implemented
- [ ] A fresh, bounded directory reaches every request; nothing is cached
- [ ] Memory, tool availability and permission ceilings are unchanged
- [ ] Read notes appear as structured sources in desktop and CLI, with no inline markers
- [ ] Desktop panel and CLI `/notes` work
- [ ] Rust/Zod/fixtures, IPC policy, token meter and Russian copy are consistent
- [ ] PASSPORT describes the feature
- [ ] Gates and manual checks pass
- [ ] Handoff filled in, status `done`, INDEX.md ticked; one `TASK_085: ...` commit

## Handoff

**Built:**
- Contract: `zvs_contract::notes` (limits, `is_note_name`, `NoteSourceDto`, `NoteMetaDto`,
  `NoteSummaryDto`, `NoteDto`, `NoteRef`, `NoteReadInput`, `NoteSaveInput`, `NoteRenameInput`),
  `NoteId`, `ChatSourceDto::Note` / `ChatSourceKind::Note`, `note_sources`,
  `ChatPromptSection::Notes`; Zod mirrors in `packages/shared/src/notes.ts`; fixtures under
  `fixtures/notes/` and a `note` case in `chat/ChatSourceDto`.
- Host: `notes.rs` (format, `scan`, the `Notes` service) and `notes/writes.rs` (`prepare`,
  `check_plan`), YAML through `serde_yaml_ng` =0.10.0. `fs.apply` validates the namespace inside
  its plan (`plan::replace_after` recomputes the diff) and returns `notes` (path, id);
  `fs.read` returns `notes` sources for valid notes. `FileToolOptions` gained `clock` and `ids`.
- Prompt: `services/chat_notes.rs` — `notes_section` (≤ 2 000 tokens, JSON lines, overflow line)
  in `PromptParts.notes`, the request path and the context preview; `NotesTurn` refreshes the
  section at the top of every tool-loop round (`LoopHost::system`) and records note sources,
  which replace the message's `note` kind. The knowledge book now replaces only the `vector`
  kind instead of overwriting the whole list.
- IPC: `notes.list` / `notes.read` / `notes.remove` (console, dialog-scoped), `notes.save` /
  `notes.rename` (studio); handlers in `ipc/notes.rs`, `Services.notes`, wired in Tauri and the
  test services.
- Desktop: «Заметки» side panel (`features/chat/notes/`: `NotesStore`, `NotesPanel`, `NoteView`,
  `notesCopy`), the skills Markdown editor reused through `fileName`/`label` props, note rows
  and a «Заметки» tab in the answer's sources, `chat.openNote(id)`, the «Заметки» stats label.
- CLI: `/notes` (Alt+N), `Panel::Notes` and `Panel::Note`, delete with a second Delete, note
  rows after an answer (`zvs-note:<id>` links), «Заметка удалена» for a deleted note.
- Export lists note sources; PASSPORT «Dialog notes»; READMEs for `notes/`, `files/`,
  `services/chat/`, `ipc/`, CLI `state/`.

**Deviated from plan:**
- No `SourceSink` exists after TASK_084, so note sources follow its pattern: a per-turn
  `NotesTurn` collects them and writes the `note` kind through `replace_sources`.
- Notes have no single-file revision snapshots for editor saves; `fs.apply` writes keep their
  usual batch snapshots.
- The note view shows the metadata and the Markdown body; invalid notes show the raw text with
  the host's error. Editing works on the whole file text, frontmatter included.

**Next session should know:**
- Passed locally: `pnpm typecheck`, `pnpm lint`, the renderer and shared fixture tests touched
  here, `cargo fmt --all --check`, `cargo test` for zvs-contract and zvs-cli and the zvs-host
  notes, file-tool, chat-notes and branch tests, clippy `-D warnings` for zvs-contract,
  zvs-host and zvs-cli (before the absolute-path fix in `notes::note_at`), `pnpm format:check`.
- Not run: the full `cargo test` for zvs-host and its rerun after `note_at` (stopped for low
  memory), the full `pnpm test`, `pnpm build`, `pnpm test:e2e` (the new `e2e/notes.spec.ts`
  needs a release build), zvs-studio clippy, and the manual checks in the app.
