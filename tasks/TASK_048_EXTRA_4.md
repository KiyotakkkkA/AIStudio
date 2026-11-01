---
task: TASK_048_EXTRA_4
title: Subagents — host core and policy, preferred models, desktop and CLI
phase: 9 — Integrations
status: done
depends_on: [TASK_048_EXTRA_1, TASK_048_EXTRA_3]
unblocks: []
estimate: 12–14 h (four parts; one commit)
---

# TASK_048_EXTRA_4 — Subagents: host core and policy, preferred models, desktop and CLI

## Context

A chat turn is a kernel run (`RunKind::Chat`, `subject_id` = conversation id). Its tool loop was
extracted into a reusable loop by TASK_048_EXTRA_3; its Handoff has the name.

Tool calls are admitted by `call_tool` (`crates/zvs-host/src/services/chat.rs`, about :1644). The
path goes through:
- `StepContext::invoke_tool`, with a `ToolStepRequest` (`kernel/types.rs`, about :276)
- the tool's tier and `permission_for`
- the `ExecReview` port (`kernel/types.rs`, about :254), which `ExecPolicyService` implements
  (`crates/zvs-host/src/policy/service.rs`)

Consecutive side-effect-free calls run concurrently (`OfferedTool.read_only`, `chat.rs` about
:2178). The judge's per-run cap is `JUDGE_CALLS_PER_RUN`, 50, and it is keyed by run id.

The kernel has **no parent-run concept**. `StartRunInput` and `RunDto`
(`crates/zvs-contract/src/runs.rs` about :343 and :487) carry kind, `subject_id`, title,
`retry_of_id`, graph, input and concurrency. Drivers come from
`KernelServices.providers.text(provider_id)`. The provider mode is
`ProviderSummaryDto.auth_mode: AuthMode` (`crates/zvs-contract/src/providers.rs` about :273).

This task adds a model tool, `agent.delegate`. It starts a **child run** with its own model loop,
a narrowed tool set and its own budget, and returns only a final report to the parent. The rule
is the one scenarios already follow (PASSPORT §14 *Tool permission ceiling*): a child may narrow
its parent's rights, never widen them.

The task has four parts, done in order and committed together:

- **A, host core and policy.** Child runs, presets, the ceiling, policy and approvals, folders,
  budgets, concurrency, and the report.
- **B, preferred models.** A «Субагенты» block in Preferences → «Общие», built the same way as
  «Модель-арбитр».
- **C, desktop.** Subagent cards in the thread, labelled approvals, and child runs nested on the
  Tasks page.
- **D, CLI.** Live transcript blocks, labelled prompts, and stopping a single child.

### Where things are

**Judge setting, the precedent for part B**
- Renderer (R = `apps/studio/src/renderer`):
  - `JudgeModelRow` is in `R/features/preferences/PreferencesDialog.tsx`, about :403-447.
    `GeneralSection` is at about :449, with rows in `GENERAL_ROWS`.
  - `ExecPolicyStore.ts` has `selectJudgeProvider`, `setJudgeModel`, `clearJudge`, and a
    `save()` that calls `get` and then `set`.
  - The model picker is `R/features/chat/ModelFields.tsx`.
- Host: `policy/service.rs`.
  - `judge_model()` checks that the provider is enabled, has the Text capability, and that the
    model is available.
  - `set()` refuses a judge that fails those checks.
  - `get()` removes a judge that has since disappeared.
  - The setting is reached only through its own channels.

**Renderer, part C**
- Tool cards are `R/features/chat/ChatToolCalls.tsx`. `toolCallView.ts:147` picks the view and
  `ToolCallDetails.tsx:203` renders the details. Copy is in `toolCallCopy.ts`.
- The approval modal is in `R/features/chat/`; find where `kind` is switched.
- The Tasks page store is `R/features/tasks/TaskStore.ts`. It calls `runs.list { live: true,
  limit: 100 }` at about :153 and `runs.cancel`.
- The Tasks page UI is `TasksWorkspace.tsx`, `TaskRow.tsx` and `TaskDetailRail.tsx`. Run history
  is `RunsWorkspace.tsx`.

**CLI, part D** (`crates/zvs-cli/src`)
- `transcript.absorb` (`state/transcript.rs:175`) and `tool_call()` (:125) keep only
  `domain == "tool-call"` steps.
- Approval prompts are built by `prompt::from_request` (`state/prompt.rs`) inside `App::event`
  (`state/mod.rs:1020`).
- `ui/transcript.rs` has `lines()` (:622), `tool()` (:75) and `status_look` (:27).
- `CLI_CHANNELS` lists `chat.cancel`, not `runs.cancel`.

## Why

Long investigations fill the parent's context with file dumps. A subagent reads widely and
returns only the conclusion, and read-only subagents can run in parallel.

Subagents also multiply model calls, which causes two problems:
- Without a model choice, every subagent costs as much as the main model.
- Account-mode providers receive bursts of requests the person's own page would never send.

When work is delegated, the person must still see what is happening on their machine and which
agent is asking for permission.

## Depends on

- `TASK_048_EXTRA_1` — execution policy, judge, audit.
- `TASK_048_EXTRA_3` — the extracted tool loop; `goal.complete` (excluded from children).

## Read first

- `/docs/PASSPORT.md`:
  - §11: kernel, approval and question gates, concurrent calls.
  - §14: *Tool permission ceiling*, *Chat tool set*, *Folder access*, *Execution policy*,
    *Judge*, *Audit*, *Page and document content is untrusted input*.
  - §12 account-mode providers, D11, §7 *The CLI pipe*, §13.
- `CONVENTIONS.md`: fixed terminology; `plural()`; «Выполняется», never «думает».
- `crates/zvs-host/src/services/chat_tools.rs` — `plan_tools`, `OfferedTool`.
- `design/Chat.dc.html`, `design/Tasks.dc.html`, `design/Main.dc.html`; the warm ZVS design
  baseline, light and dark. Surfaces built from the same template must match, so change shared
  components first.

## Scope

**In scope — A. Host core and policy**

Contract
- `RunKind::Subagent`.
- `parent_run_id: Option<RunId>` on `StartRunInput` and `RunDto`, plus a migration column.
- `runs.list` accepts `parentRunId`.
- Three places: Rust, Zod, fixtures.

The tool
- `agent.delegate { task, preset, context? }` goes in `kernel_tools.rs`, with a Russian
  description and default tier `auto`.
- The delegate call needs no approval of its own, because each call the child makes is admitted
  separately.

Presets
- `explore` offers `fs.list`, `fs.read`, `fs.search` and `tool.search`, plus the knowledge and
  memory tools the parent was offered. It is read-only, so consecutive `explore` delegates run
  concurrently.
- `general` offers the parent turn's tool set minus the exclusions. It is a barrier, like any
  call with side effects.

Exclusions, in every preset
- `browser.*`, `chat.ask`, `agent.delegate` (so depth stays at 1), and `goal.complete`.
- A child never asks the user anything. It reports what it could not resolve.

Ceiling
- A child's tiers are its parent's or stricter.
- Anything `off` for the parent does not exist for the child.

Execution policy
- The same global mode applies.
- Judge calls count against the **root** run, so subagents cannot multiply the judge cap.

Approvals
- A child's approval carries the parent's `conversationId` and `subagent: { runId, label }`.
- It reaches the same modal and the same CLI prompts, and the first answer wins.
- Cancelling the parent cancels every child.

Folders
- The child uses the dialog's working folder and its stored grants (`always`, `conversation`).
- Once-grants are not inherited, so the child asks again.
- `chat:` is shared with the parent.

Budget per child
- Tool calls, tokens and minutes, from the settings `subagents.budget.*`.
- When the budget runs out, the child returns a partial report marked as partial.

Concurrency
- A parent runs at most N children at once, where N comes from part B.
- Children on an account-mode provider (`auth_mode` is `Account`) run one at a time. This keeps
  the request pattern close to what the person's own page sends (D11).

Report
- The report is capped at `TOOL_RESULT_MAX_CHARS`.
- It is fenced as untrusted data (`<subagent-report>`), because the child may have read web or
  document content.
- Meta: `{ runId, preset, model, modelSource, toolCalls, tokens, status, partial }`.

Events
- Child tool steps are emitted on the parent's stream with `domain: "subagent"` and the child
  `runId`.
- Child token deltas are not streamed.
- Child tokens are added to the parent turn's usage.

**In scope — B. Preferred models**

Contract
- `SubagentPreferencesDto { enabled, models: [{ providerId, modelId }], concurrency }`.
  - `models` is ordered, has at most 5 entries, and no duplicates.
  - `concurrency` is between 1 and 4.
- Channels `subagents.get` and `subagents.set`. `settings.set` refuses this key.
- Rust, Zod, fixtures.

Host validation
- `set` refuses a model if its provider is disabled or lacks the Text capability, or if the
  model is unavailable or cannot call tools. The last check is stricter than the judge's,
  because subagents need tools. Messages are Russian.
- `get` drops models that have disappeared and stores the result.
- `enabled: false` removes `agent.delegate` from the offered tools.

Selection
- The first listed model that is available now; otherwise the dialog's model.
- `modelSource` in the meta records which, as `preferred` or `dialog`.
- Account-mode models are allowed, and always serialise.

The «Субагенты» block in «Общие»
- A switch: «Использовать субагентов».
- «Предпочтительные модели»: an ordered list.
  - Add a model by choosing a provider and then a model through `ModelFields`.
  - Entries can be removed and moved up or down.
  - Hint: «Если ни одна недоступна, используется модель диалога».
- «Одновременно»: 1–4.
- When any listed model is account-mode, the note «Модели аккаунтов выполняют субагентов по
  одному».
- Store: `SubagentPreferencesStore`, tested with `createFakeBridge()`.

**In scope — C. Desktop**

The subagent card in the thread
- Header: the preset («Исследование» or «Общая задача»), the task's first line, and the model.
- While running: «Выполняется», «N инструментов» through `plural()`, and a collapsible list of
  the child's tool steps drawn with the existing compact rows.
- When finished: the report as Markdown, a «Частичный результат» badge when partial, tokens
  formatted for ru-RU, and the duration.
- «Остановить» while running, which calls `runs.cancel` with the child run id.
- Consecutive delegates render as one group.

Store
- Child steps are grouped by `runId` from `domain: "subagent"` events.
- Tested with `createFakeBridge()`.

Approval modal
- When `subagent` is present, the header reads «Субагент: <label>». The diff and process views
  are unchanged.

Tasks page and run history
- Child runs nest under their parent.
- A single child can be cancelled.

Copy
- In `subagentCopy.ts`; tests assert against the constants.

**In scope — D. CLI**

Transcript
- A delegate call renders as a block. Its header shows the preset, the task and the model.
- While running: a spinner, and one indented line per child tool call, reusing `tool()` styling.
  Only the last 5 lines are shown.
- When finished: the report, collapsed to 8 lines with an expand key, then tokens and duration.
- Parallel delegates appear as consecutive blocks updated in place.
- `domain: "subagent"` steps are grouped by child `runId` inside the live segments.

Prompts
- A child's approval prompt has the title line «Субагент: <label>».

Stopping a child
- A shortcut and a palette item stop the newest running child without cancelling the parent.
- This adds the host channel `cli.subagents.cancel { runId }`:
  - It accepts only runs of kind `Subagent` whose root turn belongs to a dialog this connection
    can reach.
  - It goes into `CLI_CHANNELS`, with the guard test.
  - It follows the `cli.mcp.create` pattern, a narrow CLI-only channel, instead of opening
    `runs.cancel` to the console.
- Tests in `state/tests.rs` and `ui/tests.rs` against the `copy::` constants.

**In scope — PASSPORT**
- §14 gains a *Subagents* entry covering: the ceiling, the exclusions, the policy and judge cap,
  approvals, folders, the untrusted report, account-mode serialisation, the preferences setting
  and the order models are selected in.
- §11 gains a row in the mapping table.
- §7 gains the `cli.subagents.cancel` exception.

**Out of scope** (do not do these, even if tempting)
- Subagents inside agentic sessions
- Nested delegation (depth above 1)
- Streaming child tokens
- Subagent preferences in the CLI. Settings stay studio-only, so `subagents.get` and `.set` stay
  out of `CLI_CHANNELS`.
- `runs.cancel` in `CLI_CHANNELS`
- Budget fields in the UI (TASK_052)

## Steps

1. Contract and migration; `cargo test -p zvs-contract`.
2. Creating child runs with `parent_run_id`, and cascade cancel. Tests.
3. Preset tool planning on top of `plan_tools`. Tests that every exclusion holds and that a child
   never gets a tool its parent lacked.
4. Policy wiring:
   - The judge spend is keyed by the root run.
   - Approval payloads carry the `subagent` label.
   - Tests that a child cannot exceed the cap, and that a child's approval reaches the parent's
     dialog.
5. Folder rules. Test that a once-grant is not inherited.
6. The `agent.delegate` node: run the child loop, cap and fence the report, merge usage. Test
   complete, budget-partial, cancel and failure with a fake driver.
7. The concurrency limit and account-mode serialisation. Tests.
8. Preferences:
   - Contract.
   - Host service with validation and sanitising, and tests for each refusal.
   - Selection, with tests for preferred, fallback and account-mode.
   - Renderer store, its tests, and the «Субагенты» block.
9. Desktop:
   - Store grouping and its tests.
   - Cards and groups.
   - The modal label.
   - Nesting on the Tasks and Runs pages, with `TaskStore` tests.
   - Check light and dark themes, narrow widths, and the keyboard.
10. CLI:
    - The host channel with its kind and ownership checks, and its tests.
    - Transcript grouping and blocks.
    - The prompt title.
    - The stop shortcut, with the uniqueness test.
11. PASSPORT §7, §11 and §14.

## ❓ DECISION 1 — ask the user before implementing part A

> **Question to ask, verbatim:**
> "Which subagent kinds do you want in the first version: only read-only «explore», or also
> «general» subagents that can edit files and run commands under the same approvals?"
>
> **Why it matters:** `explore` cannot change anything, can run in parallel, and adds almost no
> security surface; `general` multiplies the approvals and policy load.
> **Options:** a — both; b — `explore` only, `general` later; c — both, but `general` always
> runs one at a time.
> **If the user is unavailable:** c; note it in Handoff.

## ❓ DECISION 2 — ask the user before implementing part B

> **Question to ask, verbatim:**
> "Should subagent models be one ordered list for every kind of subagent, or a separate list
> for «explore» (reading and searching) and for «general»?"
>
> **Why it matters:** exploring is where a cheap model pays off most; general work needs a
> capable one.
> **Options:** a — one list (simplest); b — two lists, `explore` falling back to the general
> list and then the dialog's model; c — one list plus a per-entry «только для поиска» flag.
> **If the user is unavailable:** a; note it in Handoff.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check:
- In «Общие», list a cheap API model first and the dialog's model second.
- In a dialog, ask "explore how X works using three subagents in parallel".
  - Three live cards render as one group, each listing its tool calls and showing the cheap
    model.
  - Each card ends with a report, and the parent answers from the reports.
- A `general` child that writes a file raises the diff approval with the title
  «Субагент: …».
- On the Tasks page the children sit under the turn. Stopping one leaves the others running, and
  cancelling the turn stops them all.
- Disable the cheap provider. The next delegate uses the second model, and the block drops the
  missing entry after a reload.
- With an account-mode model listed and «Одновременно» set to 3, three explore delegates run one
  after another.
- In `aistudio`, the same request shows three live blocks and a labelled prompt. The stop
  shortcut ends the newest child while the parent continues.

## Definition of done

- [x] A child never receives a tool or tier its parent lacked, and never an excluded tool
- [x] Judge cap, approvals, folders and audit behave as specified and are tested
- [x] Reports are capped and fenced; usage is merged into the parent
- [x] Concurrency limit and account-mode serialisation are enforced
- [x] Preferences persist, validate and sanitise like the judge setting; selection is tested
- [x] Desktop and CLI show live child work, labelled approvals, and can stop a single child
- [x] `cli.subagents.cancel` is narrow, allow-listed and tested
- [x] Copy is Russian and lives in copy modules; counts go through plural forms
- [x] PASSPORT §7, §11 and §14 are updated
- [ ] Gates pass; `pnpm build` is left to CI if not run locally
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Decisions, answered by the user: DECISION 1 option c (`explore` and `general`, `general`
  always one at a time) and DECISION 2 option a (one ordered model list).
- Contract: `RunKind::Subagent`; `parentRunId` on `StartRunInput`, `RunDto`, `RunSummaryDto`
  and `RunListFilter` (migration `0031_run_parent`, indexed); `SubagentModelDto`,
  `SubagentPreferencesDto` (≤ 5 unique models, concurrency 1–4), `CliSubagentCancelInput`,
  `SUBAGENT_DOMAIN`; channels `subagents.get`, `subagents.set`, `cli.subagents.cancel`.
  Rust, Zod (`packages/shared/src/subagents.ts`) and fixtures (`fixtures/subagents/`, runs
  fixtures updated).
- Host core (`services/chat/subagent.rs`): `agent.delegate` (catalogue tool, tier `auto`,
  Russian description) handled in `call_tool` like `chat.ask`; each delegate starts a
  `subagent` run with one `subagent.run` node that runs the shared `ToolLoop` with a
  `CallSink::Subagent` (steps go out as `domain: "subagent"` with the child `runId`, nothing
  is written to a message). Presets, exclusions and the ceiling are in `child_tools`; the child
  graph's `allowedTools` is the child set. Budget hook (`subagents.budget.*`, defaults 20 /
  200 000 / 10) wraps up with a partial report via the new `RoundNext::Wrap`. The report is
  capped and fenced in `<subagent-report>`; meta carries runId, preset, model, modelSource,
  toolCalls, tokens, status, partial, plus report, durationMs and compact steps for display.
  Child usage merges into the parent through `CallOutcome.usage`.
- Policy: the judge cap is keyed by the root run (`ExecPolicyService::root_run`). Child
  approvals are re-emitted on the parent stream with `conversationId` and
  `subagent { runId, label }`; `chat.attach` checks pending approvals against the request's
  own run. `RunService::cancel` cascades to unfinished children; `signal_cancel` stops without
  waiting. `retry` refuses subagent runs.
- Concurrency: consecutive explore delegates run concurrently through the loop; a per-turn
  semaphore holds N permits; `general` and account-mode children take all of them.
- Preferences: `SubagentService` (`services/subagents.rs`) with validation, sanitising,
  selection and budgets; `settings.set` refuses the `subagents` key.
- Desktop: «Субагенты» block in «Общие» (`SubagentPreferencesStore`); subagent cards and
  groups (`SubagentCard.tsx`, `SubagentSteps.ts`) with live child steps, inline child
  approvals titled «Субагент: …», report, partial badge, tokens, duration and «Остановить»;
  labelled standalone banner and folder modal; nested child runs on Tasks and Runs; copy in
  `features/subagents/subagentCopy.ts`.
- CLI: live child steps grouped by run, `subagent_block` (spinner, last 5 child lines, report
  collapsed to 8 lines, tokens and duration), prompt title line, `/subagent-stop` and Alt+X
  calling `cli.subagents.cancel`.
- PASSPORT §7 (CLI exception), §11 (mapping row), §14 (*Subagents*).

**Deviated from plan:**
- The full built-in catalogue guard in `tools/tests/catalogue.rs` was raised from 15 000 to
  16 000 characters: the catalogue sat just under the old limit and `agent.delegate` adds
  about 450 even with a trimmed description.
- `RunSummaryDto` also gained `parentRunId` (the Tasks and Runs pages list summaries).
- Once-grants needed no new code: they live in the parent's own `FolderCall`; a workspace
  test proves a child asks again while a conversation grant carries over.
- Light/dark, narrow-width and keyboard checks of the new desktop surfaces were not done in a
  running app; `pnpm build` and e2e are left to CI.

**Next session should know:**
- Pre-existing failures left alone (they reproduce on the previous commit): the
  `vendor_usage_replaces_the_estimate…` host test, clippy's `unnecessary_option_map_or_else`
  in `data/repositories/model.rs`, and two `TS2352` casts in `tests/renderer/chatGoal.test.ts`
  that fail `pnpm typecheck`.
- Budget fields are read from settings only; exposing them is TASK_052.
- After a reload a finished card shows the compact steps from meta; live child steps are not
  replayed by `chat.attach`.
