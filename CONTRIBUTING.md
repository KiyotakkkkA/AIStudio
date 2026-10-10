# Contributing to ZVS AI Studio

Short and directive on purpose. `/docs/PASSPORT.md` is the binding architecture reference;
`/tasks/INDEX.md` is the backlog and the rules for running one task. This file covers how
work gets verified and landed.

> **Phase M (TASK_058 – TASK_076): the move to Tauri and a Rust host.** While it runs, the
> TypeScript host in `apps/studio/src/host/` is frozen. Its Vitest suite is the specification
> each Rust port must reproduce, so **port the tests together with the code** and do not
> delete them before TASK_076. New host code is Rust and is tested with `cargo test`. The
> sections below describe both halves, and TASK_076 removes the Electron-only parts.

## Testing policy

**Tests are mandatory** for logic that can silently be wrong:

- services and the execution kernel
- repositories and migrations
- the permission model and approval gates
- DTO schemas and the IPC contract, including a golden fixture for every DTO in
  `packages/shared/fixtures/` that both Vitest and `cargo test -p zvs-contract` parse
- all Rust host and core logic (`cargo test`)

**Tests are not required** for presentational components. Snapshot tests over mockup-driven
UI produce churn, not safety. What UI needs is covered once, end to end: `e2e/smoke.spec.ts`
launches the built app and asserts that it renders, routes, calls the host and streams.

Where a rendered surface carries logic worth protecting — a store, a reducer, a selector —
test the logic, not the markup. `apps/studio/tests/renderer/uiStore.test.ts` is the shape to
copy.

## The suite

| Command                  | What it runs                                                                                            |
| ------------------------ | ------------------------------------------------------------------------------------------------------- |
| `pnpm test`              | Every unit test: `packages/*`, host and renderer, in one Vitest run                                     |
| `pnpm test:watch`        | The same suite in watch mode                                                                            |
| `pnpm test:coverage`     | The suite plus a coverage report in `coverage/`                                                         |
| `pnpm test:e2e`          | The Playwright e2e suite against the built app (Electron until TASK_075; Tauri over WebView2 CDP after) |
| `cargo test --workspace` | Rust host, contract, core and sidecar tests                                                             |

Vitest projects live in `vitest.config.ts`: `packages` and `host` run in a Node environment,
`renderer` runs in jsdom. The `host` project disappears at TASK_076, when its tests exist in
Rust. Workspace packages resolve to source, so `pnpm test` needs no
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

Rust helpers (from TASK_060 – TASK_061), in `crates/zvs-host`'s `testing` module:
`temporary_database()` (a migrated SQLite in a temp directory, dropped with the guard; every
repository test uses it), `temporary_directory()` with the same temp-path assertion, and
`FakeClock`. Use `tokio::time::pause()` for timer logic instead of real sleeps.

## Gates

Before a task is done, all of these pass:

```bash
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm test:e2e
pnpm format:check
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same set. Unit gates and the Rust workspace (except `zvs-studio`) run on Linux.
`zvs-studio` clippy, the Tauri build and e2e run on Windows, the only verified target.

## Landing work

- **One commit per task**, message `TASK_{N}: <title>`.
- **A task is not done until its Handoff block is filled in** — what you built, where you
  deviated from the plan, what the next session needs to know — its header says
  `status: done`, and its line in `/tasks/INDEX.md` is ticked.
- Never widen scope. Note what you found in Handoff instead of fixing it.
- Never invent versions; resolve the latest stable at install time and pin it exactly.
- Never commit secrets, tokens or `.env` files.
