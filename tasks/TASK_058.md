---
task: TASK_058
title: Tauri shell beside Electron — app crate, Vite entries, renderer bridge, capabilities
phase: M — Migration to Tauri
status: done
depends_on: [TASK_034_EXTRA_1]
unblocks: [TASK_059]
estimate: 4 h
---

# TASK_058 — Tauri shell beside Electron — app crate, Vite entries, renderer bridge, capabilities

## Context

**Migration ground rules (TASK_058 – TASK_076).** On 2026-10-10 we decided to move the app off
Electron and onto Tauri 2, with the whole host rewritten in Rust (PASSPORT D10). Until TASK_076
both shells live in the tree. `pnpm dev` stays the Electron app with the full TypeScript host.
`pnpm dev:tauri` is the Tauri app: its Rust host answers the channels ported so far and returns
`NOT_IMPLEMENTED` for the rest. The TypeScript host under `apps/studio/src/host/` is **frozen**:
no features go in, only fixes that block the migration. A port reproduces the existing
behaviour exactly. The TypeScript implementation and its tests are the specification, and each
task ports the tests together with the code. The renderer does not change beyond the bridge.
There is no data migration: the Tauri build starts from an empty data directory and ignores
Electron's `%APPDATA%\ZVS AI Studio`.

Today `apps/studio` is built by electron-vite into three bundles. `out/host/main.js` is the
Electron main process, `out/preload/*.cjs` are two `contextBridge` preloads, and `out/renderer`
holds the React renderer and the browser chrome (`src/browser-ui`). The studio renderer reaches
the host only through `window.zvs`, which has two functions: `call(channel, payload)` and
`subscribe(handler)`. They are defined in `src/preload/index.ts`. The browser chrome has its
own, narrower `window.zvs` (`src/preload/browser.ts`).

This task adds the second shell without touching the first. It creates a Tauri 2 app crate at
`apps/studio/src-tauri` (crate `zvs-studio`, a cargo workspace member) and a plain Vite build
for the two web entries. It also adds a Tauri implementation of the same two-function bridge.
Every channel answers `NOT_IMPLEMENTED` except `system.ping`, which this task answers inline
to prove the round trip. TASK_060 replaces that with the real Rust IPC server.

## Why

Every later port task needs somewhere to land and a way to be exercised from the real
renderer. Getting the shell, the bridge and the security surface right once is what lets the
next eighteen tasks be pure porting.

## Depends on

- `TASK_034_EXTRA_1` — the last task landed on the Electron host. The migration starts from
  that state.

## Read first

- `/docs/PASSPORT.md` §2 D10, §5 (target layout and the TS→Rust path map), §7 (bridge), §14
- `apps/studio/src/preload/index.ts`, `src/preload/browser.ts`: the exact bridge surfaces to
  reproduce
- `apps/studio/src/renderer/main.tsx`, `src/renderer/ipc.ts`, `src/renderer/app/AppFrame.tsx`,
  `src/renderer/pages/BrowserPage.tsx`, `src/browser-ui/main.tsx`: every `window.zvs` consumer
- `apps/studio/electron.vite.config.ts`: the renderer and browser-ui build options to carry over
- `apps/studio/src/host/platform/windows.ts`: the CSP, navigation blocking and window options
  to reproduce

## Scope

**In scope**
- `apps/studio/src-tauri/` with `Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/`,
  `icons/`, and `src/main.rs` plus `src/lib.rs`. Add it to the cargo workspace.
- `apps/studio/vite.config.ts`: a plain Vite multi-page build of `src/index.html` and
  `src/browser-ui/index.html` into `apps/studio/dist/`, with Tailwind and the same aliases as
  the electron-vite renderer config
- Studio window `studio`: size limits, background colour and title from `windows.ts`. The
  window opens hidden and shows on first paint. Navigation away from the app origin and all new
  windows are denied.
- CSP in `tauri.conf.json` (`app.security.csp` and `devCsp`), equivalent to
  `installContentSecurityPolicy`, with no `unsafe-inline` and no `unsafe-eval`
- Capabilities: `studio.json` grants the `studio` webview only the custom commands `ipc_call`
  and `events_subscribe`. No core plugin permissions beyond what the bridge needs.
- Tauri's isolation pattern (`app.security.pattern: isolation`), with a minimal isolation
  script that passes only the two bridge commands and drops anything else. This is defence in
  depth on top of the capabilities, and the nearest equivalent to Electron's
  `contextIsolation`.
- Commands: `ipc_call(channel, payload) -> ResultEnvelope` and
  `events_subscribe(channel: Channel<serde_json::Value>)`. Both reject callers whose webview
  label is not `studio`.
- `src/renderer/app/tauriBridge.ts`: implements `{ call, subscribe }` over
  `@tauri-apps/api/core` `invoke` and a `Channel`, and installs it as a frozen `window.zvs`
  when `window.__TAURI_INTERNALS__` is present. Under Electron it does nothing, so
  `src/renderer/ipc.ts` and every consumer stay unchanged.
- `tauri-plugin-single-instance`: a second launch focuses the existing window, as
  `app.requestSingleInstanceLock` does today
- Root scripts `dev:tauri` and `build:tauri` (`tauri build --no-bundle`), and the `@tauri-apps/cli`
  and `@tauri-apps/api` dev dependencies, pinned exactly
- Lint policy for the new crate: `unsafe_code = "deny"` at crate level. Any later Win32/COM
  module opts in with `#[allow(unsafe_code)]` and a `// SAFETY:` comment on every block.
  `zvs-core` keeps `forbid`.
- CI: the Linux `rust` job runs `--workspace --exclude zvs-studio`, so it needs no webkit2gtk.
  A new Windows job runs `cargo clippy -p zvs-studio --all-targets -- -D warnings` and
  `pnpm build:tauri`.

**Out of scope** (do not do these, even if tempting)
- The Rust IPC server, event bus, logger and paths (TASK_060)
- Any real channel other than the inline `system.ping`
- The browser chrome webview and site views (TASK_065). `/browser` may render empty under Tauri.
- Bundling and installers (TASK_056)
- Removing anything Electron (TASK_076)

## Steps

1. Resolve and pin the latest stable `tauri`, `tauri-build`, `@tauri-apps/cli`,
   `@tauri-apps/api` and `tauri-plugin-single-instance`. Record the versions in the handoff.
   Enable `tauri`'s `unstable` feature now, because TASK_065 needs multi-webview, and say why
   in a comment.
2. Scaffold `src-tauri` by hand, not with `create-tauri-app`, so that nothing unexplained
   lands. `frontendDist` is `../dist`. `devUrl` is the Vite dev server. `beforeDevCommand`
   and `beforeBuildCommand` run the Vite scripts.
3. Write `vite.config.ts` from the renderer and browser-ui parts of `electron.vite.config.ts`.
   `pnpm --filter studio build:web` must produce `dist/index.html` and
   `dist/browser-ui/index.html`.
4. Commands. `ipc_call` takes `tauri::Webview`, checks `webview.label() == "studio"` and
   answers `{ ok: false, error: { code: "NOT_IMPLEMENTED", message } }` for any channel except
   `system.ping`. Add `NOT_IMPLEMENTED` to `AppErrorCode` in `packages/shared` with a Russian
   message, and handle it in the renderer's error copy map. `events_subscribe` stores the
   channel and drops a previous one.
5. `tauriBridge.ts`. `call` is `invoke("ipc_call", { channel, payload })`. `subscribe` creates
   a `Channel`, invokes `events_subscribe` once, fans incoming messages out to handlers and
   returns an unsubscribe. Import it first in `src/renderer/main.tsx`, guarded so the Electron
   build tree-shakes nothing it needs.
6. Window and CSP. Reproduce the navigation guard: `on_navigation` allows only the app origin,
   and `on_new_window` denies everything. Reproduce the background colour per OS theme.
7. Single instance: focus, restore and show the existing `studio` window.
8. CI and scripts as listed in scope. `pnpm build` (Electron) must be unaffected.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Which bundle identifier should the Tauri app use? It also fixes where the app keeps its
> data: `%APPDATA%\<identifier>`, plus the WebView2 profile next to it."
>
> **Why it matters:** changing it later moves the data directory, which means a data migration
> or a lost workspace once there are real users.
> **Options:** (a) `com.zvs.aistudio`; (b) a domain you own; (c) something else you name.
> **If the user is unavailable:** take (a) and note it in Handoff.

## Verify

```bash
pnpm install --frozen-lockfile
pnpm --filter studio build:web
pnpm build:tauri
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: `pnpm dev:tauri` opens the studio. The nav rail renders and every route opens.
In DevTools, `await window.zvs.call("system.ping", undefined)` resolves. Any other channel
rejects with `NOT_IMPLEMENTED`, and pages show their error state instead of crashing.
`pnpm dev` still opens the Electron app with everything working. A second `dev:tauri` launch
focuses the first window.

## Definition of done

- [x] `pnpm dev:tauri` boots the real renderer in a Tauri window
- [x] The bridge reproduces `call`/`subscribe` exactly, and the renderer code is unchanged
      apart from the bridge import
- [x] Only `ipc_call` and `events_subscribe` are reachable, and only from the `studio` webview
- [x] CSP has no `unsafe-inline` or `unsafe-eval`, and navigation and new windows are denied
- [x] The Electron build and all existing tests are untouched and green
- [x] CI checks `zvs-studio` on Windows and the rest of the workspace on Linux
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `apps/studio/src-tauri` (crate `zvs-studio`, a workspace member, lib `zvs_studio_lib`),
scaffolded by hand.
- `commands.rs` holds `ipc_call` and `events_subscribe`. Both are label-checked and async, so they stay
  off the main thread. `ipc_call` answers `system.ping` inline and `NOT_IMPLEMENTED` for everything
  else. The answer uses the TS server's validation message for a bad ping payload.
- `window.rs` holds the studio window. It is built from config (`create: false`) so that
  `on_navigation` allows only the app origin and `on_new_window` denies everything. The window shows
  on the first `PageLoadEvent::Finished`, and the canvas colour follows the OS theme. Single instance
  unminimizes, shows and focuses it.
- The isolation pattern lives in `src-tauri/isolation/`. Its hook passes `ipc_call`,
  `events_subscribe` and `plugin:__TAURI_CHANNEL__|fetch`. It throws on anything else, so the call
  is dropped. The channel fetch is how Tauri delivers channel messages over 8 KiB, and Tauri exempts
  it from the ACL.
- The app ACL manifest in `build.rs` declares only the two commands. `capabilities/studio.json`
  grants them to the `studio` **webview**, not the window, so TASK_065's child webviews inherit
  nothing. There are no core plugin permissions.
- `apps/studio/vite.config.ts` builds both entries into `apps/studio/dist`.
  `src/renderer/app/tauriBridge.ts` is the first import in `main.tsx`.
- `NOT_IMPLEMENTED` was appended to `AppErrorCode`, and the four renderer copy maps have Russian copy for it.
- Scripts: root `dev:tauri` and `build:tauri`; studio `dev:web`, `build:web`, `dev:tauri`, `build:tauri`.
- CI: the Linux `rust` job runs `--workspace --exclude zvs-studio`. The new Windows `tauri` job runs
  build:web, clippy, `cargo test -p zvs-studio` and `pnpm build:tauri`.
- Bundle identifier: `com.zvs.aistudio` (confirmed by the user).

Pinned: `tauri` 2.12.1 (features `isolation`, `unstable`), `tauri-build` 2.7.1,
`tauri-plugin-single-instance` 2.5.2, `@tauri-apps/cli` 2.12.1, `@tauri-apps/api` 2.12.1. Newer
patches (tauri 2.12.3, cli 2.12.2, api 2.12.3) shipped on 2026-10-10, and pnpm's minimum-release-age
guard refused them. I chose not to add them to `minimumReleaseAgeExclude`. Bump them once they age in.

**Deviated from plan:**
- No source comments, at the user's request. So the reason for `unstable` lives here instead of
  `Cargo.toml`: TASK_065 needs multi-webview, which in Tauri 2 sits behind `unstable`.
- The production `csp` has no `default-src`. Under the isolation pattern, Tauri appends the per-build
  `http://isolation-<uuid>.localhost` origin to `default-src`. Next to `'none'`, that made WebView2
  ignore `'none'` and log an error. With the key absent, Tauri creates `default-src <isolation origin>`,
  which is the effective policy either way. Every other fetch directive (media, worker and manifest
  included) is spelled out. `style-src` carries the hash of Tauri's isolation-iframe style, because
  Tauri hashes it only for the isolation document, not for `index.html`.
- Dev CSP: on desktop, Tauri loads `devUrl` directly and never applies `devCsp` as a header. The
  Vite plugin therefore writes `devCsp` into the HTML meta tag, with a random per-run
  `'nonce-…'` added to `style-src` and passed to Vite's `html.cspNonce`. HMR styles work without
  `'unsafe-inline'`. The build strips the meta tag, so the Tauri header (nonces plus isolation origin)
  is the only policy. `devCsp` uses `frame-src http://*.localhost` because the isolation origin
  carries a random uuid.
- `assetsInlineLimit: 0`: Vite inlined a small font subset as a `data:` URI, which `font-src 'self'`
  blocks. The Electron build has the same latent violation. I left it alone because Electron is frozen.
- The dev server is pinned to `localhost:5183` (strict port), so `devCsp`'s `ws:` source can be static.
- The icons are a placeholder ZVS mark generated with `tauri icon`. Replace them in TASK_056.

**Next session should know:**
- `generate_context!` needs `apps/studio/dist` to exist. On a clean checkout, run
  `pnpm --filter "studio^..." build && pnpm --filter studio build:web` before
  `cargo clippy -p zvs-studio`. CI does this.
- The `EventChannel` state is stored but nothing sends on it yet. TASK_060's `EventSink` should own it.
- Verified by hand over WebView2 CDP in both the release exe and `dev:tauri`:
  - `window.zvs` is frozen, and `system.ping` resolves.
  - `providers.list` returns `NOT_IMPLEMENTED`, and pages show the new copy in their error state.
  - `plugin:window|close` and unknown commands are dropped by the isolation hook.
  - `window.open` returns `null`, and external navigation is refused.
  - All 15 nav-rail routes render with no exceptions. `/browser` is empty.
  - A second launch exits and leaves the first window.
  - No CSP violations on load.
- Not run locally: `cargo clippy --workspace`, because this machine has no `protoc`.
  `-p zvs-studio` clippy and its 7 tests are green.
- Already failing on the untouched tree, so not caused by this task: `pnpm format:check` (14
  pre-existing files) and three Electron e2e specs (chat, smoke "secret channels", vector stores).
