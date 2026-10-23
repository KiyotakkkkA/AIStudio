---
task: TASK_036_EXTRA_6
title: Notification activation through the `zvs-studio://` protocol — Action Center clicks and cold start
phase: 6 — Tools, MCP and Skills
status: todo
depends_on: [TASK_036_EXTRA_5]
unblocks: []
estimate: 4 h
---

# TASK_036_EXTRA_6 — `zvs-studio://` protocol activation

## Context

TASK_036_EXTRA_5 shows native toasts on `Windows.UI.Notifications` and opens the toast's
target when the toast is clicked. That click is delivered only through the in-process
`Activated` event. So it works while the popup is on screen and the app is running, either
open or in the tray.

Two cases do nothing today:

- **A click on a toast in the Action Center** after the popup has closed. For an app that is
  not packaged (MSIX), Windows does not route that click back to the running process's
  `Activated` handler. It needs a registered way to activate the app.
- **A click after the app has quit.** Nothing is running to receive it.

Electron apps (Slack, Discord, Teams) cover these cases with installer-side registration. This
task does the same with a URL protocol, the lighter of the two Windows mechanisms. The other one
is a COM activator: a CLSID server, plus `ToastActivatorCLSID` on the Start Menu shortcut.

TASK_036_EXTRA_5 has already prepared the ground. Every toast carries its target as
`launch="zvs-studio://…"`, and `NotificationTarget::from_url` parses it strictly. This task
switches the activation type to `protocol` and teaches the app to receive the URL.

## Why

People come back to the computer and click the notification in the Action Center, not the
popup. Without this task, «Ответ готов» in the list is a dead entry, and the native setup from
TASK_036_EXTRA_5 does not yet behave as notifications do in Electron.

## Depends on

- `TASK_036_EXTRA_5`: `WindowsToaster`, `NotificationTarget::to_url` / `from_url`,
  `NotificationService::activated`, `notifications.watch`, the AUMID registration.

## Read first

- `/docs/PASSPORT.md` §14 (security: this is a new entry point any web page can call)
- `apps/studio/src-tauri/src/lib.rs` (the single-instance plugin, `run`, `Boot`),
  `window.rs`, `platform/toast/`
- `tauri.conf.json` (`bundle`, the NSIS / MSI settings)

## Scope

**In scope**

*Protocol registration*
- `tauri-plugin-deep-link`. Resolve the latest stable version and pin it exactly. Enable its
  integration with `tauri-plugin-single-instance` (the `deep-link` feature on single-instance).
  Declare the scheme `zvs-studio` in `tauri.conf.json` (`plugins.deep-link.desktop.schemes`),
  so the NSIS and MSI installers register it under `HKCU\Software\Classes\zvs-studio`, and the
  uninstaller removes it.
- Dev builds register the scheme at runtime (`register("zvs-studio")`), so the flow can be
  tested without installing. Release builds rely on the installer, and runtime registration
  is skipped there. Note in Handoff that dev registration points the scheme at the dev exe.

*The toast*
- Change `WindowsToaster` to `activationType="protocol"`. Keep the `launch` URL as it is.
  Keep the in-process `Activated` handler only if Windows still raises it for protocol
  toasts. If it does, de-duplicate it so one click opens the target once: keep the last
  activation time per tag and ignore a second delivery within 2 s.

*Receiving the URL*
- **Warm start** (the app is running): the single-instance callback, or the deep-link
  `on_open_url` event, receives the URL. Parse it with `NotificationTarget::from_url`. If that
  fails, log and ignore it. Otherwise call `NotificationService::activated(target)`, which
  shows and focuses the window as in TASK_036_EXTRA_5.
- **Cold start** (the app was not running): read the URL from the launch arguments at
  `compose`. `NotificationService` holds **one** pending target and delivers it to the first
  `notifications.watch` subscriber. A cold start from a toast always opens the window, even
  when the app started with `--autostart` or the tray is on.
- Only the first argument that starts with `zvs-studio://` is considered. Any other
  arguments are not touched by this path.

*Security* (PASSPORT §14)
- The protocol is reachable from any web page, any document and any other program. It is
  **navigation only**: it may open the chat route with a conversation selected, or
  «Загрузки». It never approves, runs, sends, deletes or changes anything, and it never
  carries text that gets rendered.
- `from_url` already rejects anything other than the two shapes. Additionally:
  - A conversation id that does not exist falls back to the chat route with no selection,
    and the user sees no error.
  - URLs longer than 256 characters are dropped before they are parsed.
- Add one PASSPORT §14 paragraph: the `zvs-studio://` scheme is an untrusted navigation-only
  entry point, parsed in one place.

## Out of scope (do not do these, even if tempting)

- A COM activator (`ToastActivatorCLSID`) or toast buttons with background activation. These
  are for «Разрешить» / «Отклонить» on the toast itself, which is a separate decision.
- Any other `zvs-studio://` routes (opening a document, a store, a setting). Add them only
  with their own tasks and their own security notes.
- Packaging as MSIX.
- macOS and Linux URL handlers beyond what the plugin does by default.

## Steps

1. Pin the plugin and enable the single-instance integration. Declare the scheme, and add dev
   runtime registration.
2. The warm start path through the single-instance callback / `on_open_url` → `from_url` →
   `activated`.
3. The cold start path: launch args → the pending target in `NotificationService` → delivery
   to the first subscriber.
4. Switch `WindowsToaster` to protocol activation, de-duplicating if needed.
5. PASSPORT §14 paragraph.

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- `NotificationService`: a pending target from a cold start is delivered once to the first
  subscriber and not to later ones. A second activation replaces a target that has not been
  delivered yet.
- Argument scan: picks the first `zvs-studio://` argument, ignores the rest, and drops
  URLs that are too long or malformed without an error.
- Toast XML: `activationType="protocol"` with the unchanged `launch` URL.
- De-duplication: two deliveries of one click within 2 s open the target once.

Manual check on the installed build:
- Finish a turn, wait for the popup to close, and click the toast in the Action Center. The app
  opens on that conversation.
- Quit the app from the tray and click a remaining toast. The app starts on that conversation.
- Run `start zvs-studio://conversation/does-not-exist` and `start zvs-studio://evil/x`. The
  first opens the chat page with no selection. The second does nothing visible and logs one
  line.
- Uninstall the app and check that `HKCU\Software\Classes\zvs-studio` is gone.

## Definition of done

- [ ] `zvs-studio://` registered by the installer and removed by the uninstaller; dev
      registration at runtime
- [ ] Action Center clicks and clicks after quitting open the toast's target
- [ ] Cold start delivers the pending target once
- [ ] Navigation only, strict parsing, length cap; PASSPORT §14 updated
- [ ] Plugin pinned exactly
- [ ] Gates pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

_To be filled in by the implementing session._
