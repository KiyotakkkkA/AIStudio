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
UI produce churn, not safety.

Where a rendered surface carries logic worth protecting — a store, a reducer, a selector —
test the logic, not the markup. `apps/studio/tests/renderer/uiStore.test.ts` is the shape to
copy.

Playwright was removed as unnecessary by project decision on 2026-11-18. Do not add or
run Playwright tests or require them as a gate. Keep automated coverage in Vitest and Rust
tests; verify presentation manually when needed.

## The suite

| Command                  | What it runs                                                         |
| ------------------------ | -------------------------------------------------------------------- |
| `pnpm test`              | Every TS unit test: `packages/*` and the renderer, in one Vitest run |
| `pnpm test:watch`        | The same suite in watch mode                                         |
| `pnpm test:coverage`     | The suite plus a coverage report in `coverage/`                      |
| `cargo test --workspace` | Rust host, contract, core and sidecar tests                          |

Vitest projects live in `vitest.config.ts`: `packages` runs in a Node environment, `renderer`
runs in jsdom. Workspace packages resolve to source, so `pnpm test` needs no build.
The native `account_gateway` integration test uses the `e2e` cargo feature and real WebView2;
it does not depend on Playwright.

Coverage is reported, never gated. A threshold chosen this early would reward writing tests
for the wrong things; TASK_057 decides one when the codebase has a settled shape.

## Test helpers

Use these rather than rolling your own — `test/helpers/`:

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
pnpm format:check
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same set. Unit gates and the Rust workspace (except `zvs-studio`) run on Linux.
`zvs-studio` clippy and the Tauri build run on Windows, the only verified target.

## Landing work

- **One commit per task**, message `TASK_{N}: <title>`.
- **A task is not done until its Handoff block is filled in** — what you built, where you
  deviated from the plan, what the next session needs to know — its header says
  `status: done`, and its line in `/tasks/INDEX.md` is ticked.
- Never widen scope. Note what you found in Handoff instead of fixing it.
- Never invent versions; resolve the latest stable at install time and pin it exactly.
- Never commit secrets, tokens or `.env` files.
