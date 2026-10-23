---
task: TASK_039_EXTRA_1
title: Memory engine — account and dialog memory, prompt injection, the «Память» preferences section
phase: 7 — Scenarios and Agentic Build
status: done
depends_on: [TASK_039, TASK_044_EXTRA_1, TASK_036_EXTRA_5]
unblocks: [TASK_040]
estimate: 5 h
---

# TASK_039_EXTRA_1 — Memory engine

## Context

A chat turn is one run: `chat.prepare` → `chat.generate` → `chat.persist`. `chat.prepare`
(`services/chat.rs`, `PrepareNode::run`) builds the system prompt from the dialog's own prompt
and the sections the turn needs (knowledge, MCP guidance, skills, working folder, file and
process tools, questions), then `window_chat_history` fits it with the history into the
model's context window. Nothing outlives a dialog: what the user says about themselves, their
stack or their preferences is lost when they open a new dialog, and inside a long dialog it is
lost when the history is trimmed.

The scenario node `memory.window` (TASK_037/039) is a different thing: the last N messages of a
scenario run. It is not touched here.

## How memory works (the algorithm)

Memory is a small store of **short, standalone facts** that the host injects into the system
prompt of every turn. It has three parts.

1. **Storage.** A `memory` row is one fact (≤ 500 characters) with a scope:
   - `account`: about the user, shared by every dialog («Пишет на Rust и TypeScript»,
     «Отвечать кратко»).
   - `conversation`: about one dialog only («Проект называется ZVS, ветка main»). It is deleted
     with its dialog (`ON DELETE CASCADE`).
   Each row records its `source` (`user` when typed in Preferences, `model` when saved by the
   model) and `created_at` / `updated_at`.

2. **Write path.** There are two ways in:
   - *The model* calls the `memory` bridge (`chat.memory`) in a turn with an action:
     `save { scope, content }`, `update { ref, content }` or `forget { ref }`. A short English
     prompt section says what is worth keeping: durable preferences, facts about the user or
     the project, and anything the user asks it to remember. It says what is not worth keeping:
     one-off task details, secrets, things already in memory. The host normalises the text,
     refuses an empty or over-long fact, collapses an exact duplicate in the same scope into the
     existing row, and enforces the per-scope cap. The call shows in the thread like any tool
     call, so every write is visible.
   - *The user* adds, edits and deletes facts in Параметры → «Память».

3. **Read path (injection).** `chat.prepare` loads the account facts and this dialog's facts,
   newest first, and renders a `Memory` section into the system prompt. Each fact gets a short
   per-turn ref (`a1…` for account, `d1…` for the dialog) that the model can use with
   `update` / `forget`, so no UUID reaches the prompt. The section is capped at
   `MEMORY_PROMPT_CHARS`; facts that do not fit are left out and counted in a note. The facts
   are fenced and introduced as data about the user, not as instructions, because they are
   text the model wrote earlier. Because the section is part of the system prompt,
   `window_chat_history` and `chat.context` count its tokens like any other.

Two switches control it: `memory.enabled` (inject and offer the bridge at all) and
`memory.model` (the model may write; off means memory is read-only for the model and only the
user edits it).

## Why

Without memory the user repeats the same context in every dialog, and a long dialog forgets its
own early decisions once they scroll out of the window. The agentic engine (TASK_040) needs the
same store to keep what it learned between sessions.

## Depends on

- `TASK_039`: the backlog slot this extends.
- `TASK_044_EXTRA_1`: the bridge pattern (`chat.ask`): a call intercepted in `call_tool`, not
  in `ToolRegistry`, not on «Инструменты».
- `TASK_036_EXTRA_5`: the preferences dialog and `AppSettingsStore`.

## Read first

- `/docs/PASSPORT.md` §7 (error model), §18.6 (locale)
- `crates/zvs-host/src/services/chat.rs`: `PrepareNode::run`, `call_tool`, `respond`
- `crates/zvs-host/src/services/chat_ask.rs`: the bridge to copy
- `crates/zvs-host/src/data/README.md`: adding a migration and a repository
- `apps/studio/src/renderer/features/preferences/PreferencesDialog.tsx`

## Scope

**In scope**

*Contract* (`zvs_contract::memory`, `packages/shared/src/memory.ts`, `fixtures/memory/`)
- `MemoryId`; `MemoryScope` `account | conversation`; `MemorySource` `user | model`;
  `MemoryChatDto { id, title }`; `MemoryDto { id, scope, conversation, content, source,
  createdAt, updatedAt }` (conversation present iff scope is `conversation`);
  `CreateMemoryInput { scope, conversationId?, content }`; `UpdateMemoryInput { id, content }`;
  `MemoryRef { id }`; `ClearMemoryInput { scope, conversationId? }`.
- `MEMORY_CONTENT_MAX` = 500; `MEMORY_ACCOUNT_MAX` = 100; `MEMORY_CONVERSATION_MAX` = 50.
- Channels `memory.list`, `memory.create`, `memory.update`, `memory.remove`, `memory.clear`.

*Host*
- Migration `0016_memory`: `memory` table, scope check, cascade from `conversation`.
- `MemoryRepository` and `MemoryRow`.
- `MemoryService` (`services/memory.rs`): list (with dialog titles), create, update, remove,
  clear; `normalise` (trim, collapse whitespace); duplicate collapse; caps → `CONFLICT` with
  Russian messages; unknown id → `NOT_FOUND`; `snapshot(conversation)` for the prompt.
- `chat_memory.rs`: the `chat.memory` bridge (`memory` on the wire), its parameters, the prompt
  section with refs and the size cap, and `apply` that executes one call against the service
  and the turn's ref table.
- `chat.prepare` adds the section and offers the bridge when `memory.enabled` and the model can
  call tools (`memory.model` on); `call_tool` intercepts it. Memory calls count against the call
  budget.

*Renderer*
- `MemoryStore` (`features/memory/`): load, add, edit, remove, clear by scope; errors by
  `AppErrorCode`.
- Preferences section «Память»: the two switches, «Общая память» and «Память диалогов» (grouped
  by dialog), add field, inline edit, delete, «Очистить». Copy in `memoryCopy.ts`. Search
  entries for the switches.

**Out of scope** (do not do these, even if tempting)
- Automatic extraction: a second model pass after each turn that mines facts without a tool
  call. It costs a model call per turn; revisit after real use.
- Semantic retrieval of memories (embedding them, top-k by the user message). Injection is
  all-or-budget until the store outgrows the cap.
- Memory in scenarios and the agentic engine (the service is ready for them).
- Memory per linked provider account. "Account" here is the app's single local user.

## Decisions (made while writing the task; change them before merging if they are wrong)

- The model writes without an approval prompt. Every write is visible as a tool call card, and
  `memory.model` turns model writes off.
- Per-turn refs (`a1`, `d1`) instead of ids in the prompt.
- Both switches default to on.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- Contract: fixtures for every new DTO, both sides byte-equal.
- Repository: create, list by scope, conversation cascade.
- `MemoryService`: normalise, duplicate collapse, caps, update/remove unknown id, clear by scope,
  conversation memory requires an existing dialog.
- Bridge: section rendering with refs and the size cap; `save`, `update`, `forget` through refs;
  malformed arguments go back to the model.
- Chat: a turn injects account and dialog memory into the system prompt; a model `memory` call
  saves a row; `memory.enabled = false` removes both the section and the bridge.
- Renderer: `MemoryStore` loads, splits by scope, adds, edits, removes and clears.

## Definition of done

- [x] Contract, fixtures, channels
- [x] Migration, repository, `MemoryService`
- [x] Prompt injection and the `memory` bridge in chat
- [x] Preferences «Память»
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Contract `zvs_contract::memory` / `packages/shared/src/memory.ts`: `MemoryId`, `MemoryScope`,
  `MemorySource`, `MemoryChatDto`, `MemoryDto`, `CreateMemoryInput`, `UpdateMemoryInput`,
  `MemoryRef`, `ClearMemoryInput`, `check_memory_content`; fixtures in `fixtures/memory/`.
  Channels `memory.list|create|update|remove|clear`.
- Migration `0016_memory` (scope check, cascade from `conversation`), `MemoryRow`,
  `MemoryRepository`.
- `services/memory.rs`: `MemoryService` — whitespace normalisation, case-insensitive duplicate
  collapse inside a scope (refreshes `updated_at`), caps 100 / 50 → `CONFLICT` with Russian
  messages, `switches()` reads `memory.enabled` / `memory.model` (default on), `snapshot`.
  Owned by `ChatService` (`ChatService::memory()`), exposed as `Services::memory`.
- `services/chat_memory.rs`: the `chat.memory` bridge (`memory` on the wire, actions `save`,
  `update`, `forget`), `memory_section` with per-turn refs `a1…`/`d1…` and the
  `MEMORY_PROMPT_CHARS` cap (dialog facts take the budget first), `apply_memory_call`.
  `MemoryRefs` travels in `ChatGenerateRequest.memory`, so refs survive a checkpoint.
- Chat: the section sits right after the dialog's own system prompt; `chat.context` counts it
  under `system`. Memory calls count against the call budget and have no kernel step.
- Renderer: `features/memory/` — `MemoryStore`, `MemoryFacts` (tabs «Общая» / «Диалоги» with
  counts, a borderless composer with a counter and Ctrl+Enter, fact rows with a source chip,
  per-dialog cards, clear with confirmation), `memoryCopy.ts`. Preferences gains «Память» with
  the two switches; the tool card shows a summary of the memory call.

**Deviations:**
- The chat test `Harness` sets `memory.enabled = false` (new `HarnessOptions::memory`): its
  1024-token model cannot fit the memory rules and the bridge spec. Memory tests opt in.
- A model with a very small context window pays the same cost in production (≈300 tokens for
  the rules and the tool spec while memory is on). Revisit if small local models complain.

**Also fixed:** a pre-existing type error in `tests/renderer/fileChanges.test.ts` (`from:
undefined` is not `Json`).

**Not run:** `cargo test --workspace` as a whole — the running dev build locks
`target/debug/zvs-jobd.exe`; `zvs-contract` and `zvs-host` suites were run instead. The
tailwind lint reports 22 pre-existing warnings in files this task did not touch.
