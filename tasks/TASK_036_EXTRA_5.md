---
task: TASK_036_EXTRA_5
title: Preferences «Общие» and «Уведомления» — start with the system, the tray, OS notifications
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_034_EXTRA_1, TASK_036_EXTRA_4]
unblocks: [TASK_036_EXTRA_6]
estimate: 8 h
---

# TASK_036_EXTRA_5 — «Общие» and «Уведомления» in the preferences dialog

## Context

The cog in the nav rail opens the «Параметры» dialog (`features/preferences/PreferencesDialog.tsx`).
Its nav has one group, «Приложение» (`PREFERENCES_GROUP`), with two sections: «Внешний вид» and
«О системе» (`PreferenceSection` in `stores/UiStore.ts`, labels in
`preferencesPresentation.ts`).

The app has no way to tell the user that something happened while they are looking elsewhere.
A tool call waits for approval, a long answer finishes, or a model download completes, and
nothing shows unless the window is in front. The app also cannot start with the system, and
closing the window quits it. Because of that, nothing can run in the background.

Decided with the user (2026-10-23):

- **Two new sections in the «Приложение» group: «Общие» and «Уведомления».**
- **«Общие»** holds two toggles, stacked in this order: «Запускать вместе с системой» and
  «Значок в области уведомлений».
- **«Уведомления»** holds a master toggle that turns notifications on or off. While it is on,
  three more toggles appear below it, one per event type: an approval is required, a chat answer
  finished, a download finished. While it is off, the three toggles are hidden.
- Notifications are native OS notifications (Windows toasts), not in-app toasts. They work
  like Electron's `Notification`: a toast appears for every enabled event, whatever window is
  in front, including the studio's own.
- **Clicking a toast opens what it is about.** A «Ответ готов» or «Требуется подтверждение»
  toast opens the app on that conversation. A download toast opens «Загрузки».

## Why

Tool approvals (TASK_034_EXTRA_1) and agentic work in the working folder (TASK_036_EXTRA_1…4)
make turns long, and those turns stop to wait for the user. Without a notification, the user
has to keep checking the window. The tray and autostart let the app keep running in the
background, so downloads, indexing and the coming scheduled scenarios (Phase 7) continue
there, and notifications are how that background work reaches the user.

## Depends on

- `TASK_034_EXTRA_1`: approval requests (`HostEventDraft::Approval`, `ApprovalRequestDto`).
- The preferences dialog (commit `cd6465f`): `PreferenceSection`, `SettingRow`, the
  `ui.preferences` state, and the theme port pattern for applying a setting in the shell.

## Read first

- `/docs/PASSPORT.md` §7 (IPC, the event channel), §8 (host layering, ports), §14 (security),
  §15 (cross-cutting)
- `/CONVENTIONS.md`: every string below is user-facing copy, and that includes the notification
  text
- `apps/studio/src/renderer/features/preferences/PreferencesDialog.tsx`,
  `preferencesPresentation.ts`, `stores/UiStore.ts`, `tests/renderer/preferences.test.ts`
- `crates/zvs-host/src/ipc/settings.rs` and `platform/theme.rs`: how a setting is applied
  through a shell port at `settings.set` and at startup
- `apps/studio/src-tauri/src/lib.rs` (`run`, `on_run_event`, the single-instance plugin,
  `Lifecycle` / shutdown), `window.rs` (`focus_studio`), `theme.rs` (the port implementation)
- Where the three events originate: the approval request in the kernel
  (`kernel/scheduler.rs`), turn completion in the chat service, and download completion in
  `downloads/service.rs`

## Scope

**In scope**

*Settings keys* (stored through the existing settings service, JSON booleans)

| Key | Default |
| --- | --- |
| `app.autostart` | `false` |
| `app.tray` | `false` |
| `notifications.enabled` | `true` |
| `notifications.approval` | `true` |
| `notifications.turn` | `true` |
| `notifications.download` | `true` |

The OS is the source of truth for `app.autostart`. At startup the host reads the actual
registration and writes it back to the setting if they differ, because the user can remove the
entry in Task Manager.

*Host: the shell port (`crates/zvs-host/src/platform/shell.rs`)*
- A `ShellIntegration` port, injected like `ThemeSource`: `set_autostart(bool) -> Result`,
  `autostart_enabled() -> Result<bool>`, `set_tray(bool)`, `notify(NotificationDraft)`.
  `NotificationDraft` holds the title, the body and a `NotificationTarget`
  (`Conversation { id }` or `Downloads`). A `FakeShell` in `testing` records the calls and can
  simulate a click.
- `settings.set` applies `app.autostart` and `app.tray` through the port, following the same
  pattern as `ui.theme`. If `set_autostart` fails, the call fails with an `AppError` whose
  message is Russian copy (see Copy), and the stored value stays unchanged. Startup applies
  `app.tray`, then reconciles `app.autostart`.

*Host: the notification service (`crates/zvs-host/src/notifications/`)*
- `NotificationService::raise(kind, draft)` checks `notifications.enabled` and the per-kind
  key, then calls `notify` on the port. It holds the policy, and the three call sites only
  describe the event:
  - **Approval** (`approval`): raised when the kernel publishes an approval request. Title
    «Требуется подтверждение». Body: the tool's display name and the conversation title.
  - **Turn** (`turn`): raised when a chat turn ends. Title «Ответ готов» on success, or
    «Ответ не получен» on failure. Body: the conversation title. A turn the user cancelled
    raises nothing. A turn that stops for approval raises only the approval notification.
  - **Download** (`download`): raised when a download completes or fails. Title «Загрузка
    завершена» or «Загрузка не удалась». Body: the item's display name.
- No foreground suppression. Whether the studio window is focused makes no difference (see
  DECISION).
- **Activation.** When a toast is clicked, the shell calls
  `NotificationService::activated(target)`. That shows and focuses the studio window (taking it
  out of the tray if it is hidden) and publishes the target to the renderer through
  `notifications.watch` (below).
- Coalescing: notifications of one kind for one conversation within 5 s collapse to the
  latest one. Downloads collapse per item. Use the injected clock.
- Notifications never include message text, file contents or paths (PASSPORT §14). Only
  titles and display names.

*Contract*
- `NotificationTargetDto` (`{ kind: 'conversation', conversationId } | { kind: 'downloads' }`)
  and the `notifications.watch` / `notifications.unwatch` / `notifications.clear` channels. These follow the
  `workspace.watch` pattern: the stream carries `step` events whose payload is the target, so
  the event envelope is unchanged. The renderer subscribes once at startup, in `RootStore`.
  Each DTO needs the Rust type, the Zod schema and a golden fixture in
  `packages/shared/fixtures/notifications/`. Add the channels to the IPC channel list test.

*Shell (`apps/studio/src-tauri`)*
*Shell: the toaster (`apps/studio/src-tauri/src/platform/toast/`)*

The toasts are built directly on the Windows notification API
(`Windows.UI.Notifications`) through the `windows` crate, which is already a dependency.
Add the `UI_Notifications`, `Data_Xml_Dom` and `Foundation` features at the pinned
`=0.62.2`. No wrapper crate is used:

- `tauri-plugin-notification` has no click callback on desktop, because the Actions API is
  mobile only.
- `tauri-winrt-notification` has a click callback but cannot set the toast's `launch`
  argument, tag, group or activation type. Those are what Action Center clicks
  (TASK_036_EXTRA_6), replacing toasts in place and removing stale toasts all depend on.

Owning the XML takes more work up front, but every later feature becomes an attribute rather
than a library swap.

- **A `Toaster` trait** in the shell, implemented by `WindowsToaster`. The port's `notify`
  calls `show(draft)`. On other platforms a `PluginToaster` implements the same trait over
  `tauri-plugin-notification`, with no click handling, so the build still compiles there.
  Windows is the only verified target.
- **App identity.** At startup, register the AppUserModelID once, per user and idempotently.
  Use the bundle `identifier` from `tauri.conf.json` and write it under
  `HKCU\Software\Classes\AppUserModelId\<identifier>` with `DisplayName` «ZVS AI Studio» and
  an `IconUri` that points to the installed icon. Then call
  `SetCurrentProcessExplicitAppUserModelID`. This way toasts show the app's name and icon
  instead of «PowerShell», in dev builds as well as installed ones. Check whether the
  installer's Start Menu shortcut carries the same AUMID, set it if the bundler does not, and
  note the result in Handoff. The uninstaller removes the registry key.
- **The toast XML.** Use a `ToastGeneric` binding with the title and the body. Escape every
  string as XML text, never by concatenation. Do not show an app logo override, and do not add
  audio beyond the system default.
  - The `launch` attribute always carries the target as a URL: `zvs-studio://conversation/<id>`
    or `zvs-studio://downloads`. A single `NotificationTarget::to_url` / `from_url` pair in
    `zvs-contract` builds and parses it, with round-trip tests. Parsing accepts only these
    shapes and a well-formed conversation id, and rejects everything else. In this task the
    activation type stays `foreground`. TASK_036_EXTRA_6 switches it to `protocol` without
    touching the rest.
  - `Tag` is the target key (the conversation id, or the download id), and `Group` is the kind
    (`approval`, `turn`, `download`). A new toast with the same tag and group replaces the old
    one in the Action Center, so the 5 s coalescing in `NotificationService` is backed by the
    OS as well.
- **Activation.** Subscribe to `Activated` on each `ToastNotification`. The handler parses the
  `launch` argument with `from_url` and calls `NotificationService::activated(target)` on the
  async runtime. Keep every shown `ToastNotification` in a map keyed by tag and group, and
  remove it on `Activated`, `Dismissed` or `Failed`, so its handlers stay alive. Log `Failed`
  with its error code; never show it to the user.
- **Removing stale toasts.** The port gains `clear(target)`, which calls
  `ToastNotificationHistory::RemoveGroupedTagWithId` for every group of that tag. The
  renderer calls a new `notifications.clear` channel when a conversation is opened, and when
  «Загрузки» is opened for downloads. This way the Action Center does not keep «Ответ готов»
  for a chat the user has already read.
- Errors from the WinRT calls never reach the user. A toast that cannot be shown is logged at
  `Warn`, and the event goes on as if notifications were off.

*Shell: tray and autostart*
- `tauri-plugin-autostart`, plus the `tray-icon` feature on `tauri`. Resolve the latest stable
  versions and pin them exactly. Call the plugins from Rust only, so no new capability
  permissions are needed.
- **Tray.** When `app.tray` is on, there is a tray icon with the app icon and the tooltip
  «ZVS AI Studio». Left click shows and focuses the window (`window::focus_studio`). The menu
  has «Открыть» and «Выход». Closing the studio window hides it instead of destroying it. Only
  «Выход» (or the OS ending the session) goes through `Lifecycle::shutdown`, so a hidden window
  still runs its streams and downloads. When `app.tray` is off, the tray icon is removed and
  closing the window quits, as it does today. The browser gateway is disposed only on a real
  exit, not on hide.
- **Autostart.** The registration launches the app with `--autostart`. On such a launch with
  `app.tray` on, the window is created hidden, in the tray. Without the tray, the window opens
  normally. The single-instance handler still focuses the window.

*Renderer*
- **Activation.** A `NotificationRouter` in `RootStore` subscribes to `notifications.watch`
  and handles each target. A `conversation` target navigates to the chat route and selects
  that conversation through `ChatStore`. If the conversation has a pending approval, the
  approval card is scrolled into view. A conversation that no longer exists falls back to the
  chat route with no selection. A `downloads` target navigates to «Загрузки». The
  preferences dialog closes if it is open.
- `PreferenceSection` gains `general` and `notifications`. The nav order in the «Приложение»
  group is «Общие», «Внешний вид», «Уведомления», «О системе». Icons: `mdiCogOutline`,
  `mdiBellOutline`.
- `PreferencesStore` (or an extension of `UiStore`, whichever fits next to `ui.font.*`) loads the
  six keys, writes them through `settings.set` and rolls back the toggle when the call fails,
  showing the error under the row.
- «Общие» has two `SettingRow`s with toggles, in this order:
  - «Запускать вместе с системой». Hint: «Открывать приложение при входе в Windows.»
  - «Значок в области уведомлений». Hint: «При закрытии окна приложение продолжает работать
    в области уведомлений.»
- «Уведомления» has a master row, «Уведомления», with the hint «Системные уведомления о
  событиях приложения.» While
  the master toggle is on, three rows follow, indented under it:
  - «Требуется подтверждение»: «Инструмент ожидает разрешения на выполнение.»
  - «Ответ готов»: «Модель завершила ответ в диалоге.»
  - «Загрузка завершена»: «Модель или компонент загружены.»
  The sub-keys keep their values while hidden, so turning the master toggle back on restores
  them.
- `SECTION_LABELS` / `SECTION_HINTS`: «Общие» with «Запуск и работа в фоне.», and
  «Уведомления» with «Системные уведомления о событиях приложения.»

*Copy*
- Dialog strings go in `preferencesPresentation.ts`, and notification titles and bodies go in
  the host (`notifications/copy.rs`). All of it is Russian, per `CONVENTIONS.md`. Error message
  for a failed autostart change: «Не удалось изменить автозапуск.»

## ✅ DECISION — resolved with the user (2026-10-23)

> **Asked:** "Should a notification also appear while the app window is in front, or only when
> the window is hidden, minimised or in the background?"
>
> **Answer: always (option b), as in Electron.** Every enabled event raises a native toast,
> whatever window is in front, including the studio's own. The settings are the only filter.
> **Clicking a toast opens its target:** the conversation for «Ответ готов» and «Требуется
> подтверждение», and «Загрузки» for a download.
>
> **Implementation:** the toasts are built on `Windows.UI.Notifications` directly through the
> `windows` crate, with our own AUMID registration. Every toast already carries its target as
> a `zvs-studio://` URL in `launch`, along with a tag and a group. This takes the most work of
> the three options, but it is the setup that holds up in production: name and icon are
> correct in every build, toasts replace each other and are cleared natively, and Action
> Center clicks (TASK_036_EXTRA_6) only switch the activation type.

## Out of scope (do not do these, even if tempting)

- Clicks from the Action Center after the popup has closed, and clicks after the app has quit.
  For an app that is not packaged, both need the `zvs-studio://` protocol activation in
  TASK_036_EXTRA_6. Here, clicks work on the popup while the app runs, whether it is open or
  in the tray.
- Buttons on toasts (for example «Разрешить» / «Отклонить» on an approval). The click only
  opens the conversation, and the decision is made in the app.
- In-app toasts, a notification history or centre, sounds, do-not-disturb hours.
- A taskbar badge or overlay count.
- Notifications for other events (indexing done, MCP server down, scenario finished).
- Start-minimised as its own setting. It is implied by autostart combined with the tray.
- macOS and Linux specifics beyond compiling. Windows is the verified target.

## Steps

1. `ShellIntegration` port, `FakeShell`, and wiring into `Services`.
2. `NotificationService` with the policy, coalescing and copy. Unit tests first, on `FakeShell`
   and `FakeClock`.
3. Call sites: kernel approval, chat turn end, download end.
4. `settings.set` and startup application for `app.*`, with autostart reconciliation.
5. Shell: `WindowsToaster` (AUMID registration, XML, activation, history removal), the
   plugins, tray icon and menu, hide-on-close while the tray is on, `--autostart`
   launch, the port implementation.
6. Renderer: the sections, the store, the toggles, copy, tests.
7. PASSPORT §15: a paragraph on notifications (host-raised, policy in one service, no content
   in toasts) and on the tray lifecycle (hide versus exit).

## Verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm format:check
```

Tests that must exist:
- `NotificationService`: the master toggle off suppresses every kind. Each per-kind key
  suppresses only its own kind. A toast is raised whatever window is in front. Each draft
  carries the right target. `activated` shows the window and publishes the target on the
  watch stream. A cancelled turn
  raises nothing. A failed turn and a failed download use their failure titles. A burst within
  5 s collapses to one. No body contains message text or a path.
- `settings.set`: `app.tray` and `app.autostart` reach the port. A failing `set_autostart`
  returns the error and leaves the stored value unchanged. At startup, an autostart value that
  differs from the OS is overwritten.
- `NotificationTarget::to_url` / `from_url`: round trip for each target. Unknown hosts,
  extra path segments, query strings and malformed ids are rejected.
- Toast XML: titles and bodies with `<`, `&` and quotes are escaped. `launch`, `Tag` and
  `Group` are set. This is a pure builder, so test it without WinRT.
- `notifications.clear` reaches the port with the right target.
- `NotificationRouter`: a conversation target selects that conversation on the chat route.
  A missing conversation falls back to no selection. A downloads target opens «Загрузки».
  The preferences dialog closes.
- Renderer: the section order. The sub-toggles are hidden while the master toggle is off and
  keep their values. A failed write rolls the toggle back and shows the error.

Manual check, on the installed build: turn on the tray, close the window, and check that the
app stays in the tray and «Открыть» restores it. Start a long answer, switch to another app,
and check that a toast with the app's name appears. Do it again with the studio window in
front and check that the toast still appears. Click a «Ответ готов» toast while the window
is hidden in the tray, and check that the app opens on that conversation. Click an approval
toast and check that the approval card is in view. Check that a dev build shows «ZVS AI
Studio» and its icon on the toast, not «PowerShell». Finish two turns in one chat quickly and
check that the Action Center holds one toast for it. Open that chat and check that its toast
leaves the Action Center. Turn on autostart, sign out and back in,
and check that the app starts in the tray. Turn autostart off in Task Manager, reopen
«Общие», and check that the toggle shows off.

## Definition of done

- [x] «Общие» and «Уведомления» in the «Приложение» group, toggles in the stated order
- [x] Sub-toggles shown only while notifications are on
- [x] OS toasts for approvals, finished turns and finished downloads, gated by the settings,
      shown whatever window is in front
- [x] Clicking a toast opens its conversation, or «Загрузки» for a download
- [x] Toasts on `Windows.UI.Notifications` directly: AUMID registered, `zvs-studio://` launch
      URL, tag and group, replaced and cleared natively
- [x] `NotificationTargetDto` and channels in Rust, Zod and fixtures
- [x] Tray with hide-on-close, «Открыть» / «Выход», shutdown only on exit
- [x] Autostart registered through the OS, reconciled at startup, launched to tray with the tray on
- [x] Plugins pinned exactly; PASSPORT §15 updated
- [x] Gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Contract: `zvs_contract::notifications` holds `NotificationTargetDto` (`conversation` |
  `downloads`), `to_url` / `from_url` (strict, capped at 256 characters, round-trip tested), and
  `NAVIGATION_STEP_DOMAIN`. There is a Zod mirror in `packages/shared/src/notifications.ts`,
  fixtures in `fixtures/notifications/`, and the `notifications.clear` channel.
- Host: the `platform::shell` port (`ShellPort`, `NotificationDraft`, `NotificationKind`, the
  `app.*` / `notifications.*` keys) and `notifications::NotificationService`. The service holds
  the policy, coalesces within 5 s into a silent replacement, and handles `activated`, `clear`,
  `apply` and `start`. Copy is in `notifications/copy.rs`. `RunService::watch` is a global run
  watcher, and the service plugs into it, so the three events have no call sites of their own.
  `settings.set` applies `app.tray` / `app.autostart` through the port before storing. A refused
  autostart returns `NATIVE_ERROR` «Не удалось изменить автозапуск.» and keeps the stored value.
  Startup applies the tray and adopts the OS autostart state. `RecordingShell` is in `testing`.
- Shell: `WindowsToaster` (`platform/windows/toast.rs`) uses `Windows.UI.Notifications` on a
  worker thread. It registers the AUMID under `HKCU\Software\Classes\AppUserModelId\com.zvs.aistudio`
  with `DisplayName` and an `IconUri` that points to `<data>/shell/notification.png`, which
  is written from the bundled icon, and then calls `SetCurrentProcessExplicitAppUserModelID`.
  The XML is built in `toast.rs` (pure and tested) with a `launch` URL, a tag, a group,
  `SuppressPopup` for coalesced toasts, and `Activated` / `Dismissed` / `Failed` handlers that
  keep each live toast in a map. `clear` uses `RemoveGroupedTagWithId` / `RemoveGroupWithId`.
  `TauriShell` (`shell.rs`) provides the tray (`Открыть`, `Выход`, left click opens),
  `tauri-plugin-autostart` pinned `=2.7.0` with `--autostart`, and the `tray-icon` feature on
  `tauri`. While the tray is on, close hides the window. «Выход» closes the window, so its
  geometry is stored, and exits through the shutdown coordinator. A launch by autostart with
  the tray on stays hidden.
- Renderer: the sections «Общие», «Внешний вид», «Уведомления», «О системе». `AppSettingsStore`
  writes each toggle and rolls it back with an error under the row when the write fails. The
  notification kinds are shown only while the master toggle is on and keep their values.
  `AppFrame` handles the `navigation` step for `/chat` + `conversationId` and for
  `/downloads`, and closes the dialog. `ChatStore.open` defers until the page has mounted and
  bumps `revealed`, which scrolls the thread to the pending approval card. Selecting a
  conversation clears its toasts, and opening «Загрузки» clears the download toasts.
- PASSPORT §15 has «Notifications» and «Tray and autostart».

**Deviated from plan:**
- Activation reuses the existing `navigation` step, as account linking does, instead of new
  `notifications.watch` / `unwatch` channels. The event bus already reaches the renderer, so
  the only new channel is `notifications.clear`.
- One global run watcher replaces three call sites. Approvals raise a toast only for chat runs:
  tool probes and skill runs are started interactively from their own pages.
- Other platforms get no toaster (`NoToaster`) rather than `tauri-plugin-notification`. They
  cannot be verified here, and that dependency would exist only for them.
- The bundle is inactive (`bundle.active: false`, TASK_056), so there is no installer
  shortcut or uninstaller yet. The AUMID is registered at runtime, which also covers dev
  builds. TASK_056 must remove the `AppUserModelId` key and the autostart `Run` value on
  uninstall, and give the Start Menu shortcut the same AUMID.
- The preferences dialog still opens on «Внешний вид» by default.

**Next session should know:**
- Gates run: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` (clean),
  `cargo test --workspace -j 2` (1362 passed), `pnpm lint`, `pnpm test` (1724 passed),
  `pnpm format:check`. `pnpm typecheck` fails only on `tests/renderer/fileChanges.test.ts:59`,
  which also fails on HEAD.
- On this machine, `cargo test --workspace` at the default parallelism fails with
  `LNK1102` (the linker runs out of memory on the lance test binaries). Use `-j 2`.
- Not run: `pnpm build`, `pnpm test:e2e` and the manual check (toasts, clicking from the
  popup, the tray, autostart through a sign-out). They need a running app.
- Clicks from the Action Center after the popup closes, and clicks after quitting, belong to
  TASK_036_EXTRA_6. The `launch` URL is already in place for them.
