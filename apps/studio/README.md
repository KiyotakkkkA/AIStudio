# Studio app

The Tauri app: the web front end in `src/` and the shell in `src-tauri/` (`zvs-studio`). The
host behind it is `crates/zvs-host`. Architecture and the decisions behind it live in
`/docs/PASSPORT.md`; this file records what the runtime pieces do.

| Path                     | Contents                                                                   |
| ------------------------ | -------------------------------------------------------------------------- |
| `src/renderer`           | The studio window: React + MobX, the `window.zvs` bridge, the event router |
| `src/browser-ui`         | The browser workspace chrome (tabs, address bar, cookies)                  |
| `src/recovery`           | The page shown when a migration fails at boot                              |
| `src-tauri/src/lib.rs`   | The composition root: paths, logger, database, services, IPC, shutdown     |
| `src-tauri/src/browser`  | The browser workspace, the site webviews and the account gateway           |
| `src-tauri/capabilities` | What each webview may call: `studio` and `browser-ui`                      |

| Script      | What it does                                                                 |
| ----------- | ---------------------------------------------------------------------------- |
| `dev`       | `scripts/tauri.mjs dev`: builds `zvs-jobd`, then `tauri dev` over `dev:web`  |
| `build`     | `scripts/tauri.mjs build --no-bundle`: `zvs-jobd`, `build:web`, `zvs-studio` |
| `package`   | `scripts/tauri.mjs build`, for the installer (TASK_056)                      |
| `dev:web`   | Vite alone on `localhost:5183`; Tauri starts it as `beforeDevCommand`        |
| `build:web` | Vite into `dist/` (studio, browser-ui and recovery pages)                    |

`scripts/tauri.mjs` points `PROTOC` at `target/tools/protoc` when it exists, builds the
`zvs-jobd` sidecar next to `zvs-studio.exe` (where `StudioPaths::sidecar_path` looks for it),
and drops a literal `--`, so `pnpm build -- --features e2e` works. With the `e2e` feature it
also sets `ZVS_E2E_BUILD` and builds `zvs-core`'s `seed_vectors` example for the e2e suite.

## The bridge

`src/renderer/app/tauriBridge.ts` defines `window.zvs` with exactly two functions:
`call(channel, payload)`, which invokes the `ipc_call` command, and `subscribe(handler)`, which
opens one `Channel` through `events_subscribe` and returns its own unsubscribe. The browser
chrome has its own bridge over `browser_command` and `browser_subscribe`. Adding a command is a
security decision, not a convenience: it needs a capability entry too.

## The event channel

Request/response goes through `@zvs/ipc`. Everything that streams — tokens, progress, step
events, log lines, approval requests — goes through the one events channel instead (PASSPORT
§7, D4), typed by `HostEvent` in `@zvs/shared`.

The host end is `zvs_host::platform::events::EventBus`: `open_stream()` hands back a
`StreamHandle` whose emits are stamped with `seq` and `ts` and whose `end(outcome)` sends the
terminal `end` event. `seq` starts at 0 and is per stream, never global. Emitting on an ended
stream is logged at `error` and dropped, never a panic. With no window attached the event is
logged at `debug` and dropped.

`src/renderer/app/EventRouter.ts` is the renderer end. `createEventRouter()` validates each
incoming payload against `HostEvent` and routes it by `streamId` to whatever subscribed;
`connect(source)` attaches it to the bridge's `subscribe` and returns a detach function.
`subscribe(streamId, handler)` returns a dispose function — stores subscribe on start and
dispose on teardown (PASSPORT §13). The router tracks the last `seq` per stream: an event
arriving beyond `last + 1` logs a warning and carries `gap` (the number of missing events) so
a store can refetch instead of rendering a hole; a `seq` that goes backwards warns as well and
is still delivered. Events for a `streamId` nobody has subscribed to yet are held in a small
ring (256 by default) and replayed to the first subscriber, which is what makes it safe to
start a run and attach a tick later; older held events are dropped with a warning.

Backpressure is a known gap. The bus sends every event straight to the window with no
coalescing; revisit only if a real producer floods the UI.

## Recording and replay

Set `ZVS_RECORD_EVENTS=1` and the host appends every event as JSONL to
`<data>/logs/streams/<date>.jsonl`. `src/renderer/app/replay.ts` reads such a file back:
`replayRecording(router, text)` feeds it into an `EventRouter` with the host completely idle,
so a failing run can be reproduced in a fresh UI. In development the hook is on the window as
`zvsReplay.fromText(text)` / `zvsReplay.fromUrl(url)`.

## Smoke channels

`system.ping` is the request/response round trip. `system.demoStream` is the streaming one: it
opens a stream, emits ten `progress` events and ends. `e2e/smoke.spec.ts` calls both.
