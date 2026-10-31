---
task: TASK_048_EXTRA_2
title: Background shell processes — host, desktop and CLI
phase: 9 — Integrations
status: todo
depends_on: [TASK_048_EXTRA_1]
unblocks: [TASK_048_EXTRA_3]
estimate: 8–10 h (three parts; one commit)
---

# TASK_048_EXTRA_2 — Background shell processes: host, desktop and CLI

## Context

`shell.run` and `code.run` (`crates/zvs-host/src/commands/nodes.rs`) run a process to
completion inside the tool call. Both share the runner in
`crates/zvs-host/src/toolchain/process.rs`: `run_process` (about line 220) with
`PROCESS_TIMEOUT` 120 s, `PROCESS_OUTPUT_BYTES` 64 KiB, and `kill_tree` on timeout or cancel. A
model cannot start a dev server, a watcher or a long build, keep working, and read the output
later.

The user's own shell is a different thing: the «Терминал» dock. It lives in
`crates/zvs-host/src/terminal/` (`TerminalService`, at most 8 per dialog, 256 KiB scrollback,
`terminal.attach`) and sends status events as `TerminalStatusDto { domain: "terminal", id,
status, exitCode }` (`crates/zvs-contract/src/terminal.rs`). It is a PTY the person types into
and it raises no approvals. This task does **not** reuse it for agent processes. It copies its
lifecycle ideas: the per-dialog cap, the scrollback, status events, and kill on dialog removal.

The task has three parts, done in order and committed together:

- **A, host.** `shell.run` gains `background: true`, which starts the process, returns a handle
  at once and leaves the process to a supervisor owned by the dialog. New model tools read,
  list and stop it. New channels let the desktop and the CLI do the same.
- **B, desktop.** Thread cards with live status, a composer indicator, and an output view.
- **C, CLI.** A `/processes` panel, an output overlay, a footer count, and transcript rows.

Approval does not change in kind. Starting a background process is a `shell.run` call. It goes
through `permission_for` (minimum tier `ask`), `ExecReview` (`crates/zvs-host/src/policy/`) and
the `kind: "process"` approval, exactly like a foreground call. The plan shown to the user and
to the judge says the process stays running.

### Where things are

**Host**
- `ShellRunNode::run` is in `commands/nodes.rs` (about :720); `Tools::approve` is in the same file.
- The terminal's per-dialog cap is in `terminal/service.rs` (about :378) and `close_conversation`
  is at about :522.
- `ShutdownCoordinator::register` and `ShutdownStage::{Stop, Drain, Dispose}` are in
  `platform/shutdown.rs`.
- Dialog removal closes terminals at `ipc/chat.rs:55-60`.
- `CLI_CHANNELS` is at `cli/connection.rs:18-52`, guarded by
  `every_allowed_channel_is_a_contract_channel` (`cli/connection/tests.rs:180`).
- A channel is added in several places: `packages/shared/contract.channels.json`, Zod in
  `packages/shared/src/ipc/contract.ts`, the Rust DTO in `crates/zvs-contract/src/<domain>.rs`,
  fixtures in `packages/shared/fixtures/<domain>/`, and a handler in
  `crates/zvs-host/src/ipc/<domain>.rs`.

**Renderer** (R = `apps/studio/src/renderer`)
- Tool cards are drawn by `R/features/chat/ChatToolCalls.tsx`. `processResult` is at about :179
  and `ProcessPlanView` at about :280 and :322.
- The presentation switch is `toolCallView.ts:147`, and details are in `ToolCallDetails.tsx:203`.
- Copy is in `toolCallCopy.ts`, `toolCallSummary.ts` and `processCopy.ts`.
- The composer is `ChatComposer.tsx`. The terminal dock is `R/features/terminal/`, and
  `TerminalStore.ts` shows the event and attach pattern.

**CLI** (`crates/zvs-cli/src`)
- `enum Command`, `COMMANDS`, `parse` and `Item::Sub` are in `state/palette.rs`.
- `App::command` is at `state/mod.rs:1123`, `open_panel` at :1230 and `App::event` at :1020.
  `App::event` drops events that are not part of the current run.
- `transcript.absorb` (`state/transcript.rs:175`) keeps only `domain == "tool-call"` steps.
- `enum Panel` and `Action` are in `state/overlay.rs`, drawn by `ui/overlay.rs:221`.
- `footer()` is at `ui/mod.rs:391`.
- `enum Shortcut` is in `state/shortcuts.rs`. Ctrl+C, Ctrl+D, Ctrl+J and Ctrl+M are reserved.
- Copy is in `copy.rs`, in Russian.
- `runtime::execute` maps a `Request` to a pipe call.

## Why

Without background processes an agent cannot run a server and test against it, cannot watch a
build, and every long command dies at 120 s. `/goal` (TASK_048_EXTRA_3) depends on this to do
real work. The person also has to be able to see and stop what the agent left running, from
whichever surface they use. With the tray on, the app serves the CLI while its window is hidden.

## Depends on

- `TASK_048_EXTRA_1` — `ExecReview`, `ReviewedAction::Process`, the judge, the process family.
- `TASK_036_EXTRA_3` (done) — `shell.run`, `code.run`, the shared runner.

## Read first

- `/docs/PASSPORT.md` §14 *Process tools*, *Execution policy*, *Judge*, *Terminal*; §7 *The CLI
  pipe* (widening `CLI_CHANNELS` is a security decision); §11 *Concurrent tool calls*; §13
- `CONVENTIONS.md` — Russian copy, `plural()`, `toLocaleString("ru-RU")`, no `мы`
- `design/Chat.dc.html`; warm ZVS design baseline, light and dark

## Scope

**In scope — A. Host**

Contract
- `ProcessPlanDto.background: bool` (default `false`) in Rust, Zod and the fixtures. The judge
  input (`policy/judge.rs` `action_json`) and the approval plan carry it.

The `shell.run` input
- New field `background`, default `false`.
- Background is refused for `skill:<key>/scripts/…` and when `stdin` is given
  (`ValidationFailed`, with a Russian message).

The `BackgroundShells` service (`crates/zvs-host/src/background/`)
- Spawns through the same spec as `shell_spec`.
- Keeps a ring buffer per process with a monotonic `seq`.
- Tracks status `running | exited | stopped | failed | timed-out`, the exit code, start and end
  times, and the owner (a conversation id or an agentic session id).
- Enforces the caps and the lifetime from the DECISION. A call over a cap fails with `Conflict`
  and a Russian message that names the cap.

Model tools (`kernel_tools.rs`, Russian descriptions)
- `shell.output { id, sinceSeq?, tail? }`: side-effect free, tier `auto`, fenced output. It
  returns `status`, `exitCode`, `nextSeq` and `truncated`.
- `shell.kill { id }`: tier `auto`, only for a process the same owner started. Kills the process
  tree.
- `shell.list {}`: tier `auto`, the owner's own processes.
- The model never supplies the owner; the host injects it.

Turn context
- Processes that ended since the dialog's previous turn go into the system prompt: id, argv,
  status, exit code and the last 20 lines, fenced.

Events and channels
- `BackgroundShellStatusDto { domain: "shell", id, ownerId, status, exitCode }` is sent as
  `HostEventDraft::Step` on start and on every status change.
- New channels `shells.list { ownerId? }`, `shells.output { id, sinceSeq? }` and
  `shells.kill { id }`, each with its DTOs, Zod schemas and fixtures.
- All three go into `CLI_CHANNELS`, keeping the list sorted, and the guard test is extended.
  `shells.kill` stops only processes this service started, never an arbitrary pid.

Lifecycle
- The process tree is killed on dialog removal (beside `terminals.close_conversation`), on
  agentic session removal, and in a `ShutdownStage::Stop` task.
- Nothing is resumed after a restart.

**In scope — B. Desktop**

`BackgroundShellStore` (`R/features/backgroundShells/`)
- Loads `shells.list` for the open dialog and applies `domain: "shell"` events.
- Pages output by `sinceSeq` while a view is open, and calls `shells.kill`.
- Takes the bridge and the clock, and is tested with `createFakeBridge()` and
  `createFakeClock()`.

Thread
- A background `shell.run` card shows «В фоне» and its live status, not the frozen meta.
- When it ends, the card shows «Завершён, код N», «Остановлен» or «Превышено время».
- The card's actions are «Вывод», plus «Остановить» while running.
- `shell.output`, `shell.kill` and `shell.list` get compact cards in `toolCallView.ts`.

Composer
- An indicator beside the terminal control, shown only while one of this dialog's processes is
  running: «N фоновых процессов» through `plural()`.
- It opens a popover with program and arguments, folder, run time, status, «Вывод» and
  «Остановить».

Output view
- Placed where the DECISION says. Monospace, follows the tail, and keeps the scroll position
  when the person scrolls up.
- Holds no more in memory than the host buffer size.

Copy goes in `backgroundShellsCopy.ts`, and tests assert against the exported constants.

**In scope — C. CLI**

`/processes` and `Panel::Processes`
- A new `Command::Processes` (`/processes`, `copy::CMD_PROCESSES`).
- One row per process of the current dialog: program and arguments, status, run time, exit code.
- Actions «Вывод» and «Остановить».

Output overlay
- A scrolling tail re-polled with `sinceSeq`. `Esc` closes it and `End` follows the tail.

Events
- `domain: "shell"` status events are applied for the current dialog whatever the current run.

Footer and transcript
- The footer shows «N фоновых процессов» while any of this dialog's processes runs.
- A background `shell.run` row shows «в фоне» and its id, and is updated by status events.

Shortcut, requests and tests
- `Shortcut::Processes`, default chord Alt+B if it is free; keymap id `processes`.
- New `Shells`, `ShellOutput` and `ShellKill` requests, mapped in `runtime::execute`.
- Tests in `state/tests.rs` and `ui/tests.rs` against `copy::` constants.

**In scope — PASSPORT**
- §14: a *Background processes* paragraph under *Process tools*.
- §7: the dated `CLI_CHANNELS` exception.

**Out of scope** (do not do these, even if tempting)
- Background `code.run`
- Writing to a background process's stdin
- Attaching agent processes to the «Терминал» dock as PTYs
- Sandboxing, or changing the process family's row in the policy table
- Background processes on the Tasks page (they are not runs)
- Processes of other dialogs in the CLI panel

## Steps

1. Contract: `background` on `ProcessPlanDto`; the shell DTOs and channel inputs; fixtures;
   `cargo test -p zvs-contract`.
2. The `background/` service, tested with a fake clock: the ring buffer and `seq`, caps,
   lifetime, status transitions, owner checks, kill-tree.
3. The `shell.run` background branch: approval and review exactly as today, then hand the
   process to the service.
4. `shell.output`, `shell.kill` and `shell.list`. Test that one owner cannot read or kill
   another owner's process.
5. The turn-context note for ended processes. Test that it appears once.
6. IPC handlers, `CLI_CHANNELS` and the guard test.
7. Lifecycle hooks, with tests that removing a dialog or a session kills its processes.
8. Desktop: the store and its tests, the cards, the composer indicator and popover, the output
   view. Check light and dark themes and narrow widths.
9. CLI: requests and the runtime mapping, event routing (test that an event outside a run is
   not dropped), the panel and the overlay, the footer and the transcript row, the shortcut and
   its uniqueness test.
10. PASSPORT §14 and §7.

## ❓ DECISION 1 — ask the user before implementing part A

> **Question to ask, verbatim:**
> "What limits should background processes have: how many per dialog, how many in total, how
> long may one live, and how much output is kept?"
>
> **Why it matters:** a forgotten dev server keeps a port and CPU until the dialog is removed or
> the app quits; too tight a cap breaks real workflows.
> **Options:** a — 4 per dialog, 16 total, 60 minutes, 256 KiB; b — 8 per dialog, 32 total, no
> lifetime cap (killed only with the dialog or the app), 1 MiB; c — a, with the lifetime as a
> setting for TASK_052.
> **If the user is unavailable:** a; note it in Handoff.

## ❓ DECISION 2 — ask the user before implementing part B

> **Question to ask, verbatim:**
> "Where should a background process's output open in the desktop app: as a read-only tab in
> the «Терминал» dock, or in a side panel beside the thread?"
>
> **Why it matters:** the dock already has xterm rendering and tabs; a side panel keeps the
> person's own terminals separate from the agent's processes.
> **Options:** a — a read-only tab in the dock, marked as the agent's; b — a side panel in the
> split used for files; c — a modal (simplest; blocks the thread).
> **If the user is unavailable:** a; note it in Handoff.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check:
- In a dialog with a working folder, ask the model to start `npx http-server -p 8099` in the
  background, fetch the page with a second command, and read the log with `shell.output`. The
  approval says the process stays running.
- Desktop: the card says «В фоне» and the composer shows «1 фоновый процесс». «Вывод» follows
  the log, and «Остановить» ends the process and updates the card and the indicator.
- CLI: the transcript says «в фоне» and the footer counts the process. `/processes` tails and
  stops it, and the desktop shows the same state.
- Removing the dialog, or quitting with a process running, leaves no `node.exe` behind.

## Definition of done

- [ ] A background start is approved and reviewed exactly like a foreground `shell.run`
- [ ] One owner can never read, list or kill another owner's process
- [ ] Caps and lifetime are enforced and tested
- [ ] No process survives dialog removal, session removal or app exit
- [ ] Three channels in the contract and in `CLI_CHANNELS`, guard test updated
- [ ] Desktop and CLI show live status and can stop every process
- [ ] Copy Russian, in copy modules, counts through plural forms
- [ ] PASSPORT §14 and §7 updated
- [ ] Gates pass; `pnpm build` and `zvs-studio` clippy left to CI if not run locally
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
