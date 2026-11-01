---
task: TASK_048_EXTRA_3
title: The /goal command — host loop, chat composer and CLI
phase: 9 — Integrations
status: done
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

- [x] An ordinary chat turn behaves exactly as before the loop extraction
- [x] A goal ends only by completion with evidence, budget, guard, stop or failure
- [x] Every round is persisted when it ends; restart never resumes silently
- [x] Approvals, questions and the execution policy behave as in a normal turn
- [x] Composer and CLI show every goal status with the right action
- [x] Channels in the contract and `CLI_CHANNELS`; PASSPORT updated
- [x] Copy Russian, in copy modules
- [ ] Gates pass; `pnpm build` left to CI if not run locally
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- DECISION 1, answered by the user: option a (40 rounds, 400 000 tokens, 30 minutes) and
  option x (messages are refused while a goal is active).
- Contract (`zvs-contract/src/goal.rs`, `packages/shared/src/chat.ts`, fixtures in
  `fixtures/chat/`):
  - `ChatCommand::Goal` (`"goal"`).
  - `ChatGoalDto`: text, status, reason, budget, usage, summary, startedAt and endedAt.
    `ConversationDto.goal` is left out when there is no goal, so the older fixtures still
    round-trip.
  - `GoalStatus`, `GoalReason`, `GoalBudgetDto` and `GoalUsageDto`.
  - `ChatGoalStepDto`, the `domain: "goal"` step.
  - `ChatGoalInput`.
  - The channels `chat.goal.continue` (returns a `RunHandleDto`) and `chat.goal.stop` (returns
    a `ConversationDto`). Both are in `CLI_CHANNELS`.
  - Migration `0030_conversation_goal` (`conversation.goal`, JSON).
- `services/tool_loop.rs`: the round loop taken out of `GenerateChatNode::respond`.
  - `ToolLoop` is parameterised by `LoopBudget` (`Calls(n)` for chat, `Open` for a goal), a
    `LoopHost` (call execution and persistence; chat implements it as `ChatLoopHost`) and a
    `RoundHook`. The hook supplies the bridge tools (`handles` / `bridge`) and decides after
    every round whether to continue, stop or nudge.
  - `ChatTurn` is the old chat behaviour. Apart from the commands-list assertions, the existing
    chat tests are unchanged and pass.
  - `request_tokens` and `round_tokens` moved here from `agentic/node.rs`.
- `services/chat_goal.rs`:
  - The `goal.complete` bridge (`complete_goal`) and the English goal note.
  - Budgets read from `chat.goal.maxRounds`, `chat.goal.maxTokens` and `chat.goal.maxMinutes`.
  - `GoalHook`. It counts rounds, tokens and minutes, trips `LoopGuard`, refuses empty evidence,
    saves the goal after every round (only while it is still `active` in the database, so a
    stop is never overwritten) and emits the step.
  - `ChatService::send` refuses a message with `Conflict` while a goal is active.
  - `continue_goal` renews the budget as the default budget plus the usage so far, and adds a
    user message «Продолжить работу над целью» marked `/goal`.
  - `stop_goal` cancels the turn.
  - `recover_goals` runs in the shell after `runs.recover`.
  - The run's `End` event settles a goal that is still active: cancelled becomes
    `stopped (user)`, anything else becomes `failed (error)`.
- Desktop:
  - `GoalStore`. It overlays the live steps on `conversation.goal`, reloads the dialog once when
    a goal it does not know starts, and handles stop and continue. It takes an injected clock.
  - `ChatGoalBar`, above the composer.
  - The composer: the «/goal» chip with the placeholder «Опишите цель…», an empty goal refused
    with a reason under the field, and a locked field with its reason during a goal.
  - The completion card: the summary, a collapsible «Подтверждение», and the card open by
    default.
  - `goalCopy.ts`.
  - Tests in `tests/renderer/chatGoal.test.ts`.
- CLI:
  - `ChatSendInput.command` is now passed through `App::send` and `pending_command`.
  - `Command::Goal` (Alt+G). The subcommands `/goal stop` and `/goal continue` are offered only
    when they apply (`Sub::offered`). A typed `/goal stop` or `/goal continue` that does not
    apply explains why.
  - The footer shows «Цель: раунд N из M · K мин» or «Цель приостановлена: …». The minutes
    keep counting between rounds by frame ticks.
  - `goal` steps update the footer and post notices on pause and stop.
  - `goal.complete` renders as a block whose evidence is collapsed to three lines.
  - The first Ctrl+C during a goal asks for confirmation and the second one stops the goal.
    Any other key in between disarms it.
- PASSPORT: §7 (dated note on the two channels), §11 (*Goal* row in the mapping table) and §14
  (*Goals* after *Chat tool set*).

**Deviated from plan:**
- LoopGuard repetition sets `stopped` with the reason `repetition`, the same as the agentic
  session (`Stopped / Loop`). Budget exhaustion sets `paused`.
- A round without tool calls does not end a goal. The loop sends a nudge, and the budget bounds
  how often that can happen.
- During an open-ended loop the oldest tool results are replaced with a short placeholder when
  the context would overflow (`shed_tool_results`). Ordinary chat turns are unaffected.
- The CLI needed an Alt+G shortcut: a test requires every palette command except `/exit` to
  have one.
- `ChatGoalStepDto` also carries `reason`, so a pause shows its reason without a reload.

**Next session should know:**
- Gates not passing, and not caused by this task (both reproduce on a clean `main`):
  - `cargo clippy` fails in `data/repositories/model.rs:149` on
    `clippy::unnecessary_option_map_or_else`, a lint new in the current toolchain. With that
    one lint allowed, clippy is clean across the workspace.
  - `services::chat::tests::tool_loop::vendor_usage_replaces_the_estimate_and_the_window_reaches_the_context_meter`
    fails with `tokens_out` 63 instead of 60.
- Not run: `pnpm build` and `pnpm test:e2e`. The bar was not checked by eye in light and dark
  themes or at narrow widths. It uses only theme tokens and wraps its progress line.
- TASK_048_EXTRA_4 reuses `ToolLoop`. It should implement `LoopHost` and `RoundHook` rather
  than add parameters to `ChatGenerateRequest`.
- A goal continuation sees earlier rounds only through the saved assistant text, not their tool
  results, because chat history replays text only.
- TASK_052 can expose the three `chat.goal.*` settings.
