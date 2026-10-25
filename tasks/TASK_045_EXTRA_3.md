---
task: TASK_045_EXTRA_3
title: Chat turn reattach — a reopened chat shows the live turn, its approvals and its questions
phase: 8 — Documents and the agent browser
status: done
depends_on: [TASK_045]
unblocks: []
estimate: 4 h
---

# TASK_045_EXTRA_3 — Chat turn reattach

## Context

A chat turn keeps running in the host when the window is closed or the renderer reloads.
When the chat is opened again the user sees only the stored partial answer, with no live
updates, and a send fails with `ACTIVE_TURN` («В диалоге уже выполняется ход»). If the turn
was waiting for an approval (`fs.apply`, `code.run`, `shell.run`, folder access) or a
question (`chat.ask`), nothing shows it, so the turn waits with no way to answer.

Root cause, from the code:

- `ChatStore.run` (`apps/studio/src/renderer/features/chat/ChatStore.ts`) exists only in
  memory. A new store starts with `run === null`, so `mount()` never calls `subscribe()`.
- `select()` loads `chat.conversations.get` and resets the live state; it never asks the host
  whether a run for this conversation is still live.
- Approvals, questions, tool-call progress and tokens reach the renderer only as stream
  events (`receive`). A renderer that subscribes late, or never, does not see them.
- `EventRouter` (`apps/studio/src/renderer/app/EventRouter.ts`) holds at most 256 events
  for a stream nobody listens to, then drops them.

The host already stores every event of a run in `run_event` (`kernel/stream.rs` →
`runs.append_event`), including `approval` and `question`, and `runs.detail` returns them.
`AgenticStore.sync` (`apps/studio/src/renderer/features/agentic/AgenticStore.ts`) already
solves the same problem for agentic sessions: `runs.list` → `runs.detail` → replay the events
→ `listen(streamId)`, holding live events while syncing and resyncing on a gap.

## Why

A turn that cannot be seen or answered after reopening looks hung and blocks the
conversation. The fix reuses a pattern the codebase already has.

## Depends on

- `TASK_045` — base task of this extra (backlog placement only; no code dependency).

## Read first

- `/docs/PASSPORT.md` §7 (IPC, the event channel D4) and §11 (kernel, approvals) — binding
- `apps/studio/src/renderer/features/chat/ChatStore.ts` (`mount`, `select`, `subscribe`, `receive`)
- `apps/studio/src/renderer/features/agentic/AgenticStore.ts` (`sync`, `drain`, `receive`)
- `apps/studio/src/renderer/app/EventRouter.ts`, `apps/studio/src/renderer/app/replay.ts`
- `crates/zvs-host/src/services/runs.rs` (`detail`), `crates/zvs-host/src/services/chat.rs` (`require_idle`)

## Scope

**In scope**
- A host channel `chat.attach { conversationId }` returning, for a live chat run, the run
  handle (`id`, `streamId`), the last persisted `seq`, the pending approvals and questions,
  the current tool calls and the partial assistant message; `null` when no run is live.
  Rust type, Zod schema and golden fixture together.
- `ChatStore.select()` and `mount()` call it, restore `run`, `liveText`, `liveReasoning`,
  `liveToolCalls`, `approvals`, `folderRequests`, `questions`, set `lastSeq`, then subscribe.
  Live events that arrive during the attach are held and drained, as in `AgenticStore`.
- A gap in the stream after attach triggers a re-attach.
- Approving, denying, answering and stopping work on a reattached turn.
- `ChatStore` shows `"Generation failed. Try again."` when a run fails without a message.
  That is English user-facing copy; replace it with a Russian constant (CONVENTIONS.md).

**Out of scope** (do not do these, even if tempting)
- Resuming a turn whose host process died (the kernel's interrupted-run rules stay).
- Changing how events are stored (TASK_045_EXTRA_5).
- Agentic sessions, which already reattach.

## Steps

1. Host: implement `chat.attach` over `runs.unfinished()` (the same lookup `require_idle`
   uses), the approval registry and `run_event`. Tests: no live run → `null`; a run waiting
   on an approval returns it; a run waiting on a question returns it; a finished run → `null`.
2. Contract: DTO in `crates/zvs-contract`, schema in `packages/shared`, fixture in
   `packages/shared/fixtures/`; `cargo test -p zvs-contract` byte-equal.
3. Renderer: an `attach()` step in `ChatStore` used by `select()` and `mount()`, with
   held-event draining and gap handling. Store tests with `createFakeBridge()` and
   `createFakeClock()`: reopen mid-stream continues the text; reopen with a pending approval
   shows it and `decideApproval` works; reopen with a pending question shows the card;
   a gap re-attaches.
4. Replace the English failure string with an exported Russian constant and assert against
   the constant in tests.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check: start a turn that runs `code.run` (asks for approval), close the window to the
tray before approving, reopen it, open the chat: the approval card shows and approving lets
the turn finish with live tokens. Repeat with a `chat.ask` question.

## Definition of done

- [x] `chat.attach` with tests; contract in three places
- [x] A reopened chat shows the live turn, approvals, folder requests and questions
- [x] A pending approval or question can be answered after reopening
- [x] No English user-facing string left in `ChatStore`
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `chat.attach { conversationId }` → `ChatAttachDto | null` (Rust type, Zod schema, fixtures in
  `fixtures/chat/ChatAttach*`). `ChatService::attach` finds the live chat run the way
  `require_idle` does, replays its `run_event` rows into the text and reasoning (token deltas,
  overridden by a succeeded `persist` step), the latest `tool-calls` snapshot and the approval
  events whose `pending_approval` row is still undecided and unexpired
  (`ApprovalService::is_pending`); questions come from `QuestionService::pending`. `seq` is the
  last persisted seq, `null` before the first event; `startedAt` falls back to `createdAt`.
- Host tests (`tool_loop.rs`): no live turn and a finished turn → `null`; a turn waiting on an
  approval returns its text, calls, approval and `seq`; a turn waiting on a question returns it.
- `ChatStore.select()` (and so `mount()`, which ends in `select`) calls `chat.attach` beside
  `chat.conversations.get`, restores `run`, `runStartedAt`, `liveText`, `liveReasoning`,
  `liveToolCalls`, `approvals`, `folderRequests`, `questions` and `lastSeq`, hides the run's
  stored partial assistant message, then subscribes. Approvals go through one `admitApproval`
  for both live events and the attach payload.
- Gaps: a router-flagged gap, or a first event after an attach that is not `seq + 1`, holds the
  event and re-attaches; events arriving meanwhile are held and drained through the seq filter.
  A run that is gone after the re-attach ends locally and the conversation reloads. An `end`
  event never re-attaches.
- `GENERATION_FAILED` replaces the English failure string; tests assert the constant.

**Deviated from plan:**
- Store tests use `vi.useFakeTimers()` like the existing chat tests: `ChatStore` has no clock
  dependency, its token flush is a plain `setTimeout`, so `createFakeClock()` has nothing to
  drive.
- The initial attach does not hold events: the store is not subscribed yet, and the router keeps
  up to 256 events for an unsubscribed stream, delivered on subscribe and filtered by `seq`.

**Next session should know:**
- Partial text is rebuilt from stored `token` events. TASK_045_EXTRA_5 (lean run events) must
  keep tokens for a live run, or persist the partial text another way, or attach loses it.
- The app builds `RunService` with `DEFAULT_APPROVAL_TIMEOUT` (60 s): a turn reopened more than
  a minute after it asked has its approval denied already, so the manual check must reopen
  within that window. Out of scope here; worth its own task.
- Manual check with the window closed to the tray was not run here.
- `pnpm format:check` still flags `apps/studio/src/renderer/ui/organisms/NavRail.tsx`, which
  this task did not touch.
