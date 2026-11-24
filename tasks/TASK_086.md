---
task: TASK_086
title: Move Desktop dialogs to JSONL and rebuild the layout at boot
phase: C — Shared dialogs and standalone CLI
status: done
depends_on: [TASK_045_EXTRA_5, TASK_085]
unblocks: [TASK_087, TASK_088]
estimate: 5–8 engineer-weeks
---

# TASK_086 — JSONL dialog storage and boot rebuild

## Context

Studio stores regular dialog metadata, messages, tool calls and compactions in SQLite. A
dialog already has a directory for attachments, notes, work files, outputs and snapshots.
The Desktop host should move regular dialogs to versioned JSONL in those directories and
retain a fast, rebuildable index. The repository must be usable by the standalone CLI.

This is a storage cutover. The `project_struct` 1 → 2 boot step converts supported SQLite
dialogs to JSONL while keeping their IDs and owned files. Boot rebuilds indexes from JSONL.
Unsupported legacy dialogs leave the active layout. The database backup policy preserves the
legacy database, and files owned by unsupported dialogs move to the backup area.

## Why

Both apps need to open and continue the same dialog by ID. A single durable history per dialog
eliminates synchronization between two canonical stores and lets either app resume locally.

## Depends on

- `TASK_045_EXTRA_5` — per-dialog folders and ownership rules.
- `TASK_085` — Markdown notes in the dialog work directory.

## Read first

- `AGENTS.md`, `CONTRIBUTING.md`, `CONVENTIONS.md`, `docs/PASSPORT.md` and `docs/FILES_STRUCTURE.md`.
- `crates/zvs-host/src/platform/paths.rs`, the Desktop startup and layout recovery code, and `crates/zvs-host/src/data/README.md`.
- `crates/zvs-host/src/data/repositories/{chat,run,data_transfer}.rs`, dialog services, and migrations for dialog-scoped permissions and memory.

## Scope

**In scope**

- A Tauri-free dialog repository with stable IDs and a versioned JSONL format. Persist the metadata required to rebuild listings and resume a dialog, along with messages, tool calls, usage, sources, edits, truncations and compactions. Keep attachments and other owned files under their existing dialog-relative paths.
- Derived SQLite indexes for dialog lookup, workspace and pin ordering, activity, search and paginated history. Normal listing and recent-history reads use indexes. Make any metadata stored only for indexing recoverable from the dialog folder. Publish index updates after the corresponding durable write and repair a stale or missing index on boot.
- One OS-backed writer lock per dialog with concurrent reads of committed history. Coordinate edits, branching and deletion with active turns. Recover an interrupted append without discarding earlier committed records or affecting other dialogs.
- Desktop cutover through chat, history, search, import/export and owned-resource cleanup. Adapt dialog references used by permissions, memory and run linking. Preserve current-format export/import and round trips with dialog files.
- A versioned boot step stages and validates the new layout, publishes it idempotently, and resumes or recovers after interruption. Rebuild derived data from JSONL and preserve supported non-dialog state under its existing migration rules.
- Coordinate factory reset with processes using the state root. Active hosts hold a shared OS lock and reset takes the exclusive lock before deleting state. The lock lives outside the reset target so it retains its identity across reset. Keep a blocked reset pending for the next start.

**Out of scope**

- Reading or dual-writing legacy SQLite dialogs after boot conversion, and importing legacy dialog data in older transfer bundles. The version 2 layout uses the existing query layer over rebuilt projections.
- Migrating agentic-session transcripts, the execution audit log or unrelated SQLite stores. Desktop renaming and shared roots belong to `TASK_087`.

## Steps

1. Inventory chat queries and transaction boundaries. Document the record format, index, dialog lifecycle and layout version in the Passport and file-structure docs.
2. Implement the repository, durable writes and index reconciliation. Test competing writers, incomplete tails and stale indexes.
3. Wire Desktop through the repository. Update dialog-scoped references and cleanup, import/export and renderer pagination where required.
4. Add the boot cutover and recovery tests for clean, legacy, interrupted and already-upgraded starts. Exercise the existing Windows app and benchmark listings, search and first-page loads with representative large histories.

## Cutover boundaries

- A chat turn owns the dialog writer lock through its final commit or cancellation. Message
  inserts, text deltas, tool-call settlements and compaction each publish the index only after
  their history commit. Keep SQLite write transactions short so unrelated dialogs can proceed.
- Branching takes the source lifecycle lock and stages the new dialog's history and owned
  files before publishing the new ID. Deletion publishes a tombstone before cleaning files,
  memory and grants, so boot can finish an interrupted cleanup.
- Run records stay in `studio.sqlite`. The dialog history retains the run IDs needed to
  deduplicate activity across branches. Index repair reconciles dialog references without
  dropping memory or permission records for dialogs that are still present.
- Transfer restore reserves fresh IDs and stages histories with the owned files. Extend the
  transfer journal to recover incomplete publication across dialogs and non-dialog records.
  The boot cutover uses the database backup policy before dropping legacy dialog tables.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check && pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check on Windows: create dialogs, continue a long dialog after restart, edit and branch it, then remove the index and confirm that boot restores its history and listing. Boot an existing version 1 layout and confirm that supported dialogs keep their IDs and history, unsupported dialogs are absent, and other settings remain available.

## Definition of done

- [x] JSONL is the only durable regular-dialog history and Desktop uses it through the shared repository.
- [x] The 1 → 2 boot step converts supported SQLite dialogs and resumes after interruption. Unsupported legacy history leaves the active layout, and the recovery backup preserves the source.
- [x] Clean, legacy and interrupted boot checks pass. Rebuilding the index preserves current dialog state.
- [x] Factory reset preserves the pending marker and dialog files while another host holds the state root, then succeeds after that host closes.
- [x] History, attachments, notes, branching, deletion, search, permissions and current-format export/import pass regression checks. Record measured timings and fixture sizes in Handoff.
- [x] Verification passes, Handoff is filled in, status is set to `done` and `INDEX.md` is ticked.

## Handoff

**Built:** Desktop writes regular dialog history to version 1 JSONL and reads its SQLite
projection. The layout 1 → 2 upgrade normalizes a private snapshot of the legacy SQLite
database, stages JSONL, and validates it before publication. The live database migration
backs up the original and replaces obsolete history tables. Boot rebuilds the projection
before constructing services. Scoped grants and memory use the projected dialog IDs.
Run links survive history replay. The dialog list searches
message text, and the thread fetches older messages by cursor while keeping pins available.
Version 2 transfer round trips use the new history, and version 1 dialog bundles are rejected.
Factory reset leaves its marker and files in place while another host holds the root.

**Deviated from plan:** The approved reset coordination and conversion of supported legacy
SQLite dialogs are included in TASK_086.

**Projection timings:** Windows, SQLite 3.50.4, 50 dialogs and 100,000 messages of about
480 bytes each. The Desktop projection occupied 91,422,720 bytes. Over 50 warm runs, median
listing was 0.130 ms, a 50-message first page was 0.225 ms, and a search returning 50 dialog
IDs was 14.800 ms. These are direct SQLite measurements.

**Next session should know:** `history.jsonl` is the authoritative history. The regular dialog
tables and search index in `studio.sqlite` are rebuildable. State-root locks live beside the
root and per-dialog writer locks live outside owned dialog folders. Rebuild runs after transfer
journal recovery, so incomplete imports are cleaned before replay.

**Verification:** TypeScript type checking, lint, formatting and 3,254 tests passed. Rust
formatting, workspace Clippy, 2,606 Rust tests and the Windows release build passed. Isolated
Windows Desktop smoke tests covered clean boot, restart with a 122-message history, rebuilding a dropped
projection table, truncation and continuation, branching, and search. The version 1 fixture
kept the original dialog and message IDs, owned files, and settings after conversion. The
unsupported dialog left the active layout and its files and source database were backed up.
