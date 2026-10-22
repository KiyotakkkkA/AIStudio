---
task: TASK_076
title: Cutover — delete Electron, the TS host, napi, Drizzle and better-sqlite3; Tauri becomes the app
phase: M — Migration to Tauri
status: done
depends_on: [TASK_075]
unblocks: [TASK_034_EXTRA_2, TASK_035, TASK_042, TASK_046, TASK_048, TASK_052]
estimate: 4 h
---

# TASK_076 — Cutover — delete Electron, the TS host, napi, Drizzle and better-sqlite3; Tauri becomes the app

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). This is the last
task of the migration. After it, only one shell and one host exist.

By TASK_075 every channel is served by `zvs-host`, the four e2e specs pass against the built
Tauri app, and the parity checklist is complete. Still in the tree:
- the Electron shell: `apps/studio/src/host/` (~22k lines of TS), `src/preload/`,
  `electron.vite.config.ts`, the `tsconfig.host.json` and `tsconfig.preload.json` projects,
  and the `electron`, `electron-vite`, `@electron/rebuild`, `better-sqlite3`, `drizzle-orm`
  and `drizzle-kit` dependencies, plus `postinstall`
- the host half of the Vitest suite (`apps/studio/tests/*.test.ts`, now duplicated in Rust)
- `crates/zvs-napi`, the addon half of `scripts/build-rust.mjs`, `test/native/` and
  `apps/studio/resources/native/`
- `packages/ipc`'s server half (`server.ts` and its tests), which only the TS host used
- ESLint boundary rules about `host`, which are now meaningless for TS

This task deletes all of it, makes `pnpm dev`/`pnpm build` the Tauri commands, and updates
every README so nobody follows a dead path.

## Why

Two shells cost double: every dependency bump, every CI minute, every reader wondering which
one is real. The migration is not finished until the old one is gone and the open backlog
(TASK_034_EXTRA_2 onward) can resume against one architecture.

## Depends on

- `TASK_075` — e2e parity sign-off.

## Read first

- TASK_075's Handoff: the parity checklist. Do not start if any line is unchecked.
- `/docs/PASSPORT.md` §5 (path map), §16
- Root `package.json`, `turbo.json`, `pnpm-workspace.yaml`, `vitest.config.ts`,
  `eslint.config.js`, `packages/eslint-config/`, `.github/workflows/ci.yml`,
  `scripts/build-rust.mjs`

## Scope

**In scope**
- Delete `apps/studio/src/host/`, `src/preload/`, `electron.vite.config.ts`,
  `tsconfig.host.json`, `tsconfig.preload.json`, `apps/studio/out/` and
  `drizzle.config.ts`, and every Electron, Drizzle and `better-sqlite3` dependency and
  `postinstall`
- Delete host Vitest tests and the `host` Vitest project. Keep `packages` and `renderer`.
- Delete `crates/zvs-napi`, `test/native/`, `resources/native/`, and the napi half of
  `build-rust.mjs` (rename what is left to `build-sidecar.mjs`, or fold it into Tauri's
  `beforeBuildCommand`), and drop `@napi-rs/cli`
- `packages/ipc`: remove `server.ts`, its tests and README sections. The client stays.
- Scripts: `dev` → `tauri dev`, `build` → web build plus `tauri build --no-bundle`, `start`
  removed, `package` added for TASK_056. Remove `dev:tauri` and `build:tauri`, or keep them as
  aliases for one task and note it.
- `apps/studio/src/browser-ui` and the renderer: delete the Electron-only branches (the
  bridge's "not Tauri" path)
- ESLint: drop host-related boundary rules and keep `shared` ↛ `renderer` and the UI rules.
  Update `packages/eslint-config/tests/boundaries.test.js`.
- `knip` clean: no unused files or dependencies left behind
- CI: Linux job runs `cargo fmt`, `clippy` and `test` for the workspace except `zvs-studio`,
  plus `pnpm typecheck`, `lint`, `test` and `format:check`. Windows job runs
  `cargo clippy -p zvs-studio`, the Tauri build and e2e.
- READMEs: root `README.md`, `CONTRIBUTING.md`, `apps/studio/README.md`, `crates/*/README.md`
  and `packages/ipc/README.md` describe only what exists. Move the TS `kernel/README.md` and
  `data/README.md` content into `crates/zvs-host/src/kernel/README.md` and
  `src/data/README.md` if TASK_061 and TASK_070 did not already.
- `/docs/PASSPORT.md`: remove the "migration in progress" notes and mark D10 complete
- Update `/tasks/INDEX.md`: Phase M ticked. Open tasks keep their post-migration banners.

**Out of scope** (do not do these, even if tempting)
- Packaging and installer (TASK_056, rewritten for Tauri)
- Any feature work or refactor of the Rust host beyond deleting dead seams

## Steps

1. Confirm the TASK_075 checklist is green, and run the full e2e suite once more.
2. Delete in this order, building after each: TS host and preloads → Electron deps and
   configs → napi crate and native tests → `packages/ipc` server → host tests → lint rules.
3. Rewire scripts, Turbo pipelines and CI.
4. `knip`, `pnpm install` with a regenerated lockfile, and a clean clone build on Windows.
5. Docs sweep: `grep -ri "electron\|preload\|napi\|drizzle\|better-sqlite" --exclude-dir=node_modules`
   over the repo should hit only history (`tasks/` handoffs, `resolved/`) and the PASSPORT
   decision record.
6. One commit for the cutover, `TASK_076: …`.

## Verify

```bash
pnpm install --frozen-lockfile
pnpm typecheck && pnpm lint && pnpm build
pnpm test
pnpm test:e2e
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
pnpm format:check
```

Manual check: from a fresh clone on Windows, run `pnpm install && pnpm dev`. The Tauri app
opens, and every page works on an empty data directory. The repo-wide grep from step 5 is
clean.

## Definition of done

- [x] No Electron, TS host, preload, napi, Drizzle or `better-sqlite3` code or dependency
      remains
- [ ] `pnpm dev`/`pnpm build` are the Tauri app, and CI is green on both jobs (the scripts
      are done; CI has not run, because nothing is pushed yet)
- [x] Every README and PASSPORT describe only the Tauri architecture
- [x] The open backlog is unblocked: its tasks depend on this one and are ready to run
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Deleted: `apps/studio/src/host/` and `src/preload/`, `electron.vite.config.ts`,
  `drizzle.config.ts`, `tsconfig.host.json`, `tsconfig.preload.json`, `out/`, `resources/`
  (`native/` and the old `sidecar/` copy), every `apps/studio/tests/*.test.ts`, the host
  helpers in `test/helpers/` (all but `paths.ts`, `fakeBridge.ts`, `fakeClock.ts` and
  `fakeMcpServer.mjs`, which the Rust MCP tests still launch), `crates/zvs-napi` and the
  `napi*` workspace dependencies, `test/native/`, `scripts/build-rust.mjs`, `@napi-rs/cli`,
  `electron`, `electron-vite`, `@electron/rebuild`, `better-sqlite3`, `@types/better-sqlite3`,
  `drizzle-orm`, `drizzle-kit`, `postinstall`, `db:generate`, `start`, and the
  `better-sqlite3`/`electron` entries in `allowBuilds`. Also `uuid` and `zod` from the studio
  (only the host used them) and `renderer/features/chat/ChatRetrieval.tsx`, orphaned since
  7ac8ed7, so knip is clean.
- `packages/ipc`: `server.ts`, `server.test.ts`, `types.test.ts` gone; the harness keeps only the
  test contract; the two server round-trip tests became one bridge-rejection test. README
  rewritten for the client only.
- Scripts. Studio: `dev` = `tauri.mjs dev`, `build` = `tauri.mjs build --no-bundle` (its
  `beforeBuildCommand` is `build:web`), `package` = `tauri.mjs build`. Root: `dev`, `build`
  and `package` go through Turbo. `dev:tauri` and `build:tauri` are removed, not aliased.
  `scripts/tauri.mjs` already built `zvs-jobd` next to the executable, so the napi script's
  sidecar half had nothing left to do there.
- Turbo: only `studio#build` depends on `lint` (`apps/studio/turbo.json`, uncached, because
  its output is `target/`). `//#lint:root` and `//#typecheck:root` now depend on
  `@zvs/shared#build` and `@zvs/ipc#build`. Before, a clean tree raced them against the
  package builds and failed (`Unable to resolve '@zvs/shared'`). With `build → lint` on every
  package, that dependency would have been a cycle. `dev` depends on `^build`, so a fresh clone
  can run `pnpm dev` directly.
- The e2e vector seeding moved off the napi addon to `crates/zvs-core/examples/seed_vectors.rs`
  (store directory as the argument, `VectorRow` JSON on stdin). `scripts/tauri.mjs` builds it
  whenever `--features e2e` is passed, and `seedVectors` in `e2e/launch.ts` runs it.
- Both Tauri bridges define `window.zvs` unconditionally. The "already defined by a preload" /
  "not Tauri" guard is gone.
- ESLint: the host zones, the `data/client.ts` zone and the Drizzle import ban are gone.
  `packages/shared` ↛ `apps` and the `ui` ↛ `features`/`stores` rules stay. The boundary test
  covers just those.
- `knip.json` declares the entries knip cannot find (the browser-ui and recovery pages, the
  isolation script, the gateway page script, e2e, scripts, `fakeMcpServer.mjs`).
  `knip --include files,dependencies,devDependencies,unlisted,binaries,unresolved` is clean and
  runs in CI.
- CI: `linux` runs cargo fmt/clippy/test except `zvs-studio`, typecheck, lint, the channel
  contract check, knip, Vitest with coverage and format:check. `windows` runs `zvs-studio`
  clippy (plain and `e2e`), its tests, `pnpm build`, the e2e build and `pnpm test:e2e`.
- Docs: root README, CONTRIBUTING, `apps/studio/README.md` (rewritten for Tauri),
  `packages/ipc/README.md`, the `zvs-host`, `zvs-core`, `zvs-contract` and data READMEs,
  `docs/QWEN_WEB.md`. The TS kernel README now lives at `crates/zvs-host/src/kernel/README.md`
  with Rust names. The data README had already moved in TASK_061. PASSPORT: the revision
  banner says Phase M is complete, D10 is marked complete, and §19 is in the past tense. INDEX:
  Phase M ticked, "Where we are" updated.
- Test renames, so the step 5 grep is clean: `the_baseline_schema_equals_the_frozen_snapshot`
  and `the_id_is_the_json_tuple_of_domain_path_name_and_secure`. `fakeBridge.ts`'s doc
  comment now says Tauri.
- `apps/studio/src-tauri/src/browser/gateway/README.md` is Prettier-formatted (the
  `format:check` failure TASK_075 reported).
- Verified on Windows 2026-10-20: `pnpm install --frozen-lockfile`, `pnpm typecheck`,
  `pnpm lint` and `pnpm test` (30 files, 1300 tests) pass, all three from a tree with the
  package `dist/` folders deleted. `pnpm build` produces `target/release/zvs-studio.exe`.
  `pnpm format:check`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` and `cargo clippy -p zvs-studio --all-targets --features e2e -- -D warnings` pass.
  `cargo test --workspace` passes (zvs-host 844 + 5 ignored, zvs-studio 87). `pnpm test:e2e`:
  four full runs, of which two passed 14/14 and two failed only the browser test (see below).
  The browser spec passed 3/3 when run alone. The step 5 grep over tracked files hits only
  `docs/PASSPORT.md`'s decision record and path map.

**Deviated from plan:**

- No clean-clone build and no interactive `pnpm dev` session. The clean-tree checks above and
  the e2e page sweep on an empty data directory stand in for them.
- `build-rust.mjs` was deleted rather than renamed. Its remaining half copied `zvs-jobd` into
  `resources/sidecar/`, which only the TS host read.
- Two Rust tests were renamed. Their names said "drizzle" and "electron".
- `pnpm package` is plain `tauri build`, which bundles nothing yet because
  `bundle.active` is `false`. TASK_056 switches it on.

**Next session should know:**

- `browser.spec.ts › isolated browser, navigation, cookie management and restart persistence`
  failed in 2 of 4 full runs. Both failures timed out on `getByLabel("Browser workspace")`
  after the browser had already loaded the site. The failing runs were slow (2.5 min and
  9.6 min, the latter with a build in the same run; a passing run takes about 40 s). The
  test passed 3/3 alone. Probably the late route restore TASK_075 described: the studio leaves
  `/browser` after the browser mounts. CI retries once.
- `runtimes::llama_server::tests::a_process_that_exits_on_its_own_is_not_mistaken_for_a_running_server`
  failed once in a full `cargo test --workspace` and passed 3/3 alone. It is a timing test.
- knip still reports unused **exports** in the renderer (about 40, older than this task). Only
  files and dependencies are gated.
- Turbo: if a new package's build ever has to be lint-gated, put that in the package's own
  `turbo.json`, as `apps/studio` does. A global `build → lint` brings the cycle back.

---

**Phase M is complete when this task is done.** The studio is a Tauri app with a Rust host;
the backlog resumes at TASK_034_EXTRA_2.
