---
task: TASK_048_EXTRA_6
title: Carrying tool results, sources and reasoning into later turns
phase: 9 — Integrations
status: todo
depends_on: [TASK_048_EXTRA_5]
unblocks: [TASK_048_EXTRA_7]
estimate: 6–8 h (host, contract, desktop statistics view; one commit)
---

# TASK_048_EXTRA_6 — Carrying tool results, sources and reasoning into later turns

## Context

A later turn sees only part of what earlier turns produced:

- `model_message` / `model_history` in `crates/zvs-host/src/services/chat_prompt.rs` turn stored
  messages into model messages from their final `content` plus selections, skipping empty
  answers. Nothing else is carried.
- `PrepareNode::run` in `crates/zvs-host/src/services/chat.rs` builds the history from those
  messages only.
- Tool calls and tool results live only inside one turn, in
  `crates/zvs-host/src/services/tool_loop.rs`. When the turn ends they are persisted for display
  but never sent again.
- Retrieved store passages are injected by `window_chat_history`
  (`crates/zvs-host/src/services/chat_history.rs`) as one user message placed before the
  current message. They belong to the current turn only.
- Web sources found during a turn are not carried either.
- Reasoning is not carried.

Two visible consequences:

1. Earlier answers contain `[n]` citation markers. The next turn has the markers but not the
   sources, so the model cannot say what `[2]` was or build on it.
2. The model cannot see earlier tool results, so it repeats the same tool calls (the same file
   read, the same search) to recover what it already knew.

This task defines what is carried into later turns, how it fits the token budget, and how the
chat statistics view shows it.

## Why

A multi-turn dialog about files, searches or command output degrades into repeated tool calls
and citations that point nowhere. Carrying a compact record of earlier work makes follow-up
questions cheap and correct, and makes the citation markers in old answers meaningful.

## Depends on

- `TASK_048_EXTRA_5` — the event message kind, which the history mapping already handles and
  which must keep working with the new carried parts.

## Read first

- `/docs/PASSPORT.md` §11 (turns, the tool loop), the retrieval sections, §7 (contract rules)
- `crates/zvs-host/src/services/chat_prompt.rs` — `model_history`, `history_entries` (the
  «Статистика» view's per-message status)
- `crates/zvs-host/src/services/chat.rs` — `PrepareNode::run`, `ChatService::stats`
- `crates/zvs-host/src/services/chat_history.rs` — `window_chat_history`
- `crates/zvs-host/src/services/tool_loop.rs` — `compact_knowledge`, `shed_tool_results`, how
  tool calls are paired with results
- `crates/zvs-host/src/drivers/ai/ports.rs` — the message shape the adapters receive
- `crates/zvs-host/src/services/token_estimate.rs`
- The chat statistics view («Статистика» mode on the chat page), which is being added now

## Scope

**In scope — what is carried**
- Tool calls of earlier assistant messages: the tool name, the arguments and a compact result.
  A result is truncated to a per-call cap and marked as truncated; failed calls keep their
  error code only.
- The cited sources of earlier answers: for every `[n]` marker that appears in a carried
  answer, the source title, its location (store and fragment, or URL) and a short excerpt.
  Sources that the answer does not cite are not carried.
- Reasoning: not carried by default (see the DECISION).
- Everything carried is fenced as untrusted, the same way tool results are inside a turn.

**In scope — budget**
- Carried parts get their own share of the history budget, inside
  `contextWindow - maxOutputTokens`.
- When the history does not fit, carried parts are shed before messages, oldest first:
  first tool results are reduced to a one-line summary, then whole tool records are dropped,
  then sources. This follows `compact_knowledge` and `shed_tool_results`, reused where possible.
- The final `content` of a message is never shortened by this task; dropping whole messages
  stays the job of `window_chat_history` (and TASK_048_EXTRA_7).
- Token estimates for carried parts go through `token_estimate.rs`.

**In scope — message pairing**
- Native tool APIs (OpenAI-style `tool_calls` + `tool` messages, and the other adapters that
  speak native tools): carried calls are sent as an assistant message with tool calls followed
  by the matching tool messages, with the original call ids, and then the final assistant
  content. Every call id has exactly one result; a shed result is replaced by a stub result, not
  removed, so pairing stays valid.
- Text dialects (Qwen web, Z.ai web and other adapters without native tools): carried calls
  are rendered into the assistant message as a fixed text block, in the same dialect the
  adapter already uses for tool calls in a turn. Read `crates/zvs-qwen-web/README.md` and
  `crates/zvs-zai-web/README.md` first; no change to those crates is expected.
- A model switch between turns is handled: the pairing follows the current model's adapter,
  not the one that produced the message.

**In scope — statistics view**
- The «Статистика» mode lists, per message, what is sent and what is not. After this task it
  shows the carried tool records and sources as sent, shed parts as «Сокращено» or
  «Не передаётся», and reasoning as «Не передаётся» (unless the DECISION says otherwise).
- The host computes this; the renderer maps codes to copy and never parses strings. If the
  statistics DTO needs new fields, update the Rust type, the Zod schema and a fixture.

**In scope — tests (host)**
- `model_message` and history build: tool records and cited sources are carried; uncited
  sources are not; reasoning is not.
- Pairing: native call ids match their results; a shed result becomes a stub; text dialects
  get the text block.
- Budget: shedding order is oldest first and parts before messages; the total stays under
  `contextWindow - maxOutputTokens`.
- Event messages from TASK_048_EXTRA_5 still map to the user role.
- The statistics DTO reports carried, shortened and dropped parts.

**Out of scope** (do not do these, even if tempting)
- Summarising dropped messages (TASK_048_EXTRA_7)
- Re-running retrieval for earlier turns
- Changing how a turn's own tool loop compacts its context

## Steps

1. Read the history path and the statistics DTO; list the carried parts and their estimates.
2. Carry tool records with per-call caps; native pairing and text-dialect rendering; tests.
3. Carry cited sources; tests.
4. Shedding under the budget; tests.
5. Statistics fields, contract and fixtures; renderer copy.
6. PASSPORT §11: what a later turn sees.

## ❓ DECISION — ask the user before implementing

> **Question to ask, verbatim:**
> "What should a later turn carry from earlier turns: tool results, only short summaries of
> them, and should reasoning be carried too?"
>
> **Why it matters:** full results cost tokens on every turn; summaries save tokens but may
> force a repeat call; reasoning is large and some providers reject it in history.
> **Options:** a — compact results under a cap and cited sources, no reasoning; b — a one-line
> summary per tool call and cited sources, no reasoning; c — as (a), plus the reasoning of the
> last assistant message only.
> **If the user is unavailable:** a; note it in Handoff.

## Open questions

- Per-call cap for a carried result: a — 2 000 tokens; b — 1 000 tokens; c — a share of the
  history budget split across calls.
- How far back tool records are carried: a — every message that fits; b — the last 3 assistant
  messages only; c — only the last assistant message.
- Sources from store passages that were injected but not cited: a — never carried; b — carried
  for the previous turn only; c — carried as titles only.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check:
- Ask the model to read a file, then ask a follow-up about it: no second read.
- Ask a question answered with `[1]`, `[2]`, then ask «Что за источник [2]?»: the answer names it.
- Open «Статистика»: carried tool records and sources are listed as sent; reasoning as
  «Не передаётся».

## Definition of done

- [ ] Tool records and cited sources reach later turns under a budget
- [ ] Native pairing is valid; text dialects get the text block
- [ ] Shedding order is tested
- [ ] The statistics view reflects carried, shortened and dropped parts
- [ ] PASSPORT updated
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

*(to be filled in by the session that implements this task)*
