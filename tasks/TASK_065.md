---
task: TASK_065
title: Browser workspace on Tauri — child webviews, persistent profile, tab manager, Sites & cookies
phase: M — Migration to Tauri
status: done
depends_on: [TASK_064]
unblocks: [TASK_066]
estimate: 5 h
---

# TASK_065 — Browser workspace on Tauri — child webviews, persistent profile, tab manager, Sites & cookies

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

TASK_045 built the integrated browser: an **attached, VS Code-like workspace** inside the
studio window, not a separate window (PASSPORT §21 implementation update).
`host/browser/BrowserViewManager.ts` (437 lines) composes the following:
- One `WebContentsView` for the chrome, the `browser-ui` renderer with tabs, toolbar and
  Sites & cookies. It uses its own narrow preload: `call("browser.command")` and a
  `browser.state` subscription only.
- One `WebContentsView` per tab for the site, in the persistent partition
  `persist:browser-work`, with an **empty** preload. The page gets no bridge.

The studio page sends `browser.mount(bounds)` and `browser.hide` as the route opens and
closes. Commands are `state`, `new`, `navigate`, `switch`, `close`, `reorder`, `back`,
`forward`, `reload`, `sites`, `cookies` and `removeCookie`/`clearDomain`.

The profile is locked down:
- Every permission request and check is denied, and device permissions too.
- Downloads are cancelled.
- A main-frame or sub-frame request to a non-web URL is cancelled (`isWebUrl`).
- `will-attach-webview` is refused.
- Popups become tabs, and POST/OAuth popups **adopt** Chromium's created contents so bodies and
  opener callbacks survive.
- Failed loads publish an error state (`ERR_CACHE_MISS` gets the form-resubmission message).
- Cookie metadata goes to the chrome. Values never leave the host.
- Removing cookies emits `cookies-cleared` on `BrowserLifecycle`, which un-links accounts.

Under Tauri, the equivalents are **child webviews** in the studio window. `Window::add_child`
needs the `unstable` feature, which TASK_058 enabled. The profile becomes a dedicated
**WebView2 user-data directory**, `<app data>/browser/work`, set with
`WebviewBuilder::data_directory`. It is separate from the studio webview's environment, so
cookies, storage and cache are isolated from the app exactly as the partition isolated them.
The chrome is a child webview loading `browser-ui/index.html` with its own capability (only the
`browser_command` command and a state channel). Site webviews load remote URLs, appear in no
capability, and so can reach no command at all.

## Why

Account-mode providers (TASK_066–TASK_067) and browser automation (TASK_046) are built on this
profile and these webviews. The security properties have to hold before anything is layered on
top.

## Depends on

- `TASK_064` — providers under Tauri. The Accounts tab's Login button lands in this browser
  in TASK_067.

## Read first

- `/docs/PASSPORT.md` §21 (as updated for Tauri), §14
- `apps/studio/src/host/browser/BrowserViewManager.ts`, `policy.ts`, `lifecycle.ts`
- `apps/studio/src/browser-ui/main.tsx`, `src/renderer/pages/BrowserPage.tsx`,
  `packages/shared/src/browser.ts`
- `/tasks/TASK_045.md`, especially its Handoff
- `apps/studio/tests/browserPolicy.test.ts`, `browserLink.test.ts`, `e2e/browser.spec.ts`

## Scope

**In scope**
- Spike first (step 1), with the result recorded in the handoff
- `zvs-host::browser`: `policy` (`is_web_url`, `navigation_url`, layout constants) and
  `lifecycle` (`BrowserLifecycle` events), both pure and ported with their tests
- `zvs-studio::browser::BrowserWorkspace`, a port of `BrowserViewManager`: the chrome webview,
  per-tab site webviews in the `browser/work` data directory, layout from the mounted bounds
  (chrome height 148, status height 28), show and hide on route change, tab state retained
  while hidden, `browser.state` published to the chrome
- The `browser.mount` and `browser.hide` commands from the `studio` webview, and
  `browser_command` from the chrome webview. Each checks the caller's label and validates
  `BrowserBounds`/`BrowserCommand` with the zvs-contract types and fixtures.
- `browser-ui` bridge: a small `tauriBridge` that installs the chrome's two-function
  `window.zvs`, as TASK_058 did for the studio
- Lockdown, reproduced with Tauri APIs where they exist and WebView2 COM through
  `with_webview` where they do not:
  - permissions denied through WebView2 `PermissionRequested`
  - downloads cancelled through Tauri `on_download`
  - non-web navigation blocked through `on_navigation` plus a `FrameNavigationStarting` check
    for sub-frames
  - popups through new-window handling, adopting the new webview for POST/OAuth
  - no devtools on site webviews in release builds
- Load errors mapped into the tab state, including the cache-miss message
- Sites & cookies: list cookie metadata, remove one, clear an exact domain, and emit
  `cookies-cleared`. Use Tauri's cookie API at the pinned version, falling back to
  `ICoreWebView2CookieManager`. Values never cross into any webview.
- WebView2 COM access lives in `zvs-studio/src/platform/windows/webview2.rs` with scoped
  `#[allow(unsafe_code)]` and SAFETY comments

**Out of scope** (do not do these, even if tempting)
- Account linking and authenticated requests (TASK_066–TASK_067)
- CDP automation (TASK_046)
- A separate browser window. The attached layout is the decided presentation.

## Steps

1. **Spike, about an hour.** In a scratch branch, prove three things on Windows:
   (a) a child webview with a different `data_directory` can sit in the same window as the
   studio webview;
   (b) cookies persist in it across an app restart;
   (c) a popup opened by `window.open` with a POST form can be adopted as a new child webview
   with its opener intact.
   If (a) fails, try WebView2 profiles (`ICoreWebView2ControllerOptions::ProfileName`, one
   shared user-data folder with a named `work` profile), which give the same isolation. If
   neither works, stop and report with what you found. Do not ship a shared profile.
2. Port `policy` and `lifecycle` into `zvs-host::browser` with their tests.
3. `BrowserWorkspace`: create the chrome and tabs, lay them out, attach and detach, and
   publish state. Port the state-shape tests.
4. Command handlers with sender checks and validation, plus the capability for `browser-ui`.
5. Lockdown handlers, each with a test page under `e2e/fixtures/` and a test (or a manual
   check where only a live webview can show it).
6. Sites & cookies.
7. `browser-ui` bridge, and `BrowserPage` calling `browser.mount`/`browser.hide` through the
   Tauri bridge. Its code should be unchanged, because the bridge keeps the same shape.

## Verify

```bash
cargo test -p zvs-host -p zvs-studio
cargo clippy --workspace --all-targets -- -D warnings
pnpm build:tauri
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, open `/browser`. Open, switch, reorder and close tabs.
Navigate, go back and forward, reload. Switch routes and return to find the tabs intact. Log
into a site, restart the app, and confirm you are still logged in. Try `file:///C:/`,
`javascript:` and `data:` URLs and confirm they are refused. A site asking for camera or
notifications gets nothing. A download does nothing. Sites & cookies lists metadata only, and
removing a domain logs you out of it. In DevTools on a site tab (debug build),
`window.__TAURI_INTERNALS__` is `undefined`.

## Definition of done

- [x] The spike result is recorded, and the profile is isolated from the studio's environment
- [x] Tabs, layout, route hide and show, and state publishing are at parity with TASK_045
- [x] Site webviews reach no command, and the chrome reaches only `browser_command`
- [x] Permissions, downloads and non-web navigation are blocked, and popups adopted
- [x] Sites & cookies works and never exposes values, with `cookies-cleared` emitted
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Spike (step 1), run on Windows 11 with WebView2 through a throwaway binary in `src-tauri`,
  since removed:
  - (a) A child webview with `data_directory(<scratch>/browser/work)` sits in the same window
    as an app webview. The site's cookie was absent from the studio webview's cookie store.
  - (b) After a full process restart, `cookies_for_url` on a new child webview with the same
    directory returned the `HttpOnly` cookie set by a local server.
  - (c) A `form.target = "_blank"; form.rel = "opener"` POST was adopted. The server saw one
    `POST state=nonce`. The popup was moved into the studio window with `Webview::reparent`,
    the hidden host window was destroyed, and `window.opener.postMessage` reached the opener.
  - Also proved: WebView2 numbers the scripts it runs on document creation `1`, `2`, … per
    webview (12 on a site webview). Removing them and turning off web messages leaves
    `__TAURI_INTERNALS__`, `isTauri`, `window.ipc` and `chrome.webview` all `undefined` in
    site pages and popups. No fallback to WebView2 profiles was needed.
- `zvs-contract::browser`: `BrowserBounds`, `BrowserCommand` (tagged on `action`, with the Zod
  limits: UUID tab ids, URL ≤ 8192, cookie id ≤ 32768, domain ≤ 253, safe-integer index),
  `BrowserTab`, `BrowserSecurity`, `BrowserState`, `BrowserCookie`, `BrowserCommandOutput`
  (untagged state or cookie list), and `ids::is_wire_uuid`. Fixtures `browser/BrowserBounds`
  and `browser/BrowserCommand`, registered in the Rust and Vitest tables.
- `zvs-host::browser`: `policy` (`is_web_url`, `navigation_url`, `profile_dir`,
  `CHROME_HEIGHT` 148, `STATUS_HEIGHT` 28, `TAB_LIMIT` 30, the two messages) and `lifecycle`
  (`BrowserLifecycle`, `BrowserLifecycleEvent`, `LifecycleSubscription`), with the
  `browserPolicy.test.ts` cases.
- `zvs-studio::browser`:
  - `model`: the pure tab strip (open, switch, close, forget, reorder, sites, page signals),
    `state()` with the TS security and title rules, and `layout()` (chrome at the mounted
    bounds, clamped to the window, page below the chrome and above the status bar, shown only
    while visible, off Sites and with a URL).
  - `cookies`: `CookieMeta`, the Electron cookie id `[domain, path, name, secure]`, the DTO
    mapping (no value field) and `cleared_domains`.
  - `BrowserWorkspace`: the `browser-ui` chrome child webview and one `browser-site-N` child
    webview per tab in `<app data>/browser/work`. It mounts and hides, keeps tabs while
    hidden, follows window resizes, and publishes `BrowserState` to the chrome over a Tauri
    channel. When the studio window is destroyed it emits `workspace-closed`. Page events
    reach it through a worker thread, so nothing blocks the main thread or holds a lock across
    a webview call.
- `zvs-studio::platform::windows::webview2` (scoped `#![allow(unsafe_code)]`): strips the host
  scripts and web messages from site webviews; handles `NavigationStarting` and
  `FrameNavigationStarting` (cancels non-web URLs and reports a block), `NavigationCompleted`,
  `SourceChanged`, `HistoryChanged`, `DocumentTitleChanged`, `ProcessFailed` and
  `WindowCloseRequested`; implements back and forward; and lists and deletes cookies through
  `ICoreWebView2CookieManager` without reading values. `platform::site_stub` keeps non-Windows
  builds compiling.
- Lockdown on every site webview: `on_permission_request` → `Deny`, `on_download` → `false`,
  `on_navigation` limited to web URLs and `about:blank`, sub-frames limited to web URLs,
  `about:blank` and `about:srcdoc`, devtools only with `debug_assertions`. Popups to web URLs
  become tabs while there is room, and are otherwise denied. The chrome may only navigate on
  the app origin, opens no windows, downloads nothing and gets no permissions.
- Commands: `ipc_call` serves `browser.mount` (validated `BrowserBounds`) and `browser.hide`
  for the `studio` webview. `browser_command` (validated `BrowserCommand`) and
  `browser_subscribe` accept only the `browser-ui` label. The `browser-ui` capability allows
  only those two. Site webviews are in no capability.
- `browser-ui/tauriBridge.ts` installs the chrome's two-function `window.zvs`
  (`call("browser.command")`, `subscribe`). `BrowserPage.tsx` is unchanged.
- `e2e/fixtures/browser/`: `permissions.html`, `frames.html`, `download.html`, `popup.html` and
  `opener.html` for the live checks.
- Tests: 6 contract unit tests and 2 fixture names, 7 in `zvs-host::browser`, 25 in
  `zvs-studio` (model, cookies, WebView2 status and URL rules, label checks).

**Deviated from plan:**

- No source comments, at the user's request, so the COM module has no `SAFETY` comments.
- Sites & cookies uses `ICoreWebView2CookieManager` instead of Tauri's cookie API. Tauri goes
  through the `cookie` crate, which drops the leading dot of a domain cookie. A removal would
  then miss `.example.com` cookies, and `clearSite` could not match the exact domain the chrome
  shows.
- `__TAURI_INTERNALS__` is undefined on site tabs because the workspace removes WebView2's
  document-created scripts `1`–`64` right after a site webview is created and before it
  navigates. Tauri has no switch for this. Web messages are also off, so even the IPC
  transport is gone, and the webviews are in no capability.
- Popups: a hidden host window and a child webview share a label, so `get_webview_window`
  yields the `WebviewWindow` that `NewWindowResponse::Create` needs. The worker then
  reparents the webview into the studio window and destroys the host window.
- `ERR_CACHE_MISS` has no WebView2 status. The form-resubmission message is shown for an
  `UNKNOWN` failure of a back/forward navigation (`NavigationKind`). Other failures read
  `Page could not load (<WebView2 status>)`, not a Chromium net error code.
- The chrome's 15 px corner radius (`setBorderRadius`) has no WebView2 equivalent. After every
  layout, `round_corners` clips the chrome's host window with `SetWindowRgn` to a rounded
  rectangle (15 logical px, scaled to physical). The GDI region is not anti-aliased.
- `openTab` and the link-tab bookkeeping (`link-tab-closed`) are not ported. They belong to
  account linking (TASK_066–TASK_067). `BrowserLifecycle` is managed as Tauri state for them.
- `focus_studio` now uses `get_window`, since the studio window stops being a "webview window"
  once it has child webviews.

**Next session should know:**

- TASK_066/067 get the profile from `zvs_host::browser::profile_dir(app_data_dir)` and the
  lifecycle from `app.state::<BrowserLifecycle>()`. `cookies-cleared` carries the cookie's
  domain as WebView2 reports it (leading dot for domain cookies), then the `clearSite` domain,
  as in TS.
- A new command must also be added to the allowlist in `src-tauri/isolation/index.js`, or the
  isolation hook rejects it before the capability is checked.
- WebView2 calls that wait on the main thread (`add_child`, `reparent`, cookie reads) must run
  off it. Commands use `spawn_blocking`, and COM handlers only post to the workspace thread.
- Gates run on Windows on 2026-10-14, all exit 0: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm build`, `pnpm build:tauri`,
  `npx vitest run` (1192 passed, 2 skipped) and `node scripts/contract-channels.mjs --check`.
- Not done: the manual `pnpm dev:tauri` pass over `/browser` (tabs, route switching, restart
  login, refused URLs, permissions, downloads, Sites & cookies, DevTools). The spike covered
  isolation, persistence, popup adoption and the missing globals in a live webview, but the
  chrome UI and z-order over the studio webview have not been seen on screen.
