# ZVS AI Studio

Local-first desktop automation studio. The binding architecture is in
[docs/PASSPORT.md](docs/PASSPORT.md); the backlog and the rules for running a task are in
`tasks/INDEX.md`.

## Architecture

The studio is a **Tauri 2** app with the whole host in **Rust** (PASSPORT D10). The renderer
is React + MobX in a WebView2 window. It reaches the host through two Tauri commands:
`ipc_call` for request/response over the contract in `packages/shared`, and
`events_subscribe` for streams. Data lives in `%APPDATA%\com.zvs.aistudio`.

| Command        | What it does                                                                  |
| -------------- | ----------------------------------------------------------------------------- |
| `pnpm dev`     | Builds the workspace packages and the sidecar, then `tauri dev` with Vite HMR |
| `pnpm build`   | The web build plus `tauri build --no-bundle`: `target/release/zvs-studio.exe` |
| `pnpm package` | `tauri build`; bundling is switched on by the installer task (TASK_056)       |

## Requirements

- Node 26 and pnpm 12.3.4 (pinned in `package.json`). They are dev tooling only and do not
  ship with the app.
- Rust stable (the version in PASSPORT §3), with the MSVC build tools on Windows.
- `protoc` (verified with 36.1), needed by LanceDB. See `crates/zvs-core/README.md`.
- WebView2 runtime. It ships with Windows 11 and current Windows 10.

## Commands

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm format:check
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`pnpm test:e2e` runs the Playwright suite against the built app. It attaches over WebView2's
DevTools Protocol to a build with the `e2e` cargo feature.

## Layout

```
apps/studio/src           renderer (React + MobX), browser-ui chrome, recovery page
apps/studio/src-tauri     zvs-studio: Tauri shell, composition root, webviews, OS ports
crates/zvs-host           the Rust host: IPC server, services, kernel, repositories, drivers
crates/zvs-contract       serde mirror of packages/shared, bound by golden fixtures
crates/zvs-core           pure compute: chunking, embedding, LanceDB, graph
crates/zvs-jobd           sidecar for long or crash-prone jobs
packages/shared           Zod DTOs, the IPC contract, error codes, events, fixtures
packages/ipc              typed renderer client over the bridge
```

`tasks`, `design` and `references` are kept local (see `.gitignore`).
