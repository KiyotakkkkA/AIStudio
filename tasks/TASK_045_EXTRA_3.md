---
task: TASK_045_EXTRA_3
title: Chat turn reattach — a reopened chat shows the live turn, its approvals and its questions
phase: 8 — Documents and the agent browser
status: todo
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

- [ ] `chat.attach` with tests; contract in three places
- [ ] A reopened chat shows the live turn, approvals, folder requests and questions
- [ ] A pending approval or question can be answered after reopening
- [ ] No English user-facing string left in `ChatStore`
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
