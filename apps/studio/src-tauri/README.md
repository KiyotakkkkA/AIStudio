# zvs-studio (Tauri shell)

The desktop executable. It owns windows, webviews, the tray, OS integrations and the Tauri
command surface; all business logic is in `zvs-host`. The shell implements the host's ports
(theme, picker, crypto, GPU, audio, toasts, browser automation, CLI installer) and wires
everything together in one composition root, `lib.rs::compose`.

`zvs-studio` builds and is verified on Windows only (PASSPORT §18.3). On other platforms the
`platform/*_stub.rs` files keep the crate compiling; CI runs its clippy and tests on Windows.

## Layout

| Path            | Contents                                                                                       |
| --------------- | ---------------------------------------------------------------------------------------------- |
| `lib.rs`        | `run()`: plugins, commands, `compose`, boot, shutdown, deep links, sidecar warm-up             |
| `commands.rs`   | The Tauri commands and the per-window guard                                                    |
| `window.rs`     | The studio and recovery windows, geometry persistence, same-origin navigation checks           |
| `shell.rs`      | `TauriShell`: tray, autostart, hide-to-tray, the shell state shared with the host              |
| `cli.rs`        | The named-pipe listener for the `aistudio` CLI                                                 |
| `computer.rs`   | `ComputerShell`: the computer-use overlay windows (panel and glow) and their channels          |
| `picker.rs`     | `DialogPicker`: the host `PickerPort` over `tauri-plugin-dialog`                               |
| `theme.rs`      | `TauriTheme`: applies the host theme source to windows and the browser                         |
| `toast.rs`      | The `Toaster` port, toast XML, and parsing of `zvs://` launch targets                          |
| `e2e.rs`        | Overrides used only under the `e2e` feature: data dir, WebView2 args and folders               |
| `browser/`      | The built-in browser: workspace, CDP automation, network capture, account gateway              |
| `platform/`     | Windows implementations of host ports, with stubs for other targets                            |
| `isolation/`    | The Tauri isolation-pattern script that filters IPC from the renderer                          |
| `capabilities/` | Tauri capability files per window (`studio`, `browser-ui`, `computer-ui`)                      |
| `build.rs`      | Reads `../../../app.env` (`ZVS_APP_VERSION`, `ZVS_APP_CHANNEL` required) into compile-time env |

## Pipeline: boot (`run` → `compose`)

1. Clear inherited WebView2 environment variables before any thread starts (`e2e.rs`), then
   register plugins: single-instance (a second launch forwards its arguments, typically a
   `zvs://` link, to the running app), deep link, dialog, autostart.
2. `compose` resolves `StudioPaths` (data, resources, executable folder, packaged or dev),
   opens the `FileLogger`, then `prepare_database`: open SQLite and apply pending migrations,
   backing the file up first (`BACKUPS_KEPT` copies, see `crates/zvs-host/src/data/README.md`). A migration
   failure does not crash; it boots the **recovery window** instead
   (`Boot::Recovery`, `window::create_recovery`) with the failure in the URL.
3. With a database, `compose` builds the host: crypto backend (DPAPI), `Services`, the IPC
   server, the event bus, the kernel registry and node definitions, the sidecar driver
   (`zvs-jobd`), the browser workspace, the account gateway, the computer shell and the CLI
   listener. Once the registry is complete it recovers what the previous exit interrupted:
   runs (`services.runs.recover`, see `crates/zvs-host/src/kernel/README.md`), active
   `/goal` loops and downloads.
4. `setup` stores the managed state, creates the studio window with its saved geometry, and
   applies the stored interface zoom, theme and palettes before the first paint.
5. The sidecar and the account pages are warmed in the background so the first job and the
   first account request do not pay the start-up cost.

## Commands and their guard

| Command                                                      | Window allowed  |
| ------------------------------------------------------------ | --------------- |
| `ipc_call`, `events_subscribe`                               | `studio`        |
| `browser_command`, `browser_subscribe`                       | the browser UI  |
| `computer_command`, `computer_subscribe`, `computer_pointer` | the computer UI |

Every command first checks `webview.label()` against the window it belongs to, so a page
loaded in the built-in browser can never reach the host IPC. `ipc_call` handles two
shell-only channels itself (`browser.mount`, `browser.hide`, which need window bounds) and
passes everything else to `IpcServer::dispatch`, which validates input and returns the
`{ ok, data | error }` envelope (PASSPORT §7). Events go to the renderer over a Tauri
`Channel`.

## Shutdown

`RunEvent::ExitRequested` asks `ShutdownCoordinator::request_exit`. The first request returns
`Begin`: exit is prevented, the coordinator suspends runs, drains kernel work and closes
services and SQLite, then the app exits. Requests while that runs return `Wait`. `RunEvent::Exit`
disposes the event bus. Closing the studio window disposes the account gateway.

## Tests

`cargo test -p zvs-studio` (Windows). `tests/account_gateway.rs` needs `--features e2e` and a
real WebView2. The Playwright suite in `e2e/` drives a build made with `--features e2e`.
