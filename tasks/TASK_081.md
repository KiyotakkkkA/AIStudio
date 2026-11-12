---
task: TASK_081
title: Caller identity and the console pipe
phase: A — Access pipeline (APS)
status: done
depends_on: []
unblocks: [TASK_082]
estimate: 4 h
---

# TASK_081 — Caller identity and the console pipe

## Context

The host is reached two ways (PASSPORT §7): the `studio` webview through `ipc_call`, and the
`aistudio` console client over the named pipe `\\.\pipe\zvs-ai-studio-cli-<user SID>`
(`crates/zvs-pipe`, `apps/studio/src-tauri/src/cli.rs`, `crates/zvs-host/src/cli/connection.rs`).
The pipe is owner-only and dispatches through the same `IpcServer`, limited to the channels in
`CLI_CHANNELS`.

`IpcServer::dispatch` does not know who is calling. `docs/APS_AUDIT.md` found:

- **H15** — `execPolicy.set` is on the list, and the `no-restrictions` confirmation lives in the
  clients. Any process running as the same user can connect, switch the mode off, send a prompt
  and so drive the logged-in browser profile, mail and connected accounts — things it cannot
  reach by itself.
- **M16** — a connection subscribes to the whole event bus and `runs.approve`, `runs.deny` and
  `folders.answer` accept any run id, so it sees every dialog and can answer approvals raised in
  the studio window.
- **L17** — the allow-list is a flat array; nothing in code says which channels loosen access.

The pipe cannot tell the real `aistudio.exe` from another process of the same user, and on
Windows it cannot do so reliably. The aim is that a pipe caller can never *loosen* access
without the person confirming it in the studio window, and that it reaches only its own dialogs.

## Why

The console client is convenient precisely because it shares the studio's accounts and browser
profile. That makes the pipe a way to borrow them.

## Depends on

Nothing. This task is independent of the pipeline tasks and can run first.

## Read first

- `/docs/APS_AUDIT.md` — *Local caller*
- `/docs/PASSPORT.md` §7 *The CLI pipe* — every exception listed there stays
- `crates/zvs-host/src/cli/{connection,mod}.rs`, `ipc/{server,mod}.rs`
- `apps/studio/src-tauri/src/{cli,commands}.rs`

## Scope

**In scope**
- `Caller { Studio, Cli { connection }, BrowserUi, Panel }` passed to every handler through
  `IpcServer::dispatch`.
- A channel policy table replacing `CLI_CHANNELS`: per channel, the callers allowed and an
  `effect` of `read`, `act` or `loosen`. A test asserts every registered channel has a row.
- **`loosen` from the console needs the studio.** `execPolicy.set` to a less strict mode, the
  allow-rule channel, and an `always` answer on a folder or tool approval are held and shown in
  the studio window as a confirmation naming the change and its source; they apply only on
  confirmation. Tightening applies at once. With no studio window, the call fails with
  `PermissionDenied` and a Russian message constant.
- **Connection scope.** A connection owns the dialogs it created or opened with `cli.open`.
  It receives events for those only; `runs.approve`, `runs.deny`, `folders.answer`,
  `chat.answer`, `chat.cancel`, the queue and goal channels refuse another dialog's ids.
  Opening an existing dialog by id from the console is a `loosen` the first time.
- **Client image check.** On connect, read the client's process id
  (`GetNamedPipeClientProcessId`) and image path; log both, and mark the connection
  `recognised` when the path is the installed `aistudio.exe`. An unrecognised client is treated
  as above and is additionally shown in the confirmation text. This raises the bar; it is not
  a boundary, and the README says so.
- A studio surface that lists active console connections with «Отключить».
- PASSPORT §7 rewritten around the policy table; `cli/README.md`.

**Out of scope** (do not do these, even if tempting)
- Authenticating the client with a secret: a same-user process can read whatever the client can.
- Removing any channel the console uses today.
- Changing the pipe DACL or the protocol version, unless the decision below needs it.

## Steps

1. Thread `Caller` through `IpcServer` and both entry points.
2. Policy table and the every-channel-has-a-row test; delete `CLI_CHANNELS`.
3. Connection-owned dialogs and filtered events.
4. The held-confirmation flow: host state, a studio event, an answer channel, a timeout.
5. Client image check and the connections list.
6. Console client: show «ожидает подтверждения в приложении» for a held change.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Switching the mode to a looser one from the console will need a click in the app window.
> Should that apply (a) to every loosening change, (b) only to `no-restrictions` and allow
> rules, leaving `auto` switchable from the console, or (c) be configurable with (a) as the
> default?"
>
> **Why it matters:** (a) closes the borrowed-access path completely and costs one click per
> change; (b) leaves `auto` — which admits what rules allow — reachable by any local process.
> **Options:** a / b / c as above.
> **If the user is unavailable:** take (a) and note it in Handoff.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p zvs-host cli:: ipc::
cargo test -p zvs-cli -p zvs-pipe
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Manual check (Windows, built app): from the console set `no-restrictions` → the studio asks;
decline → the mode is unchanged. Start a dialog in the studio that raises an approval → the
console neither shows nor can answer it. `zvs-studio` clippy and e2e need Windows; say in the
PR what was not run.

## Definition of done

- [ ] Every handler receives a `Caller`; every channel has a policy row
- [ ] A console caller cannot loosen access without a studio confirmation
- [ ] A connection sees and answers only its own dialogs
- [ ] Connections are logged with client image and listed in the studio
- [ ] PASSPORT §7 and `cli/README.md` updated
- [ ] All runnable gates pass; the ones not run are named
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- **Caller.** `ipc::Caller { Studio, Cli { connection }, BrowserUi, Panel }`.
  `IpcServer::dispatch(caller, channel, payload)` passes it to every handler (`handle_as`;
  `handle` ignores it). `ipc_call` dispatches as `Studio`, the pipe as `Cli { connection }`.
- **Policy table (L17).** `ipc/policy.rs` `POLICY` has one row per contract channel, in contract
  order: callers, `Effect::{Read, Act, Loosen}` and `Scope::{Any, Dialog, Run, Shell, Owner}`.
  `policy/tests.rs` checks that every contract channel and every registered channel has a row,
  that the console keeps its 47 channels, and which console rows loosen. `CLI_CHANNELS` is
  deleted; `serve` and `dispatch` both use the table. `CLI_CHANNEL_REFUSED` moved to `ipc`.
- **Held loosening (H15).** `cli/gate.rs` `admit` runs before the handler for a console
  caller. `execPolicy.set` to a less strict mode or another judge, `execPolicy.allowRule`, and
  `always` on `runs.approve` / `folders.answer` wait in `CliConnections::confirm`. That shows
  the studio window (`ShellPort::show_studio`), emits a `step` with domain `cli` and sends the
  connection a `consent { pending }` notice. The change applies only on
  `cli.consents.answer { confirm: true }`. A decline, 60 s, a closed connection, or no studio
  sink on the bus (`EventBus::attached`) fail it with `PERMISSION_DENIED` and a Russian
  constant. Tightening applies at once.
- **Connection scope (M16).** A connection owns the dialogs it created or opened. A call
  naming another dialog's id, run or background process is refused (`CLI_DIALOG_REFUSED`).
  Events are filtered in the writer task by the run's `subject_id` (`RunService::
  subject_of_stream`, cached per stream). Opening an existing dialog (`chat.conversations.get`)
  is confirmed once and then stays released until the app restarts.
- **Client image.** `zvs_pipe::client_process_id`, `process_image`, `same_image`. The shell
  logs pid and image and marks the connection `recognised` when it is the installed
  `aistudio.exe`. The confirmation names an unrecognised image.
- **Studio.** `CliAccessStore` with `CliConsentModal` in `AppFrame`.
  «Параметры → Командная строка» gains the switch «Подтверждение ослабления доступа из
  консоли» (`cli.confirmLoosening`, on by default) and «Активные сессии» with
  «Отключить» (`cli.connections.disconnect` cancels the connection).
- **Console.** The status line shows «Ожидает подтверждения в приложении» while a change is
  held.
- New channels `cli.connections.list|disconnect`, `cli.consents.list|answer`. New DTOs
  `CliConnectionDto`, `CliConnectionRef`, `CliConsentDto`, `CliChangeDto` and
  `CliConsentAnswerInput` in Rust and Zod with fixtures. New `CliNotice::Consent`.
- PASSPORT §7, `cli/README.md`, `ipc/README.md` and `zvs-cli/README.md` updated.

**Deviated from plan:**

- Decision: the user chose (c), configurable, with (a) as the default. The switch lives on the
  CLI preferences page. With it off, mode and judge changes other than to `no-restrictions`
  pass without a confirmation; `no-restrictions`, allow rules, `always` answers and opening a
  dialog are confirmed either way. Also by user decision: a hidden tray window is brought
  forward rather than failing, the timeout is 60 s, and a released dialog stays released until
  the app restarts.
- The switch hint uses the app's mode label «Автоматическая проверка» where the chosen wording
  said «Авто».
- "Opened with `cli.open`": `cli.open` takes only a folder, so a dialog is opened with
  `chat.conversations.get`, which is what the console already calls for `--resume` and the
  dialog picker. The protocol version and the pipe DACL are unchanged.
- `shells.list|output|kill` are scoped to the connection's dialogs too (through the process
  owner), as are `chat.skip`, `chat.attach`, `chat.context`, `chat.bindFolder`,
  `chat.conversations.updateStores`, `cli.images.import` and `cli.subagents.cancel`.

**Next session should know:**

- Not run here: `pnpm build`, `pnpm test:e2e` and the manual check against a built app. Run
  on Windows: `cargo test --workspace` minus `zvs-studio` ran per crate (`zvs-host`,
  `zvs-contract`, `zvs-pipe`, `zvs-cli`, `zvs-studio`); workspace clippy, fmt, typecheck, lint
  and test passed.
- One `cargo test -p zvs-studio` run inside a multi-package invocation reported a failure
  without naming a test; three reruns passed.
- `pnpm format:check` flags `apps/studio/src/renderer/features/execPolicy/rulesCopy.ts`. That
  file was already unformatted at `a2bd56d` and is left as is.
- Mode changes to `safety-ask` count as loosening and are confirmed while the switch is on.
- `CliConsentDto` carries the image but not the pid; the connections list shows both.
