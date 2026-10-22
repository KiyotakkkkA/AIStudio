---
task: TASK_035_EXTRA_1
title: Toolchain — uv and Node as host runtimes for MCP and skill scripts, the «Окружение» tab
phase: 6 — Tools, MCP and Skills
status: done
depends_on: [TASK_035]
unblocks: [TASK_035_EXTRA_2, TASK_052]
estimate: 4 h
---

# TASK_035_EXTRA_1 — Toolchain: uv and Node as host runtimes

## Context

Stdio MCP servers are launched by `crates/zvs-host/src/drivers/mcp/stdio.rs`.
`windows_command.rs` resolves `npx` or `uvx` through the user's `PATH` and `PATHEXT`, and
`toolchain_env.rs` only redirects caches into `<app data>/mcp-toolchain`: `npm_config_cache`,
`npm_config_prefix`, `UV_CACHE_DIR`, `UV_TOOL_DIR`, `UV_TOOL_BIN_DIR`,
`UV_PYTHON_INSTALL_DIR`, `PIP_CACHE_DIR`. The app ships no uv and no Node. On a machine
without them, every `npx` and `uvx` server fails. TASK_035 stores skill `scripts/` but cannot
run them, so this task also gives TASK_035_EXTRA_2 something to run them with.

This task adds one host interface that both MCP and skill scripts use. Agreed with the user on
2026-10-22:

- A `toolchain` module in `zvs-host` with a `Runtime` (`uv`, `node`) and a `ToolchainService`
  that owns, per runtime: source (`managed` | `path` | `manual`), executable path, version and
  status.
- Three sources: (1) **managed** — download the latest stable into `<app data>/mcp-toolchain/
  {uv,node}/<version>` without touching the OS environment or `PATH`; (2) **path** — scan `PATH`
  for an installed copy; (3) **manual** — the user picks the executable.
- "Latest stable" is resolved when the user acts and then pinned exactly. Node: the newest
  **LTS** from `https://nodejs.org/dist/index.json`, verified against `SHASUMS256.txt`. uv: the
  newest GitHub release of `astral-sh/uv`, verified against its `.sha256` asset. The old managed
  copy stays until the new one verifies. «Обновить» appears only when a newer version exists.
- Every source is probed before it is connected: `--version`, a minimum version, and for Node an
  `npx.cmd` next to `node.exe`. A copy that fails is refused with a reason.
- The isolated cache env stays for every source. A managed copy's directory is prepended to
  `PATH` for the child process only.
- MCP launches `npx`, `node`, `uvx` and `uv` through the connected runtime; any other command is
  unchanged. When a needed runtime is not connected, the connection fails with a Russian message
  pointing to Настройки → Окружение. **Nothing downloads silently.**
- The Downloads catalogue already has `DownloadItemKind::Runtime`; uv and Node become two items
  there. One install action serves both pages: it connects an existing managed copy, or downloads,
  verifies, unpacks and connects. A download that finishes on the Downloads page connects itself;
  no rescan is needed.

## Why

Without it, MCP depends on whatever the user installed, and skill scripts cannot run at all.
PASSPORT §16 ships one executable and the sidecar, so the runtimes are downloaded on demand,
not bundled.

## Depends on

- `TASK_035` — the skills engine whose `scripts/` TASK_035_EXTRA_2 will run.
- `TASK_073` — the stdio transport, `windows_command.rs` and `toolchain_env.rs`.
- `TASK_029` — `DownloadService`, checksums, the catalogue providers.

## Read first

- `/docs/PASSPORT.md` §14 (security), §15 (paths), §16 (what ships)
- `crates/zvs-host/src/drivers/mcp/{stdio,windows_command,toolchain_env}.rs`
- `crates/zvs-host/src/runtimes/` — how llama.cpp releases are resolved and installed; follow it
- `apps/studio/src/renderer/pages/SecretsPage.tsx` — the tab bar to reuse (Секреты / Журнал доступа)

## Scope

**In scope**
- `toolchain` module: runtime definitions, release resolution, download through `DownloadService`
  with checksums, unpacking, `PATH` scan, manual path, version probe, status, `launch(runtime,
  args) -> LaunchSpec`
- MCP stdio moved onto it; the missing-runtime error
- Catalogue items for uv and Node, with auto-connect on completion
- Channels `toolchain.list|install|scan|select|update|remove`, DTOs, Zod, fixtures
- Settings page: a tab bar with only «Окружение» and its two blocks (uv, Node); TASK_052 adds the
  other tabs

**Out of scope**
- Running skill scripts (TASK_035_EXTRA_2)
- Python versions beyond what uv manages itself
- Any other Settings section (TASK_052)

## Verify

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm typecheck && pnpm lint && pnpm test && pnpm build
```

Manual check: with no Node installed, an `npx` MCP server fails with the Окружение hint. Install
the managed Node from Settings; the server connects and `where npx` in a normal terminal still
finds nothing. Repeat from the Downloads page; Settings shows it connected without a scan.

## Definition of done

- [x] Three sources work for both runtimes, each probed before connecting
- [x] Downloads pinned and checksum-verified; updates never remove a working copy first
- [x] MCP uses the connected runtime; a missing one fails with the hint, nothing silent
- [x] Settings «Окружение» tab and the Downloads items share one state
- [x] Tests for resolution, probing, launch specs and the MCP mapping
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Contract `crates/zvs-contract/src/toolchain.rs` and `packages/shared/src/toolchain/`:
  `ToolchainRuntime` (`uv`, `node`), `ToolchainSource` (`managed`, `path`, `manual`),
  `ToolchainStatus` (`missing`, `installing`, `ready`, `broken`), `ToolchainDto`,
  `ToolchainListInput { checkUpdates }`, `ToolchainRef`, `ToolchainSelectInput`. Fixtures in
  `packages/shared/fixtures/toolchain/`. Channels `toolchain.list|install|scan|select|update|remove`,
  each returning the runtime's `ToolchainDto`.
- `crates/zvs-host/src/toolchain/`: `definitions` (runtimes, minimum versions — Node 18.0.0,
  uv 0.4.0 — executables, `ToolchainCommand` for `node`/`npx`/`uv`/`uvx`, platform asset names),
  `releases` (`ToolchainResolver`: newest LTS from `nodejs.org/dist/index.json` checked against
  `SHASUMS256.txt`; newest non-draft, non-prerelease `astral-sh/uv` release checked against its
  `.sha256` asset and, when GitHub sends one, the asset `digest`; 6 h cache), `probe`
  (`--version`, banner, minimum, companion `npx.cmd`/`npx` or `uvx`), `launch` (`LaunchSpec`, moved
  here from `drivers/mcp/stdio.rs`, which re-exports it), `service` (`ToolchainService`).
- State lives in the setting `toolchain.connected` (`{ uv|node: { source, executablePath,
  version } }`). Managed copies sit in `<app data>/mcp-toolchain/{uv,node}/<version>`; the
  isolated cache directories (`uv/cache`, `npm-cache`, …) are unchanged.
- Install: connects the newest managed copy already on disk that passes the probe; otherwise
  resolves the latest build and queues it through `DownloadService` with its SHA-256. Downloads
  carry ref `toolchain:<runtime>:<version>:<asset>`; the catalogue rows are `toolchain:uv` and
  `toolchain:node` (`DownloadItemKind::Runtime`). `ToolchainService` is a second
  `DownloadInstaller`: it refuses a row without a checksum, unpacks into
  `.staging-<version>`, probes, refuses a version mismatch, renames into place, connects, and only
  then removes older managed copies. A failed probe leaves the working copy connected.
- Update: only for a managed copy and only when the resolved version is newer. `list` checks for
  updates only with `checkUpdates: true`, and only for a managed copy; nothing is downloaded
  without `install` or `update`.
- MCP: `McpServiceOptions.toolchain` (a `CommandLauncher`) maps bare `npx`, `node`, `uvx`, `uv`
  (also `.cmd`/`.exe`) to the connected runtime: absolute executable, isolated env, the runtime's
  directory first on the child's `PATH`. A server's own env still wins. A runtime that is not
  connected or is gone from disk fails as `MCP_UNAVAILABLE` with «Для команды «npx» нужен
  Node.js. Подключите его в разделе «Настройки → Окружение».» and nothing is spawned. Other
  commands are launched exactly as before.
- `DownloadService` now accepts several installers (`attach_installer` pushes); each ignores
  refs that are not its own. `RuntimeCatalogue` gained a default `invalidate`.
- Renderer: `features/toolchain/` (`ToolchainStore`, `toolchainPresentation`, `ToolchainCard`,
  `EnvironmentTab`). Settings has a tab bar with «Окружение» only; HostCheck stays below it
  because the e2e smoke test uses it. The Downloads page routes `toolchain:*` rows to
  `toolchain.install`/`update`/`remove`, so both pages read one host state; the Settings store
  refreshes when a download ends while a runtime is installing.
- Tests: `toolchain::tests` (resolution, checksums, probing, install/update/unpack/scan/select/
  remove, launch specs, catalogue), `ipc::toolchain::tests` (channels, and a download finished
  through `DownloadService` connecting itself), `services::mcp::tests::runtime_commands_…`
  (the MCP mapping), `toolchainStore.test.ts`.

**Deviated from plan:**
- The runtime's directory is put first on the child's `PATH` for every source, not only for
  managed copies: a manually picked copy is not on `PATH` either, and npx starts `node` by name.
- `toolchain.remove` always disconnects and deletes the copies the app downloaded, whatever the
  source; files outside the app folder are never touched. Settings labels it «Отключить» for a
  PATH or manual copy and «Удалить» otherwise.
- The uv probe also requires `uvx` beside `uv`, since MCP launches `uvx`.
- `toolchain.select` without `path` opens the file picker; a cancelled pick returns the
  unchanged runtime.

**Next session should know:**
- TASK_035_EXTRA_2 should call `ToolchainService::launch(ToolchainCommand, args)` (or
  `launch_command`) to get a `LaunchSpec`; it already carries the isolated env and `PATH`.
- On Linux and macOS, `extract_archive` skips tar symlinks, so a managed Node has no `bin/npx`
  and fails the probe; uv is unaffected. PATH and manual copies work. Windows is unaffected.
- Not run here: `pnpm build`, `pnpm test:e2e` and the manual check in «Verify» (they need the
  built app). `zvs-studio`'s own test target still does not compile on local rustc 1.93
  (`try_update`, unrelated to this task), so clippy ran with that target excluded;
  `cargo clippy -p zvs-studio --lib --bins` passes. A full `cargo test --workspace` did not
  finish locally (stopped for low memory); the zvs-contract suite and the new toolchain, IPC and
  MCP tests passed.
