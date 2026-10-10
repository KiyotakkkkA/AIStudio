# ZVS AI Studio

Local-first desktop automation studio. The binding architecture is in
[docs/PASSPORT.md](docs/PASSPORT.md); the backlog and the rules for running a task are in
`tasks/INDEX.md`.

## Moving to Tauri

On 2026-10-10 we decided to move the studio from Electron to **Tauri 2 with the whole host
rewritten in Rust** (PASSPORT D10). Phase M of the backlog (TASK_058 – TASK_076) does the
move one domain at a time. Until TASK_076 lands, two apps live in this repository:

|       | Electron (current, frozen)            | Tauri (target, growing)                         |
| ----- | ------------------------------------- | ----------------------------------------------- |
| Run   | `pnpm dev`                            | `pnpm dev:tauri`                                |
| Shell | `apps/studio/src/host/main.ts`        | `apps/studio/src-tauri` (`zvs-studio`)          |
| Host  | TypeScript in `apps/studio/src/host/` | Rust in `crates/zvs-host`                       |
| Data  | `%APPDATA%\ZVS AI Studio`             | `%APPDATA%\<bundle identifier>`, starting empty |

The TypeScript host is **frozen**. No feature work goes into it, because it is the behavioural
specification the Rust port is checked against. Channels not yet ported answer
`NOT_IMPLEMENTED` in the Tauri app. TASK_076 deletes Electron, the TS host, the napi addon,
Drizzle and `better-sqlite3`, and makes `pnpm dev` the Tauri app.

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

`pnpm test:e2e` runs the Playwright suite against the built app. After TASK_075 it attaches
over WebView2's DevTools Protocol to a build with the `e2e` cargo feature.

## Layout

```
apps/studio/src           renderer (React + MobX), browser-ui chrome; the frozen TS host until TASK_076
apps/studio/src-tauri     zvs-studio: Tauri shell, composition root, webviews, OS ports
crates/zvs-host           the Rust host: IPC server, services, kernel, repositories, drivers
crates/zvs-contract       serde mirror of packages/shared, bound by golden fixtures
crates/zvs-core           pure compute: chunking, embedding, LanceDB, graph
crates/zvs-jobd           sidecar for long or crash-prone jobs
packages/shared           Zod DTOs, the IPC contract, error codes, events, fixtures
packages/ipc              typed renderer client over the bridge
```

`tasks`, `design` and `references` are kept local (see `.gitignore`).
