# Contributing to ZVS AI Studio

Short and directive on purpose. `/docs/PASSPORT.md` is the binding architecture reference;
`/tasks/INDEX.md` is the backlog and the rules for running one task. This file covers how
work gets verified and landed.

## Testing policy

**Tests are mandatory** for logic that can silently be wrong:

- services and the execution kernel
- repositories and migrations
- the permission model and approval gates
- DTO schemas and the IPC contract
- Rust core logic (`cargo test`)

**Tests are not required** for presentational components. Snapshot tests over mockup-driven
UI produce churn, not safety. What UI needs is covered once, end to end: `e2e/smoke.spec.ts`
launches the built app and asserts that it renders, routes, calls the host and streams.

Where a rendered surface carries logic worth protecting — a store, a reducer, a selector —
test the logic, not the markup. `apps/studio/tests/renderer/uiStore.test.ts` is the shape to
copy.

## The suite

| Command              | What it runs                                                        |
| -------------------- | ------------------------------------------------------------------- |
| `pnpm test`          | Every unit test: `packages/*`, host and renderer, in one Vitest run |
| `pnpm test:watch`    | The same suite in watch mode                                        |
| `pnpm test:coverage` | The suite plus a coverage report in `coverage/`                     |
| `pnpm test:e2e`      | The Playwright-Electron smoke test against the built app            |

Vitest projects live in `vitest.config.ts`: `packages` and `host` run in a Node environment,
`renderer` runs in jsdom. Workspace packages resolve to source, so `pnpm test` needs no
build. `pnpm test:e2e` **does** — run `pnpm build` first; it drives `apps/studio/out`, not
the dev server, so that it exercises what actually ships.

Coverage is reported, never gated. A threshold chosen this early would reward writing tests
for the wrong things; TASK_057 decides one when the codebase has a settled shape.

## Test helpers

Use these rather than rolling your own — `test/helpers/`:

- `tempDb.ts` — `temporaryDatabase()` gives a migrated SQLite database in a throwaway
  directory plus a `dispose()`. **Every** repository test uses it. No test may open the real
  user-data database.
- `paths.ts` — `temporaryDirectory()` and `assertTemporaryPath()`. Anything a test writes to
  or deletes must sit under the OS temp directory; the assertion fails loudly if it does not.
- `fakeBridge.ts` — `createFakeBridge()` is an in-memory preload bridge. Renderer stores can
  be tested against the real IPC contract without Electron: register handlers with `handle`,
  push events with `emit`.
- `fakeClock.ts` — `createFakeClock()` is a controllable clock. Services take a clock
  dependency (PASSPORT §15) precisely so time-dependent logic is testable; pass this one.

## Gates

Before a task is done, all of these pass:

```bash
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm test:e2e
pnpm format:check
```

CI runs the same set: unit gates on Linux, the e2e smoke on Windows.

## Landing work

- **One commit per task**, message `TASK_{N}: <title>`.
- **A task is not done until its Handoff block is filled in** — what you built, where you
  deviated from the plan, what the next session needs to know — its header says
  `status: done`, and its line in `/tasks/INDEX.md` is ticked.
- Never widen scope. Note what you found in Handoff instead of fixing it.
- Never invent versions; resolve the latest stable at install time and pin it exactly.
- Never commit secrets, tokens or `.env` files.
