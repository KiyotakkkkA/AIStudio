---
task: TASK_002
title: Electron shell — host, preload, renderer, window, platform services
phase: 0 — Foundation
status: done
depends_on: [TASK_001]
unblocks: [TASK_004, TASK_006, TASK_007]
estimate: 4 h
---

# TASK_002 — Electron shell: host, preload, renderer, window, platform services

## Context

TASK_001 left a pnpm + Turbo monorepo with lint boundaries and gates, but no application.
This task creates `apps/studio` — the Electron app itself — and gets a window on screen.

ZVS AI Studio is a local-first desktop studio for AI automations. The Electron main process is
called **host** throughout this project (never "main" in prose, to avoid confusion with entry
files), the React side is **renderer**, and a deliberately tiny **preload** is the only bridge
between them. Security posture is strict and non-negotiable: `contextIsolation: true`,
`nodeIntegration: false`, `sandbox: true`, and a preload that will eventually expose exactly
two functions (`call` and `subscribe`) and nothing else. In this task the preload exposes
nothing at all — TASK_004 adds the two functions.

This task also creates the small platform layer the whole host depends on: path resolution,
structured logging and the composition root. Every later service is constructed in the
composition root and receives its dependencies explicitly, so services can be unit-tested with
no Electron present.

## Why

Nothing can be built or tested until there is an app that starts. The platform layer is here
rather than later because paths and logging get used by the very first service, and retrofitting
them means touching every file that hardcoded `__dirname` in the meantime.

## Depends on

- `TASK_001` — workspace, tsconfig, eslint boundaries, gate scripts.

## Read first

- `/docs/PASSPORT.md` §4 (domains), §5 (repo layout — build the `apps/studio/src` tree
  exactly), §7 (preload rules), §8 (host layering), §14 (security), §15 (paths, logging, ids).

## Scope

**In scope**
- `apps/studio` with electron-vite configured for three entries: host, preload, renderer
- A single `BrowserWindow` with the strict security flags and a strict CSP
- `host/platform/paths.ts`, `host/platform/logger.ts`, `host/platform/ids.ts`
- The composition root (`host/main.ts`) with an explicit startup sequence
- Dev script with HMR, build script producing an unpackaged app
- A placeholder renderer page — literally a dark screen with the app name

**Out of scope**
- IPC of any kind (TASK_004) — preload exposes nothing yet
- The database (TASK_006)
- React Router, MobX, the UI kit, the nav rail (TASK_007)
- Packaging or installers (TASK_056)

## Steps

1. `apps/studio/package.json`. Install `electron` and `electron-vite` — resolve the current
   stable versions at install time and pin them exactly. Do not copy a version number from
   any document.
2. `electron.vite.config.ts` with three builds: `main` → `src/host/main.ts`, `preload` →
   `src/preload/index.ts`, `renderer` → `src/renderer/index.html`.
3. Create the directory tree from PASSPORT §5 under `apps/studio/src`. Empty folders with a
   `.gitkeep` are fine — the shape being visible from day one is the point.
4. `host/platform/paths.ts` — the **only** place that resolves paths. Export
   `userDataDir`, `dbPath`, `logsDir`, `cacheDir`, `resourcesDir`. Everything else in the
   codebase must call these. No `__dirname` outside this file.
5. `host/platform/logger.ts` — structured JSON lines to `logsDir`, with rotation and a
   level. Also mirror to stdout in development. Every log line carries a `scope`.
6. `host/platform/ids.ts` — UUID v7 generation (time-sortable), used for every entity id.
7. `host/main.ts` — the composition root, with an explicit ordered startup:
   `app.whenReady()` → resolve paths → start logger → (later: migrations, services) →
   create window. Leave clearly-commented gaps where TASK_006 and TASK_004 will insert
   themselves. Handle `window-all-closed`, `second-instance` (single instance lock) and
   `will-quit` cleanup.
8. `host/platform/windows.ts` — window creation. `width: 1440, height: 900` (matches the
   design mockups), `minWidth: 1100`, `backgroundColor: '#191b20'` so there is no white
   flash on boot, `show: false` until `ready-to-show`. Set `webPreferences`:
   `contextIsolation: true`, `nodeIntegration: false`, `sandbox: true`, and the preload path.
9. Set a strict CSP via `session.defaultSession.webRequest.onHeadersReceived`. Allow `self`
   for scripts and styles; allow `https://fonts.googleapis.com` and
   `https://fonts.gstatic.com` only if fonts are loaded remotely (prefer bundling them and
   allowing neither). No `unsafe-eval`, no remote scripts.
10. Block navigation and new windows: `webContents.setWindowOpenHandler` returns
    `{ action: 'deny' }`, and `will-navigate` is cancelled for anything that is not the app's
    own URL. The agent browser window in TASK_045 is a separate, deliberately different case.
11. `preload/index.ts` — for now, an empty file with a comment explaining that TASK_004 adds
    `call` and `subscribe`, and that **nothing else may ever be added here**.
12. Minimal `renderer/index.html` + `renderer/main.tsx` rendering a dark screen with
    "ZVS AI Studio" centred. Install React and `react-dom` (v19). Nothing else.
13. Wire `dev`, `build` and `start` scripts in `apps/studio/package.json` and make sure the
    root Turbo tasks pick them up.
14. Commit: `TASK_002: electron shell and platform services`.

## Verify

```bash
pnpm --filter studio dev     # window opens, dark, app name visible, HMR works on edit
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks — all four must hold:
- DevTools console shows **no** CSP violations and no Node globals available
  (`typeof require` and `typeof process` are both `undefined` in the renderer).
- `window.claude`-style bridge is absent: `Object.keys(window)` shows no custom bridge yet.
- A log file appears under the user-data logs directory with at least the startup line.
- Closing and reopening the app does not spawn a second instance.

## Definition of done

- [x] App boots to a dark window with no flash
- [x] `contextIsolation`, `sandbox`, `nodeIntegration:false` verified in DevTools
- [x] CSP present, no violations logged
- [x] Navigation and window-open are blocked
- [x] Logger writes to disk; paths resolve through `paths.ts` only
- [x] Gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** Electron shell with separate host, preload, renderer and build-config TS projects;
three electron-vite entries; empty CJS preload compatible with sandboxing; dark React 19
placeholder with HMR; strict header and HTML CSP, development-only local WebSocket access,
navigation/redirect and popup blocking. Explicit startup and single-instance lifecycle,
centralized paths, injectable JSONL logger (level, scope, 5 MiB rotation, three backups,
development stdout), UUID v7 factory. Turbo caches out/. Platform tests cover paths,
logger filtering/rotation/restart/close and UUID ordering/uniqueness. All four code gates pass.
Temporary Playwright checks verified production rendering, absent Node globals/bridge,
security preferences, clean console, logging, blocked navigation/popups and second launch.
Development checks observed HMR updating the label without reloading the page; the first
restore check hit Windows watcher timing, and the final rerun was interrupted by the user,
who confirmed the correct visible label. Source is restored to "ZVS AI Studio".

**Deviated from plan:** Pinned Electron 44.2.0, electron-vite 5.0.0 and React 19.2.8 from
npm at installation. Used latest compatible Vite 7.3.6 because electron-vite excludes Vite 8.
Added postinstall install-electron because Electron 44 no longer downloads automatically.
The local Node downloader stalled; downloaded the official archive with curl ranges and
verified its SHA-256 against the package checksums before extraction. Host/config use
skipLibCheck for upstream Electron DOM and optional SWC declarations; own source remains
strict and host has no DOM globals. Used Node tests; committed E2E harness remains TASK_008.
The desktop window manager resized the live window; configured size is still 1440x900.

**Next session should know:** TASK_004 adds only call/subscribe to preload; keep CJS output
and all security flags. TASK_006 insertion points are in host/main.ts, before services/window.
All runtime path construction belongs to platform/paths.ts. Logs are currently under
%APPDATA%/ZVS AI Studio/logs/studio.jsonl. Renderer CSS is linked, not injected, to avoid CSP
inline-style exceptions. HMR remounts the placeholder root; no React Fast Refresh yet.
Temporary runtime tools/screenshots live in ignored node_modules/.task002. Task files and
INDEX remain local under the existing ignore policy; no remote push is authorized.
