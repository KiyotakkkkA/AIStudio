---
task: TASK_007
title: Renderer shell — tokens, uikit, router, root store, nav rail, empty routes
phase: 0 — Foundation
status: done
depends_on: [TASK_005, TASK_006]
unblocks: [TASK_008, TASK_012]
estimate: 4 h
---

# TASK_007 — Renderer shell: tokens, uikit, router, root store, nav rail, empty routes

## Context

The renderer is currently a dark placeholder screen. This task builds the app's chrome — the
navigation rail, the page templates, routing and the MobX root store — so that every later
feature task is "fill in a page" rather than "invent the shell".

The visual spec is not a description, it is a set of files. `/design/*.dc.html` contains
fifteen pixel-level mockups; the rail appears in thirteen of them identically. Open
`/design/Main.dc.html` and read the `<aside>` element — it is the rail, with exact widths,
heights, colours and icon paths. Copy those numbers rather than approximating them. The design
tokens are already named the way the app should define them:
`--color-main-50` … `--color-main-900`, `--color-accent-light|medium|dark`, plus three status
hues `--color-ok|warn|error`. `/design/Foundations.dc.html` is the token and component sheet.

Two organizing axes, per PASSPORT §13 and decision D6. `ui/` is presentational and domain-free
— it never imports a store. `features/<domain>/` owns everything that knows about a domain —
its store, view-models and domain-aware organisms. `pages/` compose a template with feature
organisms and wire nothing else. One component per `.tsx`, named for the component.

The UI kit is `@kiyotakkkka/zvs-uikit-lib` (verified: version 8.3.0, ESM-only, peers React 19
and React DOM 19, Tailwind-based, exports `.`, `./chart`, `./code-view`, `./server` and
`./styles.css`, and pulls in `clsx`, `tailwind-merge`, `shiki`, `recharts` and
`@tanstack/react-virtual`).

## Why

Thirteen page tasks follow this one. If the rail, templates and store wiring are not settled
here, each of those tasks re-invents them slightly differently and the app looks assembled
rather than designed.

## Depends on

- `TASK_005` — `EventRouter`, needed by the root store.
- `TASK_006` — `settings.get`/`set`, used to persist UI preferences.

## Read first

- `/design/Main.dc.html` — the rail, header and three-column shell, with exact values.
- `/design/Foundations.dc.html` — colour tokens, type scale, control geometry, and the
  rest/active states of a nav item and a list row.
- `/design/canvas.json` — which artboard corresponds to which page.
- `/docs/PASSPORT.md` §13 (renderer structure and MobX rules), §17 (naming).

## Scope

**In scope**
- Tailwind configured, including the kit's `dist` in `content` so its classes are scanned
- Kit `styles.css` imported once at app root; tokens defined as CSS custom properties
- `react-router` with hash history, one route per page, lazy-loaded
- MobX root store, created once, provided by context, consumed by a typed hook
- `ui/templates/`: the app shell (rail + content) and the three-column page template
- `ui/organisms/NavRail.tsx` built from the design file, all fourteen items and four groups
- Thirteen page components, each rendering its real header and an explicit empty state
- `UiStore` persisting rail collapse and last route through `settings.*`

**Out of scope**
- Any feature logic, data or store beyond `UiStore` — pages stay empty
- The document viewer dock (TASK_044) and the browser window (TASK_045)
- Dark/light theming — dark only for v1; define tokens so a light theme is a token swap

## Steps

1. Install `react-router`, `mobx`, `mobx-react-lite`, `tailwindcss`,
   `@kiyotakkkka/zvs-uikit-lib` (peers React 19 / React DOM 19 are already satisfied).
2. Tailwind config: `content` must include `apps/studio/src/renderer/**` **and**
   `node_modules/@kiyotakkkka/zvs-uikit-lib/dist/**`, or the kit's classes get purged and
   its components render unstyled. This is PASSPORT §18 assumption 7.
3. `renderer/app/theme.css` — define every token from `/design/Foundations.dc.html` on
   `:root` as a CSS custom property, using the exact hex values. Import the kit's
   `styles.css` once, before the app's own styles.
4. `renderer/app/root.tsx` — mount, providers, error boundary. Create the root store here,
   exactly once, and provide it by context. **No module-level singleton store** — it breaks
   tests and HMR (PASSPORT §13).
5. `renderer/stores/RootStore.ts` holding `ui: UiStore` for now, plus a slot per future
   domain store with a `TODO` naming the task that fills it. `renderer/stores/useStore.ts`
   is the typed hook every component uses.
6. `UiStore`: rail collapsed flag, active route, viewer dock width (for TASK_044). Persist
   collapse and last route via `settings.*` and restore on boot. `makeAutoObservable` in the
   constructor; mutations only in actions.
7. `ui/templates/AppShell.tsx` — rail plus a content slot. `ui/templates/PageShell.tsx` —
   the 56 px header row with title, subtitle and an actions slot, matching the mockups.
8. `ui/organisms/NavRail.tsx` — build from `/design/Main.dc.html`. Fourteen items in four
   groups: **Workspace** (Secrets & Identity, AI Providers, Vector Stores), **Build** (Chat,
   Agentic Build, Scenarios, Browser), **Extend** (Skills, Connections, Integrations,
   Tools), **System** (Tasks, Downloads, Runs & Logs, Settings). Active state is
   `bg-main-700` with a 2 px accent inset on the left. Badges and status dots as drawn.
   `Browser` is the one item that opens a window rather than navigating (TASK_045) — render
   it with the ↗ marker and wire it to a no-op with a `TODO` for now.
9. Extract the icons from the design files as inline SVG components in `ui/atoms/icons/`,
   one per file, 20×20 viewBox, `stroke="currentColor"`, stroke-width 1.4, so a row's state
   colours its icon for free.
10. Routing: hash history, a route per page, `React.lazy` per page so the initial bundle
    stays small. Route paths are lowercase and match the domain names used on channels
    (`/providers`, `/vector-stores`, …).
11. Thirteen page components under `pages/`, each rendering `PageShell` with the correct
    title and subtitle from its mockup, and an honest empty state naming the task that will
    fill it (for example, "Providers land here in TASK_016"). Do **not** stub fake data.
12. Check the rail at the mockup's 1440×900 and at the 1100 px minimum width. Fix what
    breaks; the rail is fixed-width, the content column flexes.
13. Commit: `TASK_007: renderer shell, nav rail and routing`.

## Verify

```bash
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks:
- All thirteen routes reachable from the rail; active state matches the mockup.
- Compare the rail side by side with `/design/Main.dc.html` opened in a browser. Widths,
  row heights, group labels and colours should match, not merely resemble.
- Collapse the rail, quit, reopen — the collapsed state and last route are restored.
- Resize to 1100 px: nothing overlaps or clips.
- Try importing a store inside a `ui/` component — `pnpm lint` must fail.

## Definition of done

- [x] Thirteen routes render with correct headers and honest empty states
- [x] Nav rail matches the design file, including groups, badges and active state
- [x] Tokens defined from `Foundations.dc.html` and used everywhere (no raw hex in components)
- [x] Root store provided by context, consumed by hook, no singleton
- [x] UI preferences persist across restart
- [x] `ui/` cannot import stores (lint proven)
- [x] Gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**
Tailwind v4 (`@tailwindcss/vite`) with `renderer/app/theme.css` — `@import "tailwindcss"`, the
kit's `styles.css`, `@source` pointing at `node_modules/@kiyotakkkka/zvs-uikit-lib/dist`, and an
`@theme` block that overrides the kit's palette with the exact `Foundations.dc.html` hexes
(`--color-main-50…900`, `--color-accent-light|medium|dark`, `--color-ok|warn|err|error`) plus
`--font-sans` / `--font-mono` and the radius/geometry tokens. No raw hex outside `theme.css`.

`react-router-dom` with hash history (`createHashRouter`), one lazy route per page under
`renderer/app/router.tsx`, `/` redirecting to `/secrets` and `*` back to it. All navigation goes
through `renderer/hooks/useAppNavigation.ts` (`current`, `pathname`, `isActive`, `go`, `replace`).

MobX root store created once in `app/StoreProvider.tsx` via `useState`, provided by
`stores/StoreContext.ts`, read through `stores/useStore.ts`. `RootStore` holds `ui: UiStore` and
exposes the `ipc` / `events` environment. `UiStore` keeps `railCollapsed`, `activeRoute` and
`viewerDockWidth`, persists the first two through `settings.set` (`ui.rail.collapsed`,
`ui.route.last`) and restores them in `boot()`.

`ui/templates/AppShell.tsx` (rail + content slot) and `ui/templates/PageShell.tsx` (56 px header
with title, subtitle, actions slot and an optional 46 px toolbar row).
`ui/organisms/NavRail.tsx` + `ui/molecules/NavRailItem.tsx` built from `Main.dc.html`: 248 px
rail on `main-900` with a `main-700` right border, 56 px branded header, four groups, fourteen
items at 32 px with `bg-main-700` and `inset 2px 0 0 var(--color-accent-dark)` when active,
badge and status-dot slots, the ↗ marker on `Browser`, and the identity footer. Collapsed the
rail is 64 px, icon-only, with group separators and a status pip on the icon.

Thirteen page components under `pages/` (fourteen files — `Tasks` and `Runs & Logs` are separate
routes, both filled by TASK_026), each rendering `PageShell` with its Russian title and subtitle
and an `EmptyState` from the kit naming the task that will fill it.

**Deviated from plan:**
- **`react-router-dom`, pinned 7.18.3, not `react-router` 8.** Requested mid-task. `react-router-dom`
  has no 8.x release, so `react-router` is pinned to the matching 7.18.3 to keep one copy in the tree.
- **Icons come from `@mdi/js`, not hand-extracted SVG in `ui/atoms/icons/`.** Requested mid-task.
  One `ui/atoms/Icon.tsx` renders a 24×24 filled MDI path with `fill="currentColor"`, so a row's
  state still colours its icon. Step 9's stroke-1.4 20×20 set was removed; the rail therefore uses
  MDI outline glyphs rather than the exact glyphs drawn in the design.
- **Host CSP relaxed in development only.** `installContentSecurityPolicy` sent `style-src 'self'`
  as a response header, which overrode the meta tag and blocked Vite's inline `<style>` injection —
  Tailwind did not apply at all inside Electron. Dev now sends `style-src 'self' 'unsafe-inline'`;
  packaged builds keep `style-src 'self'`, and the production bundle links a real `.css` file. The
  kit's main entry ships no inline styles, so strict CSP holds there.
- **No badge or dot values are supplied.** The rail renders them when a `NavItemModel` carries
  `badge` / `status`, but no item declares one yet — there is no data, and stubbing counts would
  be fake.
- **No `TODO` comments and no comments at all**, per the session instruction. The future-store
  slots on `RootStore` are therefore not present as commented placeholders; each domain store is
  added by its own task.
- `tsconfig.renderer.json` gained `ESNext.Collection` in `lib` — mobx 7's `ObservableSet` types
  reference `ReadonlySetLike`, which ES2023 does not declare.
- The renderer's `public/style.css` placeholder and the `Placeholder` demo screen in `main.tsx`
  are gone; the demo stream button lived only there. `window.zvsReplay` and the `EventRouter`
  connection are still wired in `main.tsx`.

**Next session should know:**
- Verified against a stub-bridge harness on the Vite dev server: rail 248 px on `#0e0f12` with a
  `#2a2d33` border, items 32 px / 13 px / 500 with `inset 2px 0 0 #85aad6` when active, group
  labels 10 px `.09em` `#6e747e`, header 56 px on `#0e0f12`, body `#191b20` — all matching the
  design file. All fourteen routes render their headers; collapse writes `ui.rail.collapsed` and
  navigation writes `ui.route.last`; at 1100 px nothing overflows horizontally.
- `ui/` cannot import `stores/` or `features/` — proven by pointing eslint at a probe file:
  `Unexpected path "../../stores/useStore" imported in restricted zone`.
- `PageShell` already takes a `toolbar` slot for the 46 px tab row several mockups use.
- `UiStore.viewerDockWidth` (clamped 320–720) exists for the viewer dock in TASK_044 but nothing
  reads it yet.
- The `Browser` rail item calls `openBrowserWindow()` in `app/AppFrame.tsx`, which only logs; wire
  it to the real window in TASK_045.
- Recharts (`@kiyotakkkka/zvs-uikit-lib/chart`) does use inline styles — whoever adds the first
  chart must revisit the production `style-src`.
