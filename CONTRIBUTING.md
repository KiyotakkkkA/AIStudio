# Contributing to ZVS AI Studio

Short and directive on purpose. `/docs/PASSPORT.md` is the binding architecture reference;
`/tasks/INDEX.md` is the backlog and the rules for running one task. This file covers how
work gets verified and landed.

The studio is a Tauri app with a Rust host (PASSPORT D10). Host code is Rust and is tested
with `cargo test`; Vitest covers the workspace packages and the renderer.

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

| Command                  | What it runs                                                           |
| ------------------------ | ---------------------------------------------------------------------- |
| `pnpm test`              | Every TS unit test: `packages/*` and the renderer, in one Vitest run   |
| `pnpm test:watch`        | The same suite in watch mode                                           |
| `pnpm test:coverage`     | The suite plus a coverage report in `coverage/`                        |
| `pnpm test:e2e`          | The Playwright e2e suite against the built Tauri app over WebView2 CDP |
| `cargo test --workspace` | Rust host, contract, core and sidecar tests                            |

Vitest projects live in `vitest.config.ts`: `packages` runs in a Node environment, `renderer`
runs in jsdom. Workspace packages resolve to source, so `pnpm test` needs no build.
`pnpm test:e2e` **does**: its global setup runs `scripts/tauri.mjs build --no-bundle --features
e2e` once (skip it with `ZVS_E2E_PREBUILT=1` when the build is fresh) and drives
`target/release/zvs-studio.exe`, not the dev server, so that it exercises what actually ships.
`e2e/launch.ts` starts the app with `ZVS_DATA_DIR` in a temp folder and a free
`ZVS_E2E_CDP_PORT`, then attaches with `chromium.connectOverCDP`. Both variables exist only in
builds with the `e2e` cargo feature. Site webviews have no debugging port, so their behaviour is
asserted through fixture pages that report to the test's local server. An e2e build also
compiles `zvs-core`'s `seed_vectors` example, which `seedVectors` in `e2e/launch.ts` runs to
write fixture vectors straight into a store.

Coverage is reported, never gated. A threshold chosen this early would reward writing tests
for the wrong things; TASK_057 decides one when the codebase has a settled shape.

## Test helpers

Use these rather than rolling your own — `test/helpers/`:

- `paths.ts` — `temporaryDirectory()` and `assertTemporaryPath()`. Anything a test writes to
  or deletes must sit under the OS temp directory; the assertion fails loudly if it does not.
  On Windows, `dispose()` tolerates a file another process still holds (`EPERM`, `EBUSY`,
  `ENOTEMPTY`): the directory stays under the temp root and is removed again when the process
  exits. Any other failure still throws.
- `fakeBridge.ts` — `createFakeBridge()` is an in-memory `window.zvs` bridge. Renderer stores
  can be tested against the real IPC contract without Tauri: register handlers with `handle`,
  push events with `emit`.
- `fakeClock.ts` — `createFakeClock()` is a controllable clock. Stores take a clock
  dependency (PASSPORT §15) precisely so time-dependent logic is testable; pass this one.
- `fakeMcpServer.mjs` — a scriptable MCP server the Rust MCP tests launch with `node`.

Rust helpers, in `crates/zvs-host`'s `testing` module:
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
