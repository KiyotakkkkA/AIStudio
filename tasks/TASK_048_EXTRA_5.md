---
task: TASK_048_EXTRA_5
title: Background code and scripts, and waking the dialog when a process finishes
phase: 9 — Integrations
status: todo
depends_on: [TASK_048_EXTRA_2]
unblocks: []
estimate: 6–8 h (host, desktop and CLI; one commit)
---

# TASK_048_EXTRA_5 — Background code and scripts, and waking the dialog when a process finishes

## Context

TASK_048_EXTRA_2 added background processes, but only for plain `shell.run` argv commands:

- `shell.run` takes `background: true` and returns a process id at once.
- The process is supervised by `BackgroundShells` (`crates/zvs-host/src/background/`): at most
  4 per owner and 16 in total, a 60-minute lifetime, and a 256 KiB buffer addressed by a byte
  `seq`.
- The model reads it with `shell.output`, `shell.list` and `shell.kill`.
- Each process has its own event stream: `token` events for output, and status steps with
  `domain: "shell"`.
- A process that ended is listed in the system prompt of the dialog's **next** turn, once
  (`ChatService` `ended_shells_note`, via `take_ended`).

Two gaps remain:

1. `code.run` and skill scripts (`argv[0] = skill:<key>/scripts/…`) cannot go to the
   background. A long Python data job or a skill script that builds a large file still dies
   at 120 s.
2. The model only learns that a process finished when the person writes again. An agent that
   starts a build or a test run cannot react when it finishes, so it either blocks or polls
   `shell.output`.

This task closes both gaps. Background mode is extended to `code.run` and skill scripts. The
model can also ask to be **woken** when a background process finishes: the host then starts a
new turn in that dialog, carrying the process result, without a message from the person.

## Why

Without waking, a background build or test run is fire-and-forget. With it, the agent can start
the work, carry on or finish its answer, and continue from the result when it arrives, the way
a person would. `/goal` (TASK_048_EXTRA_3) benefits directly, because a goal can wait for a
build instead of polling.

## Depends on

- `TASK_048_EXTRA_2` — the background service, tools, events and the ended-process note.

## Read first

- `/docs/PASSPORT.md` §14 *Background processes*, *Process tools*, *Execution policy*, *Created
  files*; §11 (runs, approvals, the question gate); §7 *The CLI pipe*
- `crates/zvs-host/src/background/service.rs`, `nodes.rs`
- `crates/zvs-host/src/commands/nodes.rs` — `CodeRunNode`, the skill-script branch of
  `ShellRunNode`, `prepare_scratch` (scratch folders are pruned to the newest 16)
- `crates/zvs-host/src/services/chat.rs` — how a turn starts (`send`, `require_idle`), the
  ended-process note

## Scope

**In scope — background code and scripts**
- `code.run` gets `background: true`, with the same approval, policy row and judge input as a
  foreground run. `ProcessPlanDto.background` is allowed together with `language`: relax the
  refinement on both sides and update the fixtures.
- The scratch folder of a background `code.run` is pinned while the process runs, so
  `prepare_scratch` never prunes it, and it is released when the process ends. Any npm install
  runs in the foreground first, as today; only the program goes to the background.
- Skill scripts can go to the background. Their created files are collected when the process
  ends, the same way `shell.run` collects them today, and reported on the wake-up turn (below)
  or in the ended-process note.

**In scope — waking**
- A `notify: true` flag on all three background starts. Only background starts can carry it,
  and it is refused together with a server-like intent. The tool description tells the model
  to use `notify` for work that finishes, such as builds, tests and jobs, and never for
  servers or watchers. The tool result says plainly that the dialog will be continued when the
  process ends.
- When a `notify` process ends, the host starts a turn in its dialog:
  - It runs only if the dialog still exists.
  - If a turn is already running, the wake-up queues and starts when that turn ends.
  - Processes that end close together are coalesced into one wake-up.
- The wake-up turn needs no user message. A new message kind (for example `role: "event"`, or
  a `kind` field on `MessageDto`) holds the process result: status, exit code and the last lines
  of output, fenced as untrusted. It is persisted and shown in the thread. This is a contract
  change, so it goes in Rust, Zod and the fixtures.
- The same tools, approvals, questions and execution policy apply as in any turn. A wake-up
  never widens anything.
- Limits:
  - At most N consecutive wake-ups without a message from the person (see the DECISION).
    After that, results go back to the passive next-turn note.
  - Nothing wakes after an app restart.
  - The global run concurrency limits apply.
- Agentic sessions: out of scope. Their loop already continues on its own.

**In scope — desktop and CLI**
- Desktop:
  - The event message renders as a compact card: «Фоновый процесс завершён», the command, the
    exit code, and a «Вывод» link to the dock tab from TASK_048_EXTRA_2.
  - A turn started by a wake-up is marked as such in the thread.
- CLI: the same card as a notice line. A wake-up turn streams like any turn, including while
  the person is idle at the prompt.

**In scope — PASSPORT**
- §14 *Background processes*: code and scripts, `notify`, wake-ups and their limits.
- §11: a wake-up is a chat turn with an event message.

**Out of scope** (do not do these, even if tempting)
- Waking on output patterns (for example "server is ready"); exit only
- Waking agentic sessions
- Resuming or waking after a restart

## Steps

1. Contract: relax `background` with `language`, add the event message kind, fixtures.
2. Background `code.run`: pinned scratch, install in the foreground, start through
   `BackgroundShells`; tests.
3. Background skill scripts: collect artifacts on exit; tests.
4. `notify` on the three starts; validation and tool copy; tests.
5. Wake-up scheduler in the chat service: subscribe to ended `notify` processes, coalesce,
   queue behind a running turn, cap consecutive wake-ups, start the turn with the event
   message. Tests with fake drivers for each rule.
6. Desktop event card and wake-up marker; CLI notice and streaming.
7. PASSPORT.

## ❓ DECISION — ask the user before implementing the wake-up part

> **Question to ask, verbatim:**
> "How many wake-up turns in a row may a dialog take without a message from you, and should a
> wake-up also happen while the execution policy is «Всегда спрашивать»?"
>
> **Why it matters:** each wake-up spends tokens unattended; in always-ask mode a wake-up turn
> will usually stop at the first approval anyway.
> **Options:** a — 3 in a row, wake in every mode; b — 1 in a row, wake in every mode;
> c — 3 in a row, but in «Всегда спрашивать» only the passive next-turn note.
> **If the user is unavailable:** a; note it in Handoff.

## Verify

```bash
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Manual check:
- Ask for a `cargo test` in the background with `notify`, and let the turn end. When the tests
  finish, the dialog continues on its own with the result and the thread shows the event card.
- Start two notified jobs that end together: there is one wake-up.
- Write while a wake-up is pending: your turn runs first, and the wake-up follows.
- A background `code.run` longer than 120 s finishes, and its scratch folder survives until it
  ends.

## Definition of done

- [ ] `code.run` and skill scripts run in the background under the same approval and policy
- [ ] `notify` wakes the dialog once per batch of finished processes, queued behind a running turn
- [ ] Consecutive wake-ups are capped; nothing wakes after a restart
- [ ] The event message is in the contract, persisted, and rendered in desktop and CLI
- [ ] PASSPORT updated
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

> Filled in by the agent that completes this task. Delete this line when writing it.

**Built:**
**Deviated from plan:**
**Next session should know:**
