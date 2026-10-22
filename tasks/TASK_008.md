---
task: TASK_008
title: Test harness — Vitest, Playwright-Electron smoke, CI gates
phase: 0 — Foundation
status: done
depends_on: [TASK_007]
unblocks: [TASK_009]
estimate: 3 h
---

# TASK_008 — Test harness: Vitest, Playwright-Electron smoke, CI gates

## Context

Phase 0 has produced a real application: it boots, routes, migrates a database, round-trips
IPC and streams events. Tests have been written per-task so far, but there is no single
harness, no e2e coverage and no enforced gate wiring them together. This task closes Phase 0
by making "the app still works" a command rather than a hope.

The testing policy for this project (agreed with the user, and it applies to every task from
here on): **tests are mandatory for logic that can silently be wrong** — services, the kernel,
repositories, the permission model, DTO schemas, Rust core — and **not required for
presentational components**, where snapshot tests on mockup-driven UI produce churn rather
than safety. This task encodes that policy so later agents do not have to re-litigate it.

One e2e smoke test matters more than it looks. Electron apps fail in ways unit tests cannot
see: a native module built against the wrong ABI, a CSP that blocks the bundle, a preload path
that is wrong in the packaged layout. A single Playwright test that launches the real app and
asserts the window renders catches all three.

## Why

From TASK_009 onward every task adds a feature that can break an earlier one. Without a suite
that runs in one command, regressions are found by hand, late, or not at all.

## Depends on

- `TASK_007` — a renderer with routes worth smoke-testing.

## Read first

- `/docs/PASSPORT.md` §3 (test tooling), §16 (build and CI).
- `/tasks/INDEX.md` — "Rules that apply to every task", which this task makes executable.

## Scope

**In scope**
- Vitest configured across the workspace, with a Node environment for host/shared packages
  and a DOM environment for renderer tests
- Test helpers: a temp-database factory, a fake IPC bridge, a fake clock
- Playwright + Electron e2e with one smoke test
- Coverage collection (reported, not gated — a coverage threshold this early rewards
  writing tests for the wrong things)
- CI running the full gate set
- `CONTRIBUTING.md` stating the testing policy so it survives agent handoffs

**Out of scope**
- Broad test coverage of existing code — the per-task tests already written stay as they are
- Visual regression testing. Note it as a candidate for TASK_057
- Rust test wiring (TASK_017, when the first crate exists)

## Steps

1. Install `vitest`, `@vitest/coverage-v8`, `jsdom` (or `happy-dom`), `@testing-library/react`
   and `@playwright/test`. Pin current stable versions.
2. Root Vitest workspace config with two project shapes: `node` for `packages/**` and
   `apps/studio/src/host/**`, `jsdom` for `apps/studio/src/renderer/**`. Keep a single
   `pnpm test` entry point that runs both.
3. `test/helpers/tempDb.ts` — creates a migrated SQLite database in a temp directory and
   returns it plus a cleanup function. Every repository test uses this; none may touch the
   real user-data database. Add an assertion in the helper that the path is under the OS temp
   directory, so a mistake fails loudly instead of eating real data.
4. `test/helpers/fakeBridge.ts` — an in-memory implementation of the preload bridge so
   renderer stores can be tested against the real contract without Electron. It routes
   `call` to registered fake handlers and lets a test push events into `subscribe`.
5. `test/helpers/fakeClock.ts` — a controllable clock. Services take a clock dependency
   (PASSPORT §15) precisely so time-dependent logic is testable; this is that dependency.
6. Playwright config targeting Electron: launch the built app from `out/`, not the dev
   server, so the test exercises what actually ships.
7. Write the smoke test `e2e/smoke.spec.ts`:
   - App launches and a window appears.
   - The nav rail renders with all fourteen items.
   - Navigate to three routes and assert each header.
   - `system.ping` round-trips (assert the rendered value updates).
   - The demo stream from TASK_005 counts to completion.
   - No console errors and no CSP violations during the run.
8. Coverage: collect and write a report, print a summary. Do **not** fail the build on a
   threshold yet — add a `TODO` pointing at TASK_057 to decide a threshold once the shape
   of the codebase is known.
9. Update the CI workflow: `typecheck` → `lint` → `build` → `test` → `e2e`. The e2e job
   needs a display on Linux runners (`xvfb-run`); if CI is Windows-only, note that instead.
10. `CONTRIBUTING.md` — short and directive. The testing policy above, the gate commands, the
    one-commit-per-task rule, and the rule that a task is not done until its Handoff block is
    filled in.
11. Commit: `TASK_008: test harness, e2e smoke and CI gates`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Should CI run on Windows runners only, or Windows plus Linux? v1 targets Windows, but
> Linux runners are faster and cheaper for the unit tests."
>
> **Why it matters:** Decides the workflow matrix and whether the e2e job needs `xvfb`. It
> also determines how early a Linux-only breakage would be caught, given PASSPORT §18
> assumption 3 says Windows is the only verified target.
> **Options:** (a) Windows only — matches the shipping target, slower and costlier;
> (b) unit tests on Linux, e2e on Windows — fast feedback plus a real check of what ships;
> (c) both everywhere — most coverage, most minutes.
> **If the user is unavailable:** take (b). Note it in Handoff.

## Verify

```bash
pnpm test
pnpm test:e2e
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks:
- Break something deliberately — rename a nav item — and confirm the e2e smoke test fails.
  Revert. A green suite that cannot go red is not a suite.
- Confirm `tempDb` never writes to the real user-data folder (check it stays untouched
  after a full test run).

## Definition of done

- [x] `pnpm test` runs host, shared and renderer tests in one command
- [x] Helpers exist for temp DB, fake bridge and fake clock, and are used by existing tests
- [x] E2E smoke launches the built app and asserts rail, routing, ping and stream
- [x] Smoke test proven to fail on a real regression, then restored
- [x] CI runs the full gate set on the agreed matrix
- [x] `CONTRIBUTING.md` states the testing policy
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- **Vitest across the workspace.** `vitest.config.ts` at the root defines three projects:
  `packages` and `host` in a Node environment, `renderer` in jsdom. `pnpm test` is the single
  entry point (`vitest run`); `pnpm test:watch` and `pnpm test:coverage` sit beside it.
  Workspace packages are aliased to their source, so the suite needs no build first.
- **Every existing test moved from `node:test` to Vitest.** The assertions were left alone —
  they still use `node:assert/strict` — only the runner and the helper imports changed.
  `apps/studio/scripts/run-tests.mjs` (which ran `node:test` under Electron-as-node) is gone;
  `better-sqlite3` v13 ships Node-API prebuilds, so the host tests run under plain Node.
  `packages/eslint-config/tests/boundaries.test.js` was converted too and now runs in the
  `packages` project (with a 60 s timeout — it drives a real ESLint instance).
- **Helpers in `test/helpers/`.** `tempDb.ts` (migrated throwaway database), `paths.ts`
  (`temporaryDirectory`, `assertTemporaryPath`, repo path constants), `fakeBridge.ts`
  (in-memory preload bridge, contract-typed, `handle`/`fail`/`emit`/`calls`), `fakeClock.ts`
  (callable clock with `advance`/`set`). `apps/studio/tests/database.ts` was folded into
  `tempDb.ts`. Every temp directory in the suite now goes through `temporaryDirectory`, so the
  "must be under the OS temp dir" assertion covers all of them.
- **`apps/studio/tests/renderer/uiStore.test.ts`** — six tests driving `UiStore` through the
  real IPC client over `fakeBridge`, plus one proving the `EventRouter` wires to the bridge's
  `subscribe`. This is the reference for testing renderer logic without Electron.
- **Playwright-Electron e2e.** `playwright.config.ts` + `e2e/smoke.spec.ts`. It launches the
  built app from `apps/studio/out` with `--user-data-dir` pointed at a temp directory, then
  asserts: a window renders, the rail shows all fifteen items, three routes render their own
  header, `system.ping` round-trips into the DOM, the demo stream counts 1/5 → `готово · ok`,
  and no console error or CSP violation occurred during the run.
- **Coverage** collected by `@vitest/coverage-v8`, written to `coverage/` with a printed
  summary. Not gated; a `TODO(TASK_057)` in `vitest.config.ts` says why.
- **CI** split into two jobs: `gates` on `ubuntu-latest` (typecheck → lint → build →
  test:coverage → format:check, coverage uploaded) and `e2e` on `windows-latest` (build →
  test:e2e, report uploaded on failure).
- **`CONTRIBUTING.md`** — the testing policy, the helper catalogue, the gate commands, one
  commit per task, and "not done until Handoff is filled in".

**Deviated from plan:**

- **CI matrix.** The user was asked the DECISION question verbatim and chose option (b):
  unit gates on Linux, e2e on Windows. No `xvfb` is needed anywhere as a result — a Linux e2e
  job would need it, and that is noted in the workflow.
- **The rail has fifteen items, not fourteen.** The task text says fourteen; TASK_007 built
  fourteen routes plus the `Браузер` entry that opens a window instead of navigating, so
  fifteen buttons render. The smoke test asserts fifteen against a hardcoded list of labels.
  Note that PASSPORT §1 is itself inconsistent here: it says "thirteen routed pages" and then
  tables fourteen. Neither was changed — flagged, not fixed.
- **A small renderer surface had to be added.** Nothing in the app rendered a ping result or
  stream progress, so there was nothing for step 7 to assert. `renderer/features/diagnostics/
  HostCheck.tsx` is a two-button panel on the Settings page that calls `system.ping` and
  `system.demoStream` and renders the results, tagged with `data-testid` attributes. It is the
  minimum the required assertions need; TASK_052 owns the Settings page and can absorb or
  replace it.
- **Per-package `test` scripts were removed** (`packages/shared`, `packages/ipc`,
  `packages/eslint-config`, `apps/studio`) along with the `test` task in `turbo.json`, because
  the task asks for one entry point. The `tsc -p tsconfig.test.json` half of those scripts was
  already duplicated in each package's `typecheck`, so nothing was lost. A root
  `tsconfig.test.json` and a `//#typecheck:root` turbo task now typecheck `test/` and `e2e/`.

**Next session should know:**

- `pnpm test` needs no build. `pnpm test:e2e` does — it drives `apps/studio/out`. If the smoke
  test reports a missing `out/host/main.js`, that is what happened.
- Repository tests must use `temporaryDatabase()` from `test/helpers/tempDb.ts`. It refuses to
  work outside the OS temp directory, which is the guard against a test eating the real
  user-data database.
- `createFakeBridge<Contract>()` is contract-typed: a handler returning the wrong shape is a
  typecheck failure, not a runtime surprise. That already caught one mistake while writing
  `uiStore.test.ts`.
- The smoke test was deliberately broken (a nav label renamed) and it failed on two assertions
  before being restored, so it is known to be able to go red.
- Visual regression testing stays out of scope — TASK_057 is the candidate home, alongside the
  coverage threshold decision.
- Rust test wiring is still TASK_017; the placeholder comment for it lives in the `gates` job.

---

**Phase 0 is complete when this task is done.** The app boots, routes, persists, talks across
IPC, streams events, and has a suite that proves it. Everything from TASK_009 is a feature.
