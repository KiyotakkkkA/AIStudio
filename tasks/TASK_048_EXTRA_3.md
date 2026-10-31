---
task: TASK_048_EXTRA_3
title: The /goal command — host loop, chat composer and CLI
phase: 9 — Integrations
status: todo
depends_on: [TASK_048_EXTRA_1, TASK_048_EXTRA_2]
unblocks: [TASK_048_EXTRA_4]
estimate: 8–10 h (three parts; one commit)
---

# TASK_048_EXTRA_3 — The /goal command: host loop, chat composer and CLI

## Context

A chat turn is one kernel run (`RunKind::Chat`, `subject_id` = conversation id). It has three
nodes, all in `crates/zvs-host/src/services/chat.rs`:

- `chat.prepare` (`PrepareNode`, about :1148)
- `chat.generate` (`GenerateChatNode`, about :2317)
- `chat.persist` (`PersistNode`, about :2343)

The tool loop is `GenerateChatNode::respond` (about :1974). Its only limit is a call budget,
`tool_calls`, which falls back to `CHAT_TOOL_CALLS = 15` (`services/chat_tools.rs:29`). When the
budget runs out, `TOOL_BUDGET_NOTE` makes the model answer. `CHAT_TOOL_ROUNDS = 4` belongs to the
skill-run loop in `skills/nodes.rs`, not to chat. A turn is meant to be short.

Slash commands already exist, through `/browser-use`:

- The command is `ChatCommand { BrowserUse => "browser-use" }`
  (`crates/zvs-contract/src/chat.rs:61`).
- `ChatSendInput.command` is stored on the message row (migration `0026_chat_commands`).
- `active_commands` (`chat_tools.rs:262`) collects commands from history, and `plan_tools`
  reacts to them.
- `Inner::command_note` (`chat.rs:659`) adds the bundled skill text.
- `chat.commands` lists the commands as `ChatCommandDto { command, description }`.

The agentic session (`crates/zvs-host/src/agentic/`) has a long loop with step, token and minute
budgets (`AgenticBudgetDto`, `agentic.budget.*` settings):

- It pauses when a budget runs out, and `agentic.extend` continues it.
- `finish` is its bridge tool.
- `LoopGuard` (`agentic/plan.rs`) stops repeated calls.
- The transcript is checkpointed after each step.

It lives on its own page with its own workspace, so it does not fit inside a dialog. Its parts
are the reference.

This task adds `/goal <text>`. The dialog keeps working on one objective across many rounds,
with its own tools, folder and grants. It stops when the model declares the goal done with
evidence, a budget runs out, or the person stops it. There are three parts, done in order and
committed together:

- **A, host.** The command, the goal state, the reusable tool loop, budgets, completion and
  channels.
- **B, chat composer.** The command chip, a goal bar, controls and a completion card.
- **C, CLI.** `/goal`, `/goal stop`, `/goal continue`, footer progress, and Ctrl+C.

### Where things are

**Host**
- `call_tool` is at `chat.rs` about :1644.
- `chat.ask` is handled at about :1748.
- `command_note` is at :659 and `resolve_model` at :423.
- The agentic `budget_exceeded` is at `agentic/node.rs` about :152, `bridge` at about :480 and
  `save` at about :331.

**Renderer** (R = `apps/studio/src/renderer`)
- The composer is `R/features/chat/ChatComposer.tsx`, and its store is `ChatStore.ts`, which
  loads `chat.commands` at about :746.
- The parsing helpers are in `chatCommands.ts`: `commandText`, `matchCommands`, `parseCommand`,
  `COMMANDS_LABEL` and `REMOVE_COMMAND`.
- The command menu is `ChatCommandMenu.tsx`. The active-command chip is at about
  `ChatComposer.tsx:104-112`.
- Tool cards are chosen by `toolCallView.ts:147`, with details in `ToolCallDetails.tsx`.

**CLI** (`crates/zvs-cli/src`)
- The CLI has never sent a `ChatCommand`. `App::send` (`state/mod.rs:1062-1089`) builds
  `ChatSendInput::new(id, text)` with effort, provider and model, and `runtime.rs:247` calls
  `chat.send`.
- `palette.rs` has `arguments(text)` (:102) and `Item::Sub`. A command with an argument goes to
  `typed_argument` (`state/mod.rs:453`).
- `transcript.absorb` keeps only `domain == "tool-call"` steps.
- `footer()` is at `ui/mod.rs:391`, and copy is in `copy.rs`.

## Why

Multi-step work in a dialog stops every 15 calls and needs «продолжай». A goal keeps the person
out of the loop except for approvals and questions, which the execution policy already governs.
The console is where long unattended work is most natural. A goal that runs for minutes needs
visible progress and a stop on both surfaces.

## Depends on

- `TASK_048_EXTRA_1` — the execution policy that answers approvals during a long run.
- `TASK_048_EXTRA_2` — background processes; a goal will commonly start a server or watcher.

## Read first

- `/docs/PASSPORT.md` §11 (kernel, question gate, concurrent calls), §14 *Chat tool set*,
  *Execution policy*; §7 *The CLI pipe*; §13
- `CONVENTIONS.md`:
  - The copy is formal and neutral. The model is not anthropomorphised: write «Выполняется»,
    never «думает».
  - Counts go through `plural()`.
  - Numbers are formatted with `toLocaleString("ru-RU")`.
- `design/Chat.dc.html`; warm ZVS design baseline, light and dark

## Scope

**In scope — A. Host**

The command
- Add `ChatCommand::Goal` (`"goal"`) in Rust, Zod and the fixtures: `ChatCommand`,
  `ChatCommandDto`, `ChatSendInput` and `MessageDto`.
- The message text is the goal.
- `chat.commands` lists it with a Russian description.

Refactor
- Extract the round loop from `GenerateChatNode::respond` into a reusable tool loop, for example
  `services/tool_loop.rs`.
- Parameterise it by the budget, the extra bridge tools, and a per-round hook.
- An ordinary chat turn must behave exactly as before; the existing chat tests are the guard.
- TASK_048_EXTRA_4 reuses this loop for subagents.

Goal state
- `ConversationDto.goal: { text, status, budget, usage, summary, startedAt, endedAt } | null`.
- `status` is one of `active | paused | done | stopped | failed`.
- Stored through a migration; the DTO change goes in Rust, Zod and the fixture.

The goal loop
- It offers the same tools as the dialog, plus a bridge tool `goal.complete { summary, evidence }`.
  Like `chat.ask`, it is not a registry tool.
- `evidence` is required and must not be empty.
- An English system note states the goal and the rules: keep working, verify before completing,
  and ask the user only when blocked.

Budgets
- Rounds, tokens and minutes, read from `chat.goal.maxRounds`, `chat.goal.maxTokens` and
  `chat.goal.maxMinutes`.
- When one is exhausted the goal becomes `paused`, with the reason.
- `LoopGuard` repetition stops the goal too.

Durability
- Each round is persisted as it ends: assistant text and tool calls.
- After a restart, an `active` goal reads as `paused`. It is never resumed automatically.

Channels
- `chat.goal.continue { conversationId }` moves a goal from paused to active with a renewed
  budget.
- `chat.goal.stop { conversationId }` cancels the turn and sets `stopped`.
- Both go into `CLI_CHANNELS`, with the guard test and the dated §7 note.

Events and sending
- After every round, send a step with `domain: "goal"` carrying `{ conversationId, status,
  round, usage, budget }`.
- While a goal is active, `chat.send` to that dialog is refused with `Conflict`, or queued,
  as DECISION 1 decides.

**In scope — B. Chat composer**

The chip
- With `/goal` chosen, the chip reads «/goal» and the placeholder asks for the goal.
- Sending empty text is blocked, with the reason shown under the field.

The goal bar
- Shown above the composer while the goal is not `done`.
- It holds the goal text, truncated, with the full text on hover.
- Status: «Выполняется», «Приостановлено: <reason>», «Остановлено» or «Ошибка».
- Progress: «Раунд 12 из 40», tokens against the budget formatted for ru-RU, and the minutes
  elapsed.
- Actions: «Остановить» while active, «Продолжить» while paused.

The composer during a goal
- Follows DECISION 1. If messages are refused, the field is disabled with a short reason, and
  «Остановить» stays reachable.

Completion
- `goal.complete` renders the summary and a collapsible «Подтверждение» with the evidence.
- The bar then closes to «Цель достигнута», which can be dismissed.

Store and copy
- Goal state lives in `ChatStore` or a small `GoalStore`. It applies `domain: "goal"` events and
  conversation reloads, and is tested with `createFakeBridge()` and `createFakeClock()`.
- Copy goes in `goalCopy.ts`, and tests assert against the constants.

**In scope — C. CLI**

Sending commands
- `App::send` passes `ChatSendInput.command` in general. `/browser-use` itself stays out of
  scope, but the plumbing must allow it later.

`Command::Goal` (`/goal`, `copy::CMD_GOAL`)
- `/goal <text>` sends with `command: Some(ChatCommand::Goal)`.
- A bare `/goal` shows a hint and sends nothing.
- The subcommands `/goal stop` and `/goal continue` (`Item::Sub`) call `chat.goal.stop` and
  `chat.goal.continue`. Each is offered only in the state where it applies.

Footer
- «Цель: раунд 12 из 40 · 3 мин», or «Цель приостановлена: <reason>».
- It is restored on `/resume` of a dialog with a paused goal.

Transcript
- `goal` steps update the footer and are not shown as tool rows.
- `goal.complete` renders as a block with the summary and the evidence, collapsed to three lines.
- Stopping or pausing shows a notice.

Ctrl+C during a goal
- Asks once for confirmation, then calls `chat.goal.stop`. It does not leave the goal paused.

Tests
- In `state/tests.rs` and `ui/tests.rs`, against the `copy::` constants.

**In scope — PASSPORT**
- §11: a *Goal* row in the front-end mapping table.
- §14 *Chat tool set*: a note on `/goal`.
- §7: the new channels.

**Out of scope** (do not do these, even if tempting)
- Subagents (TASK_048_EXTRA_4)
- Running a goal inside an agentic session
- A second model checking completion
- Editing budgets in the UI (TASK_052)
- `/browser-use` in the CLI

## Steps

1. Contract and migration; run `cargo test -p zvs-contract`.
2. Extract the tool loop. All existing chat tests must pass unchanged.
3. The goal state service: start, round accounting, pause, continue, stop, and sanitising after
   a restart. Test every budget.
4. The goal turn, tested with a fake driver. Cover a goal that completes, one that pauses on each
   budget, one that stops on repetition, and refusal of empty `evidence`.
5. Channels, `CLI_CHANNELS` and the progress events.
6. Desktop:
   - Store transitions and their tests.
   - The chip and its validation.
   - The goal bar and its actions.
   - The composer rule from DECISION 1.
   - The completion card.
   - Check light and dark themes and narrow widths.
7. CLI:
   - The command plumbing and `Command::Goal` with its subcommands.
   - Goal state on open and resume.
   - `goal` step handling, the footer and the transcript block.
   - Ctrl+C behaviour.
   - Tests for all of the above.
8. PASSPORT.

## ❓ DECISION 1 — ask the user before implementing part A

> **Question to ask, verbatim:**
> "What are the default goal budgets, and may the person write to the dialog while a goal is
> running?"
>
> **Why it matters:** budgets bound cost on API providers; messages mid-goal need either
> queueing at round boundaries (like `agentic.instruct`) or a locked composer.
> **Options:** budgets — a: 40 rounds / 400 000 tokens / 30 minutes; b: 100 / 1 000 000 / 120.
> Messages — x: refused while active, «Остановить» first (smallest); y: queued and delivered at
> the next round boundary as an instruction (adds `chat.goal.instruct`).
> **If the user is unavailable:** a + x; note it in Handoff.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check:
- In the desktop app, choose `/goal` with a task that needs more than 15 tool calls, for example
  "add tests for every module in this folder until `cargo test` passes". The bar counts rounds
  past 15.
- The goal ends on `goal.complete` with the summary card. A budget pause shows its reason, and
  «Продолжить» resumes.
- «Остановить» ends the goal within one round. After quitting mid-goal, the next start shows the
  goal paused.
- In `aistudio`, the same goal shows round progress in the footer. `/goal stop` and
  `/goal continue` work, Ctrl+C stops after one confirmation, and the desktop shows the same
  state.

## Definition of done

- [ ] An ordinary chat turn behaves exactly as before the loop extraction
- [ ] A goal ends only by completion with evidence, budget, guard, stop or failure
- [ ] Every round is persisted when it ends; restart never resumes silently
- [ ] Approvals, questions and the execution policy behave as in a normal turn
- [ ] Composer and CLI show every goal status with the right action
- [ ] Channels in the contract and `CLI_CHANNELS`; PASSPORT updated
- [ ] Copy Russian, in copy modules
- [ ] Gates pass; `pnpm build` left to CI if not run locally
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
