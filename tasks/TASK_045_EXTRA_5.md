---
task: TASK_045_EXTRA_5
title: Storage — lean run events, one folder per conversation, disk usage by category
phase: 8 — Documents and the agent browser
status: todo
depends_on: [TASK_045, TASK_045_EXTRA_3]
unblocks: []
estimate: 6 h
---

# TASK_045_EXTRA_5 — Storage: lean events, per-conversation folders, usage

## Context

Measured on a real data folder (`%APPDATA%\com.zvs.aistudio`, 2026-10-25): 616 MB in total,
of which `mcp-toolchain` 317 MB, `browser` 274 MB, `studio.sqlite` 11 MB plus a 4 MB WAL,
`backups` 6.4 MB, `cache` 2.5 MB, `logs` 556 KB, `skills` 546 KB, `dashboards` 100 KB,
`artifacts` 30 KB. The WebView profile adds 303 MB in `%LOCALAPPDATA%`.

**The database is mostly event log.** 18 messages hold 20.7 KB of text; `run_event` holds
4,331 rows and 6.2 MB:

- Tool-call progress: each status change emits a `step` event with *all* calls of the turn,
  results included. 229 rows, 14.4 KB average, 51.6 KB max, 3.2 MB in total, against 71 KB
  in the final `message.tool_calls`: 45× amplification.
- Step outputs travel inside events (`prepare` about 9 KB each, carrying the whole
  generation request), 2.3 MB, and are stored again in the `step` table (1.7 MB).
- Every token is one transaction, one `run_event` row, and a rewrite of the whole message
  content: `ChatService::record` (`services/chat.rs`) reads the message and writes
  `format!("{}{delta}", message.content)`, so a reply costs O(n²) bytes written.

**Chat data is spread over five places,** with gaps in cleanup:

| Place | Holds | Cleaned by |
| --- | --- | --- |
| `artifacts/chats/<conversationId>/` | chat working folder (`chat_files.rs`) | conversation removal |
| `artifacts/<runId>/` | copies of skill outputs (`ArtifactStore::keep`, `std::fs::copy`) | nothing |
| `runs/<run>/snapshots/` | `fs.apply` restore snapshots (`files/nodes.rs`) | nothing |
| `cache/{skill-scripts,code-runs,documents}` | scratch per run | `SCRATCH_KEEP` (last 16) |
| `dashboards/` | dashboard pages (a library) | user |

Retention (`services/retention.rs`) drops database payloads past 30 days but no files.
Skill outputs exist twice: in the chat folder and as the copy under `artifacts/<runId>/`.
Nothing measures disk usage.

For comparison: Claude Code keeps one JSONL per session under `~/.claude/projects/<slug>/`
plus `file-history/`; Codex keeps `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` with a
SQLite index. In both, the session is the unit of storage and the database is an index.

## Why

Most of the database is redundant event data, and every streamed token costs a transaction.
Spread-out folders leave orphans and make "how much space does this chat use" unanswerable.

## Depends on

- `TASK_045` — base task of this extra (backlog placement only).
- `TASK_045_EXTRA_3` — chat reattach replays events; lean events must still replay.

## Read first

- `/docs/PASSPORT.md` §9 (data layer), §11 (kernel, events), §7 (event channel) — binding
- INDEX.md decision TASK_026 (retention) and TASK_035 (skills live as folders)
- `crates/zvs-host/src/kernel/stream.rs`, `services/chat.rs` (`record`, `save_tool_calls`)
- `crates/zvs-host/src/data/repositories/run.rs`, `services/retention.rs`
- `crates/zvs-host/src/platform/paths.rs`, `artifacts.rs`, `chat_files.rs`, `files/nodes.rs`
- `apps/studio/src-tauri/src/lib.rs` (where each root is wired)

## Scope

**In scope — part A, events (do first)**
- Tool-call progress as per-call delta events (one call, its status, a result preview); the
  full results stay in `message.tool_calls`. Renderer and agentic replay apply deltas.
- Token coalescing: tokens still stream to the renderer one by one, but the message row is
  written at most every 250 ms (and at the end), and token events are persisted coalesced.
- Step outputs no longer copied into `step` events; the `step` table keeps them.
- Replay (`runs.detail`, chat reattach, `AgenticStore`) still rebuilds the same state; tests
  prove it on a recorded run.

**In scope — part B, layout and usage**
- One folder per conversation (chat and agentic):
  `conversations/<id>/{work,outputs,snapshots}`. `work` replaces
  `artifacts/chats/<id>`; `outputs/manifest.json` records outputs by reference instead of
  copying them; `snapshots` replaces `runs/<run>/snapshots`.
- Deleting a conversation removes its folder; retention removes snapshots and outputs past
  the retention window for finished runs.
- A migration moves existing folders into the new layout and rewrites artifact ids that
  point at `artifacts/<runId>/…`; old ids keep resolving for one release.
- A `storage.usage` channel: bytes per category (conversations, dashboards, skills,
  vectors, downloads, toolchain, browser, cache, logs, backups, database), cached and
  refreshed incrementally or on demand; a «Хранилище» section in preferences with sizes and
  a clear-cache action. Russian copy, counts through `plural()`, sizes formatted for `ru-RU`.

**Out of scope** (do not do these, even if tempting)
- Moving the run event log out of SQLite into per-run JSONL files (see the decision).
- Touching `mcp-toolchain`, `browser` or the WebView profile contents.
- Changing the retention period or the «Очистить завершённые» rules (TASK_026).

## ❓ DECISION

Ask the user before implementing part B, and record the answer in INDEX.md's decision table:

> Existing chat folders and artifacts move to `conversations/<id>/…`: (a) migrate on first
> start of the new version, keeping old artifact ids resolvable for one release; (b) new
> layout for new conversations only, old ones stay where they are; (c) skip part B, do only
> part A (events) and the usage page.

Fallback if unanswered: (c). Part A carries most of the measurable gain and moves no user data.

## Steps

1. Part A: record a real chat run with tool calls as a fixture; write the replay test first
   (state after replay equals state after live events); then change the event shapes.
2. Coalesce message writes in `record`; test with `createFakeClock()` equivalents on the
   host clock that a 400-token reply makes at most a bounded number of writes.
3. Measure on the fixture: `run_event` bytes and row count before and after; record in
   Handoff next to the audit numbers above.
4. Part B per the decision: paths in `paths.rs`, the stores, the migration with tests over a
   temp directory, cleanup on removal and retention.
5. `storage.usage`: DTO in Rust, Zod and fixture; service with tests over a temp directory;
   the preferences section.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: run a chat turn with several tool calls and a dashboard build, then check the
event row count in `run_event`, reopen the chat mid-turn (reattach still works), delete the
conversation and confirm its folder is gone, and open «Хранилище».

## Definition of done

- [ ] Tool-call deltas, coalesced tokens, no step outputs in events; replay tests pass
- [ ] Event bytes before / after recorded in Handoff
- [ ] Decision recorded; part B done or explicitly skipped
- [ ] No orphaned folders after conversation removal or retention
- [ ] `storage.usage` and the «Хранилище» section; contract in three places
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
