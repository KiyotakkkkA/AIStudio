---
task: TASK_048_EXTRA_7
title: Compacting history that no longer fits the context window
phase: 9 — Integrations
status: todo
depends_on: [TASK_048_EXTRA_6]
unblocks: []
estimate: 6–8 h (host, contract, desktop and CLI; one commit)
---

# TASK_048_EXTRA_7 — Compacting history that no longer fits the context window

## Context

`window_chat_history` (`crates/zvs-host/src/services/chat_history.rs`) keeps the newest
messages that fit `contextWindow - maxOutputTokens` and drops the older ones. Nothing replaces
them:

- The model gets no summary of the dropped prefix, so in a long dialog it forgets the task,
  the decisions and the names agreed at the start.
- The only trace is a count in the context meter and a `HISTORY_TRIMMED` log line.

This task adds a summary of the dropped prefix: produced by a model, stored per conversation,
sent in place of the dropped messages, and shown in the thread.

## Why

Long dialogs are where the app is most useful and where silent truncation hurts most. A
compact summary keeps the thread coherent at a small, fixed cost, and the person can see where
the model's verbatim memory ends.

## Depends on

- `TASK_048_EXTRA_6` — the budget split between messages and carried parts. The summary takes
  its share from the same budget, and the summary prompt reads the carried parts of the
  messages it replaces.

## Read first

- `/docs/PASSPORT.md` §11 (turns), §7 (contract rules)
- `crates/zvs-host/src/services/chat_history.rs` — `window_chat_history`
- `crates/zvs-host/src/services/chat_prompt.rs` — `history_entries` (statuses shown in
  «Статистика»)
- `crates/zvs-host/src/services/chat.rs` — `PrepareNode::run`, edit, branch and truncate paths
- `crates/zvs-host/src/services/token_estimate.rs`
- The context meter in the chat composer and the «Статистика» mode

## Scope

**In scope — the summary**
- A per-conversation record: the summary text, the id of the last message it covers, the model
  that produced it, its token estimate and when it was made. A migration adds it.
- Sent as the first history message, fenced and labelled as a summary of earlier messages,
  followed by the newest messages that fit.
- Invalidated when any covered message is edited, deleted, or cut by truncate, and when a
  branch starts at or before the covered range. A branch after the range inherits the summary.
- A stale or missing summary never blocks a turn: the turn falls back to today's trimming and
  the summary is rebuilt.

**In scope — when it is produced**
- Threshold: when the history before a turn uses more than a share of the budget (see Open
  questions), the prefix that would be dropped is summarised.
- An existing summary is extended, not rebuilt: the new prompt takes the old summary plus the
  newly dropped messages.
- Where it runs follows the DECISION (in the turn before the answer, or in the background after
  the turn).
- The summary call uses the dialog's model unless the DECISION picks another; its usage is
  counted in the conversation's token statistics.

**In scope — budget**
- The summary has a hard cap (see Open questions). The summary prompt asks for that size and
  the result is truncated if it is longer.
- The cap comes out of `contextWindow - maxOutputTokens` before messages are windowed.

**In scope — pinned messages**
- A pinned message is never folded into the summary. It is sent verbatim in its original
  position relative to the summary; if pinned messages alone exceed the budget, the oldest pin
  is dropped and logged, as today.

**In scope — desktop and CLI**
- Desktop: a divider in the thread at the summary boundary, «Ранние сообщения сокращены до
  сводки», with «Показать сводку», which opens the summary text read-only. The context meter
  shows «Сводка: N токенов» next to the trimmed count.
- «Статистика»: covered messages show «В сводке» instead of «Не передаётся».
- CLI: a notice line «Ранние сообщения сокращены до сводки» when a summary is created or
  extended.
- Copy follows `CONVENTIONS.md`: counts through `plural()`, numbers through
  `toLocaleString("ru-RU")`.

**In scope — contract**
- The summary DTO and the statistics fields: Rust type, Zod schema, golden fixtures.

**In scope — tests (host)**
- Threshold triggers a summary; below it nothing happens.
- Extension uses the previous summary and only the new messages.
- Invalidation on edit, delete, truncate and an early branch; inheritance on a late branch.
- Pinned messages stay verbatim and outside the summary.
- The cap holds; a failed summary call falls back to trimming without failing the turn.
- Fake AI driver and `createFakeClock()`-style host fakes, no real model.

**Out of scope** (do not do these, even if tempting)
- Letting the person edit the summary
- Summarising tool records inside a single running turn
- Cross-conversation memory

## Steps

1. Migration and repository for the summary record; contract and fixtures.
2. Windowing with a summary and pins; tests.
3. Producing and extending the summary at the threshold; tests with a fake driver.
4. Invalidation on edit, delete, truncate and branch; tests.
5. Desktop divider, meter and statistics; CLI notice.
6. PASSPORT §11: history windowing and the summary.

## ❓ DECISION — ask the user before implementing

> **Question to ask, verbatim:**
> "When the history no longer fits, should the summary be made during the turn before the
> answer, or in the background after the turn, and with which model?"
>
> **Why it matters:** in-turn adds latency to that answer but is never stale; background is
> free for the person but the first over-budget turn still loses messages.
> **Options:** a — in the turn, with the dialog's model; b — in the background after the turn,
> with the dialog's model; c — in the background, with a separate model chosen in settings
> (falling back to the dialog's model).
> **If the user is unavailable:** b; note it in Handoff.

## Open questions

- Threshold: a — 80 % of the history budget; b — only when messages would actually be
  dropped; c — a setting, default 80 %.
- Summary cap: a — 1 500 tokens; b — 10 % of the context window, at most 4 000; c — a setting.
- Whether the person can turn compaction off per conversation: a — no; b — yes, a toggle in
  the composer menu; c — a global setting only.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check:
- With a small context window, write until messages are trimmed: the divider appears and the
  model still knows what was agreed at the start.
- Edit an early message: the summary is rebuilt.
- Pin an early message: it is sent verbatim after the summary.

## Definition of done

- [ ] Dropped history is replaced by a stored, extended summary under a cap
- [ ] Invalidation and pins are tested
- [ ] Desktop divider, meter, statistics and CLI notice
- [ ] PASSPORT updated
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

*(to be filled in by the session that implements this task)*
