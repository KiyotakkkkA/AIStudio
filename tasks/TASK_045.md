---
task: TASK_045
title: Attached browser workspace, WebContentsView, session partition
phase: 8 — Documents and the agent browser
status: done
depends_on: []
unblocks: [TASK_046]
estimate: 4 h
---

# TASK_045 — Browser window shell, WebContentsView, session partition

> Scope update (2026-09-09): the user explicitly requested a VS Code-like browser attached
> to the main studio window, styled from `design/Foundations.dc.html`, plus site cookie
> management. This supersedes the separate-window requirement below. The shipped layout
> uses `/browser` and `BrowserViewManager`; browser isolation and the automation exclusions
> still apply. Browser tabs remain alive when switching studio routes.

## Context

This task opens the riskiest surface in the product, so it is built in three deliberate stages: the window and its isolation here, the automation in TASK_046, and the agent panel in TASK_047. Nothing drives the browser until the isolation is right.

Per decision D9 and PASSPORT §21, the browser is **its own window**, not a route. The rail's `Browser` item opens it. The window's chrome is a small renderer (`browser-ui/`); the page area is a `WebContentsView` loading real sites. The site's view gets **its own preload that exposes nothing at all** — far narrower than the studio's two functions. The page must have no bridge, no IPC, and no knowledge it is being automated beyond what any browser reveals.

The session uses a persistent partition, `persist:browser-work`, so the user logs into sites once inside the app and those cookies survive restarts. That is what makes the automations useful — and it is also what makes prompt injection dangerous later, which is why TASK_046 and TASK_047 exist as separate, careful tasks.

## Why

Every browser capability depends on this window existing with the right isolation. Getting the partition and preload boundaries right before any automation is added is the whole point of the sequencing.

## Read first

- `/design/BrowserWindow.dc.html` — window chrome, tab strip, toolbar, agent panel, status bar
- `/docs/PASSPORT.md` §21 (the whole section), §14 (security)

## Scope

**In scope**
- A second `BrowserWindow` with its own renderer entry (`browser-ui/`)
- `WebContentsView` for page content with a null preload
- Persistent partition `persist:browser-work`, isolated from the studio session
- Tab management: open, close, switch, reorder
- Toolbar: back, forward, reload, address bar, security indicator
- Window lifecycle: open from the rail, focus if already open, state persistence
- No automation yet

**Out of scope** (do not do these, even if tempting)
- CDP and any agent control (TASK_046)
- The agent panel and approvals (TASK_047)
- Downloads from the browser — block them for now; TASK_046's policy handles them
- Extensions, bookmarks, history sync

## Steps

1. Add a fourth build entry for `browser-ui` in `electron.vite.config.ts`, alongside host, preload and renderer.
2. `host/browser/BrowserWindowManager`: create the window, focus an existing one, persist bounds, tear down cleanly. Wire the rail's `Browser` item to it.
3. Window composition: the chrome renderer fills the window; a `WebContentsView` is positioned in the page area and resized with the window. Keep the geometry in the host, not the renderer, so the view cannot be moved by page content.
4. **The site view's preload is a separate file that exposes nothing.** Add a comment stating that anything added here is reachable by arbitrary websites. `contextIsolation: true`, `nodeIntegration: false`, `sandbox: true`, and no `webSecurity` relaxation ever.
5. Session: `persist:browser-work`, separate from the studio's session so app credentials are never in reach. Permission handler for the session denies camera, microphone, geolocation and notifications by default.
6. Tab model in the host: each tab is a `WebContentsView`; the chrome renders the strip from host state. Only the active view is attached; inactive ones stay alive but detached.
7. Toolbar: navigation actions, an address bar showing the real URL of the active tab, and a security indicator reflecting the actual TLS state — never a decorative lock.
8. Block for now: downloads, new-window requests (convert to a tab), and `file:` navigation. TASK_046 revisits these under policy.
9. Tests: the window opens and focuses correctly; the partition is genuinely separate (a cookie set in the browser is not visible to the studio session); the site preload exposes nothing (assert `window.zvs` is undefined in a loaded page); permission requests are denied by default.

## Verify

```bash
pnpm test
pnpm exec playwright test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Open the browser from the rail, browse to a real site, log into something, close and reopen the app, and confirm the login persisted. In the page's DevTools, confirm no bridge object exists and no Node globals are reachable.

## Definition of done

- [x] Browser opens from the rail inside the main window (user-requested scope update)
- [x] Page content runs in a `WebContentsView` with a preload that exposes nothing — verified
- [x] Partition is persistent and provably separate from the studio session
- [x] Tabs, navigation and the address bar work; HTTPS, HTTP and TLS failures are distinguished
- [x] Downloads and unsafe navigation blocked; safe popups become tabs without new native windows
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Attached `/browser` workspace in the main studio window. A separate chrome
renderer and isolated site `WebContentsView` instances are composed by `BrowserViewManager`.
Tabs open/close/switch/reorder (drag or Alt+Left/Right); navigation, real URL, transport/error
status, rail resizing, route hide/show and cleanup work. Browser chrome uses the studio's
shared Foundations theme. Сайты & cookies provides Qwen/DeepSeek shortcuts, cookie metadata,
filtering, confirmed individual removal and exact-domain clearing; values never leave the host.
The persistent profile also retains normal site storage. Unit and Electron tests cover
isolation, permission query/request denial, downloads, TLS failures, tab lifecycle, route
switching, rail geometry, cookie deletion and full-process cookie persistence.

**Deviated from plan:** The user superseded the separate-window requirement with an attached,
VS Code-like layout. Main-window bounds and route persistence are reused. `browser-ui` is a
second HTML entry in electron-vite's renderer build (which supports three build targets),
with separate chrome and empty site preloads. Browser IPC has a dedicated validated command
schema and exact sender/frame allowlists; studio IPC rejects browser senders. Studio HTML
entry moved to `src/index.html` so both renderers share a Vite root and build output paths
remain stable. Tests run from the root because `studio` has no `test` script.

**Next session should know:** No provider session requests or automation/CDP were added.
Future provider integrations must access `persist:browser-work` through the host. Cookie
clearing affects the selected cookie domain only, not unrelated domains or local storage.
POST popup requests adopt Chromium's existing webContents rather than replaying their URL;
this preserves POST bodies, referrers and OAuth opener callbacks. A regression test verifies
POST/callback delivery, self-closing popups and isolation. `ERR_CACHE_MISS` now gives a
form-submission recovery message; a live Google account login was not verified. Restart a
failed login from the provider page. Main-process changes require a dev-app restart.
