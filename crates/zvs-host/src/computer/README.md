# computer

Computer use: the model sees and operates the user's Windows desktop (screenshots, clicks,
typing, keys, UI Automation) inside apps the user granted for the conversation. The desktop
work itself runs in a Python sidecar from the bundled `computer-use` skill; this module is the
policy, the approvals, the session state shown in the overlay, and the kernel tools.

## Files

| File          | Contents                                                                                    |
| ------------- | ------------------------------------------------------------------------------------------- |
| `nodes.rs`    | The seven `computer.*` tools and their checks                                               |
| `policy.rs`   | App name normalisation, `classify` (forbidden / sensitive / normal), `sensitive_keys`       |
| `agent.rs`    | `ComputerAgent`: the session (one conversation at a time), actions, grants, approvals, stop |
| `progress.rs` | Folding host events into the overlay feed and deriving its status                           |
| `sidecar.rs`  | `DesktopPort`, `UvSidecar` (JSON-lines RPC to `scripts/computer_server.py`), `NoDesktop`    |
| `settings.rs` | `command_enabled`: the `computer.enabled` and browser-use switches for slash commands       |

## Tools

| Tool                      | Does                                                                                        |
| ------------------------- | ------------------------------------------------------------------------------------------- |
| `computer.screenshot`     | Captures a display; coordinates in later calls are pixels of this image                     |
| `computer.act`            | 1–12 actions (`ACTIONS_MAX`): click, move, scroll, drag, type, key, wait; then a screenshot |
| `computer.zoom`           | A region at full resolution                                                                 |
| `computer.inspect`        | UI Automation elements of the foreground window, with refs like `e12` usable in `act`       |
| `computer.apps`           | Open windows, displays, granted apps                                                        |
| `computer.open_app`       | Start or focus a granted app                                                                |
| `computer.request_access` | Ask the user to grant apps for this conversation                                            |

## Pipeline: `computer.act`

1. **Probe.** The sidecar resolves every action to the window it would hit (and, for drags,
   the window at the end point) without acting.
2. **Window check** for each target: the app's own windows (by pid) are refused; an unknown
   executable is refused; `classify` refuses forbidden apps outright and marks sensitive ones;
   apps without a grant are collected.
3. **Grants.** Missing apps are declared as resources; the access pipeline asks for them in the
   same call, before the action approval. With no grant at all the call fails with `NO_APPS`.
4. **Approval need.** `Plain` by default; `Sensitive` when a target app is sensitive or a key
   combo is a system combo. Steps 1–4 run in `prepare`, and `access` declares
   `Effect::Desktop { apps, sensitive }`; a sensitive action is raised to `ask` so it is reviewed
   every time. Sensitive input into a terminal window (`TERMINAL_APPS`) is the `Process` family
   and is never allowed by rules or the judge: it asks in every mode but `no-restrictions`.
5. **Act.** The sidecar performs the actions (password fields are refused on its side, text
   is capped at `TEXT_MAX`) and returns a screenshot; each action is shown in the overlay as
   running, waiting or done.

## Policy (`policy.rs`)

- **`normalize_app`**: takes the last path segment, lowercases it, requires
  `^[\p{L}\p{N} ._()+\-]+$` and at most `APP_NAME_MAX` (128) characters, appends `.exe`.
- **Forbidden** (`FORBIDDEN_APPS`, the user only): UAC consent, credential UI, logon and lock
  screens, and password managers (KeePass, KeePassXC, 1Password, Bitwarden, Dashlane,
  LastPass, Enpass, Proton Pass).
- **Sensitive** (`SENSITIVE_APPS`, approval on every action): shells and terminals (`cmd`,
  PowerShell, Windows Terminal, WSL, bash, mintty, conhost), `regedit`, `mmc`, Task Manager,
  Settings, Control Panel, `msiexec`, `rundll32`, Visual Studio.
- **`sensitive_keys`**: any combo with the Windows key; Ctrl+Alt+Delete/End; Ctrl+Shift+Esc.
  Key names are compared case-insensitively with whitespace removed; `valid_combo` allows at
  most five keys of at most 16 characters.

## Session state (`agent.rs`, `progress.rs`)

`ComputerAgent::begin` binds the desktop to one conversation and run; another conversation
cannot start while one is active. Host events for that run are absorbed into a feed of tool
calls and approvals (`progress::absorb`). The overlay status is derived, not stored
(`derived_status`): `Idle` and `Stopping` pass through; otherwise `Awaiting` if any approval,
question or action is waiting, `Working` if anything runs, else `Starting`. The user can stop
from the overlay or with Shift+Esc (see the shell's `platform/README.md`); stop cancels the
run and closes the sidecar session.

## Sidecar protocol (`sidecar.rs`)

`UvSidecar` starts `uv run scripts/computer_server.py` from the skill folder on first use
(`START_TIMEOUT` 240 s, covering the first dependency install), checks a handshake with
`PROTOCOL = 1`, then sends `{ id, method, params }` lines and waits for the reply with the
same `id`: `CALL_TIMEOUT` 45 s, `ACT_TIMEOUT` 180 s. The last 20 stderr lines are kept for
diagnostics. A refusal from the sidecar (`DesktopFault::Refused`) carries a code and the
window it concerns; a crash or timeout is `Unavailable` and the next call restarts it.
