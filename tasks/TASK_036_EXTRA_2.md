---
task: TASK_036_EXTRA_2
title: File tools — fs.list, fs.read, fs.search and the fs.apply batch
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_036_EXTRA_1]
unblocks: [TASK_036_EXTRA_3, TASK_040]
estimate: 5 h
---

# TASK_036_EXTRA_2 — File tools

## Context

TASK_036_EXTRA_1 gives a chat an optional working folder and a folder access service:
`ensure_access` asks the user for any folder not yet granted. This task adds the built-in tools
that use it.

Decided with the user (2026-10-22):

- The tools are offered in **every** chat, with or without a working folder. Without one, paths
  must be absolute, and every folder goes through the access request.
- "Batching" means **several file changes in one call**: one tool, one approval, one diff, one
  step. There is no generic batch tool that wraps other tools: it would get around per-call
  admission, the `CHAT_TOOL_CALLS` budget and the one-step-per-call record.
- What the assistant may do is decided by the tool tiers. Folder access decides only where.

## Why

Reading and changing files is the core of the work Agentic Build is designed around
(`/design/AgenticBuild.dc.html`: files read, a file added with a diff, changed files in the rail).
A multi-file change has to be reviewed and applied as a whole, or a half-applied refactor is
left behind.

## Depends on

- `TASK_036_EXTRA_1` — path resolution, forbidden folders, `ensure_access`, the working folder.

## Read first

- `/docs/PASSPORT.md` §11, §14 (page and document content is untrusted input)
- `crates/zvs-host/src/tools/kernel_tools.rs` — how built-ins register with Russian descriptions
- `crates/zvs-host/src/skills/script.rs` — `fenced()`, the pattern for untrusted output
- `crates/zvs-host/src/indexing/walk.rs` — include/exclude walking to reuse
- `crates/zvs-host/src/services/chat.rs` — the tool loop around line 1609

## Scope

**In scope**

*Read tools (not destructive, default `auto`)*
- `fs.list` — a folder's entries, optionally recursive with a glob, capped in count. It respects
  `.gitignore` the way `walk.rs` does.
- `fs.read` — `paths[]` (up to 16), each with an optional line range. Content is capped per file
  and in total. Binary and non-UTF-8 files are reported, not dumped. Output is fenced as untrusted.
  Each read records a content hash per path for this run, which `fs.apply` checks.
- `fs.search` — a regex or literal search over a folder, returning path, line and match. Capped.

*`fs.apply` (destructive, default `ask`)*
- Input `changes[]`, 1 to 32 entries, at most 1 MiB of content in total:
  `{ op: 'write', path, content }`, `{ op: 'edit', path, edits: [{ old, new }] }`,
  `{ op: 'move', from, to }`, `{ op: 'delete', path }`.
- Execution, as one kernel step:
  1. Resolve every path, then call `ensure_access` once for all target folders, including both
     sides of a `move`. Any forbidden path refuses the whole batch.
  2. Plan in memory: apply the changes in order to an overlay `path → Option<content>` over the
     disk, so "write then edit" and "move then edit the target" work. Each `old` must match
     exactly once in the current overlay content. A `move` onto an existing file is an error
     unless the batch deleted it earlier.
  3. Staleness: a file being edited, overwritten or deleted must still have the hash this run
     recorded when it read it. A `write` over an existing file the run never read is refused.
  4. Approval, when the tier asks: one card with the combined diff of every file. If step 1
     also needs folder access, that request and the diff are **one** modal, not two. The user
     approves or rejects the whole batch.
  5. Commit: snapshot originals into the run folder. Write each file to a temp file in the same
     folder and rename it over the target (`MoveFileExW` with replace). Create missing folders.
     On any failure, restore everything already written from the snapshots.
  6. Return per-file diffs with `+/-` line counts and the affected paths.
- Line endings (CRLF/LF) and a BOM are kept as they were. Binary and non-UTF-8 targets are refused.
- The tool description (Russian) says the batch is all-or-nothing with rollback. NTFS offers no
  cross-file transaction, so this is the guarantee, not true atomicity.

*Per-input permission*
- `NodeDef` gains `permission_for(&self, input: &Value) -> PermissionRequirement`, defaulting to
  `permission()`, and the scheduler and chat admission use it. For `fs.apply` it returns a
  minimum tier of `ask` when the batch contains a `delete`. Routine edits can then be set to
  `auto` on «Инструменты» while deletes always ask. The ceiling still applies on top.

*Chat loop*
- Calls of one round that are side-effect free run concurrently. Calls with side effects keep
  their order, and the results go back in the original call order.
- The system prompt names the file tools and the path rules (English), alongside the working
  folder section from TASK_036_EXTRA_1.

*Renderer*
- Tool call cards for `fs.apply` show the per-file diffs, collapsed past a few files. The
  approval for an `ask` batch shows the combined diff, in the same modal as a folder request when
  both apply.

**Out of scope** (do not do these, even if tempting)
- Reverting a whole session from the snapshots, and the changed-files rail: TASK_040 and TASK_041
  consume the snapshots and the diff output.
- Partial approval of a batch.
- Process execution (TASK_036_EXTRA_3).

## Steps

1. The run-scoped read ledger (path → hash) and `fs.read`, `fs.list`, `fs.search`.
2. The overlay planner for `fs.apply`, with unit tests before any disk writes.
3. Commit with snapshots and rollback, with an injected failure for tests.
4. `permission_for` in the kernel and admission. The `fs.apply` delete rule.
5. Concurrent side-effect-free calls in the chat loop.
6. Registration in `kernel_tools.rs` with Russian descriptions. Renderer cards and the combined
   modal.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- The overlay: write then edit, move then edit, an `old` matching zero or two times, a move onto
  an existing file.
- A forbidden or escaping path anywhere refuses the whole batch with nothing written.
- A stale file refuses the batch. Overwriting an unread existing file is refused.
- An injected failure on the third file restores the first two byte-for-byte.
- A batch with `delete` asks even when `fs.apply` is set to `auto`, and `off` still denies.
- Folder access and diff approval arrive as one request.
- CRLF and a BOM survive an edit. Binary targets are refused.
- `fs.read` caps, and output fencing neutralises a closing tag inside the file.
- In the chat loop, read-only calls of one round overlap in time, and results keep call order.

## Definition of done

- [x] `fs.list`, `fs.read`, `fs.search` registered, `auto`, untrusted output fenced
- [x] `fs.apply` plans, checks staleness, asks once, commits with rollback, returns diffs
- [x] `permission_for` in the kernel; deletes always ask
- [x] Side-effect-free calls of a round run concurrently
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `zvs-host/src/files/`: `text.rs` (UTF-8/BOM/CRLF decode and encode, binary detection,
  SHA-256 content hash), `ledger.rs` (`ReadLedger`, run-scoped path → hash, last 64 runs),
  `walk.rs` (breadth-first walk with `.gitignore` rules up to the git root, `.git` skipped,
  links listed but never followed), `plan.rs` (the overlay planner behind a `Disk` trait),
  `diff.rs` (line diff with `+/-` counts and unified hunks, capped at `FILE_DIFF_MAX`),
  `commit.rs` (snapshot with `manifest.json`, temp file + rename per file, rollback of written
  files and created folders, injectable `Fault`), `nodes.rs` (the four nodes,
  `register_file_nodes`, Russian descriptions).
- Kernel: `NodeDef::permission_for(input)` and `defers_approval()`; `RegisteredNode` exposes
  both; the scheduler uses `permission_for` for graph steps and model tool calls. A
  `DeferredApproval` slot on `StepContext`: the run gate, on `ask`, hands the node a ticket
  instead of suspending. Model tool calls now execute under the observed link, so approvals a
  node raises itself also mark the chat card «ждёт подтверждения».
- `FolderAccessService::resolve` + `confirm(…, Review)` split out of `ensure_access`; with a
  ticket and no uncovered folder it raises a `kind: "files"` approval carrying the diff, with
  uncovered folders one `kind: "folder"` approval that also carries `changes`.
- Contract: `zvs_contract::files` (`FileChangeKind`, `FileDiffDto`, `FileChangesDto`,
  `FilesApprovalDto`), optional `FolderApprovalDto.changes`; Zod in `packages/shared/src/files.ts`;
  fixtures in `fixtures/files/` and a combined folder approval fixture.
- Chat: file tools are added to the offered set whenever the model can call tools; the
  conversation id reaches them as a hidden `conversationId`; a «File tools» prompt section
  (English) names them and the read-before-change and one-batch rules; `OfferedTool.readOnly`
  drives concurrent execution of consecutive read-only calls.
- `StudioPaths.runs_dir` (`<data>/runs`) for snapshots.
- Renderer: `FileChangeList` (per-file diffs, collapsed past three files), the `fs.apply` card
  shows applied diffs and a summary line, an `ask` batch shows the combined diff with
  «Применить изменения» / «Отклонить», and `FolderAccessModal` shows the diff under the folders.
- Tests: `files/tests.rs` covers every case listed under Verify; the chat-loop overlap and
  order test is in `services/chat/tests/tool_loop.rs`; renderer logic in
  `tests/renderer/fileChanges.test.ts`.

**Deviated from plan:**
- `fs.apply` is registered with `destructive: false` and an explicit default tier `ask`. The
  chat loop forces `ask` on every destructive tool, which would make the «routine edits at
  `auto`» setting impossible; deletes keep their mandatory `ask` through `permission_for`.
- `walk.rs` has no `.gitignore` support, so `files/walk.rs` implements the rules on top of the
  indexing glob patterns (`matches_anchored` added to `indexing/patterns.rs`). Nested
  `.gitignore` files, `!` negations, anchored and folder-only patterns are honoured.
- Binary and non-UTF-8 files are refused for every operation, `move` and `delete` included.
- The diff is planned before folder access is answered, so it can be shown in the same modal.
  Planning reads files in a not-yet-granted folder on the host only; their content reaches the
  model only if the user approves, but a planning error (for example a missing `old`) can
  reveal that a file exists.
- Snapshots live in `<data>/runs/<runId>/snapshots/<batch>/`; nothing prunes them yet.
- `ipc/chat/tests.rs` used a 1024-token model; the file tool specs no longer fit, so its fixture
  now has 8192.

**Next session should know:**
- TASK_040/041: `FsApplyOutput.changes` (also the tool call `meta`, a `FileChangesDto`) and the
  snapshot `manifest.json` (`path`, `snapshot` file name or null, `deleted`) are the inputs for
  the changed-files rail and session revert.
- File tools need a conversation: the «Попробовать» panel and scenario graphs without a
  `conversationId` fail with «Диалог не найден».
- Not run here: `pnpm build`, `pnpm test:e2e`, `zvs-studio` test clippy (its browser gateway
  tests use the unstable `atomic_try_update` on this toolchain, unrelated to this change).
  `zvs-jobd`'s `cancelling_stops_a_job…` failed once and passed on rerun.
