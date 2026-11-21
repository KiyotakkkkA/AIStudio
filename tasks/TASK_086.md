---
task: TASK_086
title: Move Desktop dialogs to JSONL and rebuild the layout at boot
phase: C — Shared dialogs and standalone CLI
status: todo
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

This is a storage cutover. Boot creates the new versioned layout and rebuilds dialog indexes
from JSONL. Legacy SQLite dialogs are outside the supported history. An existing installation
starts with an empty regular dialog list if it has no JSONL dialogs. Keep the old data under
the boot recovery/backup policy so rebuilding the layout does not destroy the legacy source.

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

**Out of scope**

- Reading, importing, converting or dual-writing legacy SQLite dialogs, including legacy dialog data in older transfer bundles. Exclude active compatibility repositories and legacy dialog tables from the new storage path.
- Migrating agentic-session transcripts, the execution audit log or unrelated SQLite stores. Desktop renaming and shared roots belong to `TASK_087`.

## Steps

1. Inventory chat queries and transaction boundaries. Document the record format, index, dialog lifecycle and layout version in the Passport and file-structure docs.
2. Implement the repository, durable writes and index reconciliation. Test competing writers, incomplete tails and stale indexes.
3. Wire Desktop through the repository. Update dialog-scoped references and cleanup, import/export and renderer pagination where required.
4. Add the boot cutover and recovery tests for clean, legacy, interrupted and already-upgraded starts. Exercise the existing Windows app and benchmark listings, search and first-page loads with representative large histories.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check && pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check on Windows: create dialogs, continue a long dialog after restart, edit and branch it, then remove the index and confirm that boot restores its history and listing. Boot with a legacy SQLite layout and confirm that old dialogs stay outside the new listing and other supported settings remain available.

## Definition of done

- [ ] JSONL is the only durable regular-dialog history and Desktop uses it through the shared repository.
- [ ] Clean, legacy and interrupted boot checks pass. Rebuilding the index preserves current dialog state.
- [ ] History, attachments, notes, branching, deletion, search, permissions and current-format export/import pass regression checks. Record measured timings and fixture sizes in Handoff.
- [ ] Verification passes, Handoff is filled in, status is set to `done` and `INDEX.md` is ticked.

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
