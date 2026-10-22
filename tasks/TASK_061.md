---
task: TASK_061
title: Rust data layer — rusqlite, migration runner, baseline schema, settings, window state, recovery window
phase: M — Migration to Tauri
status: done
depends_on: [TASK_060]
unblocks: [TASK_062]
estimate: 4 h
---

# TASK_061 — Rust data layer — rusqlite, migration runner, baseline schema, settings, window state, recovery window

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Today SQLite belongs to the TypeScript host (old D2). `better-sqlite3` is rebuilt for
Electron's ABI. Drizzle provides the schema (`host/data/schema/*.ts`) and the queries.
`drizzle-kit` generates migrations `0000`–`0021`, and `host/data/migrate.ts` backs up,
applies, and throws `MigrationFailedError` so `main.ts` opens a recovery window instead of the
app. Repositories are the only code that touches queries. Services open transactions through
`UnitOfWork` (see `host/data/README.md`).

Under D10, D2 becomes **"Rust host only"**. There is still one writer, one schema owner and
one migration runner, but now in `zvs-host::data`. Because the Tauri build starts with no data,
the twenty-two Drizzle migrations are **squashed into one baseline** that equals the current
schema exactly. Later migrations are hand-written SQL files after it. This task builds the
layer, the full baseline, and the first aggregate end to end: `setting`, with `settings.get`,
`settings.set`, theme application and window-state persistence. Every other repository is
ported by the task that ports its domain.

## Why

Every later port needs a migrated database, a transaction API and a repository template. The
baseline must be exact now, because every domain port after this one assumes its tables
already exist as the TS host created them.

## Depends on

- `TASK_060` — `zvs-host`, paths, logger and the IPC server.

## Read first

- `/docs/PASSPORT.md` §8 (transactions belong to services), §9 (data layer, now Rust)
- `apps/studio/src/host/data/README.md`, `client.ts`, `migrate.ts`, `MigrationFailedError.ts`,
  `UnitOfWork.ts`, `types.ts`
- `apps/studio/src/host/data/repositories/SettingRepository.ts`: the template
- `apps/studio/src/host/services/SettingService.ts`, `platform/theme.ts`,
  `platform/windowState.ts`, `platform/windows.ts` (`createRecoveryWindow`)
- `apps/studio/tests/data.test.ts`, `theme.test.ts`, `test/helpers/tempDb.ts`

## Scope

**In scope**
- `rusqlite` with the `bundled` feature, pinned. One connection opened with the pragmas from
  `client.ts`: `journal_mode = WAL`, `foreign_keys = ON`, `busy_timeout = 5000`,
  `synchronous = NORMAL`.
- `Database` owns the connection behind a mutex. `db.transaction(|tx| …)` is the unit of work:
  every repository called inside receives the same `&Transaction`, and an `Err` or a panic
  rolls back. **No lock is ever held across an `.await`.** Enable the clippy lint
  `await_holding_lock` in the crate.
- `crates/zvs-host/migrations/0000_baseline.sql`: the exact schema produced by applying
  Drizzle `0000`–`0021` to an empty database, covering tables, indices, triggers and foreign
  keys
- Migration runner. Migrations are embedded with `include_str!` in an ordered list, and a
  `__migrations(id, name, checksum, applied_at)` journal records them. An applied migration
  whose checksum changed is a hard failure. Before applying, the runner backs up to
  `backups/db-<timestamp>.sqlite`, keeps the last three, and skips the backup for an empty
  database. Each migration applies in its own transaction. A failure returns
  `MigrationFailed { message, backups_dir }`.
- The recovery window in `zvs-studio`: on `MigrationFailed`, open a small window from a
  bundled `recovery.html` page (part of the Vite build, no inline script, CSP-clean) instead
  of the studio, showing the message and the backups path, with a Quit button. No services
  are constructed. This reproduces `createRecoveryWindow`.
- `data::schema::setting` row type, `SettingRepository`, `SettingService`, and the channels
  `settings.get` and `settings.set` with their DTOs and fixtures (the TASK_059 recipe)
- Theme: `applyThemeSource` becomes a `ThemePort` implemented in `zvs-studio` with Tauri's
  window theme API (`system` / `light` / `dark`)
- Window state: geometry and the maximised flag are restored on open and captured on close
  through `SettingService`, with the same key and validation as `windowState.ts`
- Test helper `temporary_database()`: a migrated database in a temporary directory, with
  disposal on drop. Every repository test from now on uses it.

**Out of scope** (do not do these, even if tempting)
- Any repository other than `SettingRepository`. Each domain task ports its own.
- An ORM or query builder. Repositories write SQL by hand behind intention-revealing methods
  (§8 still applies).
- Importing an Electron database (fresh start, decided)

## Steps

1. Generate the baseline. Use the still-present TS host: apply `0000`–`0021` to an empty file
   with the existing runner, then dump `sqlite_master` in creation order. Commit the SQL and
   remove nothing from the TS side. Add a test that applies the baseline and compares
   `sqlite_master` against a checked-in snapshot.
2. `Database::open(path, logger)` with the pragmas, and a test that reads each pragma back.
3. The migration runner, ported from `migrate.ts`, with the backup policy. Test applying,
   skipping already-applied migrations, a checksum change, a failure that leaves the database
   untouched, and backup rotation.
4. `SettingRepository` as the Rust template: row struct, `get`, `set` and `list` taking
   `&Connection` or `&Transaction`, no SQL outside the repository, and tests on
   `temporary_database()`.
5. `SettingService` and the `settings.*` channels and DTOs, with fixtures and parity tests from
   the TS suite.
6. Theme and window state through ports, wired in the composition root.
7. The recovery window: force a failing migration in a test build and confirm the window
   appears and the studio does not.
8. Update `crates/zvs-host/src/data/README.md` (new) with the rules from the TS data README,
   restated for Rust.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm build:tauri
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: `pnpm dev:tauri` creates `studio.sqlite` under the Tauri data directory. Change
the theme on the Settings page, restart, and confirm it persists. Resize and maximise the
window, restart, and confirm the geometry is restored. Corrupt the baseline checksum in a
scratch build and confirm the recovery window opens.

## Definition of done

- [x] The baseline schema equals the Drizzle schema, guarded by a snapshot test
- [x] The runner checks checksums, backs up keeping three, and never runs on a half-migrated
      database
- [x] The recovery window replaces the studio on migration failure
- [x] `settings.*`, theme and window state work under Tauri
- [x] `temporary_database()` exists and is the only way tests open SQLite
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `rusqlite =0.40.2` (`bundled`) is pinned in `[workspace.dependencies]`. `zvs-host` also takes
  `sha2` for checksums. `lib.rs` denies `clippy::await_holding_lock`.
- `data::Database`: one connection behind a `std::sync::Mutex`, opened with the `client.ts`
  pragmas.
  - `with_connection(|c| …)` and `transaction(|tx| …)` take closures that return
    `Result<T, DataError>` (`App`, `Sqlite` or `Io`), so `?` works on both error kinds.
  - An `Err` or a panic rolls the transaction back, and a poisoned mutex is recovered.
  - SQLite and IO errors are logged as `data` / `Database operation failed` and become
    `UNKNOWN` / `Внутренняя ошибка`. That is the envelope a raw Drizzle error produced in the
    TS host.
- The baseline, `crates/zvs-host/migrations/0000_baseline.sql`, is the `sqlite_master` dump
  (in creation order) after the frozen TS runner applied `0000`–`0021`. It holds 25 tables and
  32 indices; the Drizzle schema has no triggers. The dump itself is stored as
  `migrations/snapshots/0000_baseline.json`, and two tests guard it:
  - A Rust test applies the baseline and compares `sqlite_master` to the snapshot.
  - A new Vitest case in `apps/studio/tests/data.test.ts` compares the Drizzle result to the
    same file.
- `data::migrate`, the migration runner:
  - `MIGRATIONS` embeds the SQL with `include_str!`. The journal is
    `__migrations(id, name, checksum, applied_at)`.
  - The checksum is SHA-256 of the SQL with CRLF normalised to LF. A changed checksum is a
    hard failure.
  - Before migrating a populated database, it backs up to `backups/db-<ISO stamp>.sqlite` and
    keeps three backups.
  - Each migration runs in its own transaction together with its journal row.
  - `prepare_database` turns open and migrate failures into
    `MigrationFailed { message, backups_dir }`. `From` maps that to `DB_ERROR` with a
    `backupsDir` detail. `prepare_with` takes a custom migration list.
- `schema::SettingRow` and `repositories::{Repositories, SettingRepository}`, with `get`,
  `list`, `set` (an upsert with `RETURNING`) and `remove`. The repository takes `&Connection`,
  and `&Transaction` derefs to it.
- `services::SettingService`: `get`, `list`, `set`, `set_many` (one transaction) and
  `remove`. A corrupt stored value is a `DB_ERROR` naming the key. `Services` now also holds
  `settings` and `theme`.
- `zvs-contract::settings`:
  - The DTOs `SettingsGetInput/Output` and `SettingsSetInput/Output`.
  - `check_setting_key`, a hand-written equivalent of the `SettingKey` regex.
  - Eight fixture files in `packages/shared/fixtures/settings`, registered in both fixture
    tables.
- `ipc/settings.rs`: `settings.get` answers a missing key with `{ key }`. `settings.set`
  mirrors `ui.theme` to the `ThemePort`.
- Platform helpers:
  - `platform::theme`: `ThemePort`, `ThemeSource`, `apply_theme_source`.
  - `platform::window_state`: `read_window_state` and `capture_window_state`, with the same
    key, clamping and default as `windowState.ts`.
  - `paths::backup_file_path` and `is_backup_file`.
- `zvs-studio`:
  - `compose` returns `Boot::Studio` or `Boot::Recovery`. On `MigrationFailed` no services
    are built and `window::create_recovery` opens `recovery/index.html`.
  - `TauriTheme` calls `AppHandle::set_theme`.
  - The theme and window state are read from settings before the window is built. The studio
    window restores its size, position and maximised state, and saves the window state when
    it is asked to close.
  - `Host` now carries `services`.
- The recovery page, `apps/studio/src/recovery/{index.html,main.ts,style.css}`:
  - It is a third Vite input, with `tsconfig.recovery.json`, and is added to the ESLint
    client zone.
  - There is no inline script. The message and backups path come in the query string (built
    with `Url::query_pairs_mut`) and are written via `textContent`.
  - Quit is a plain link to `/recovery/quit`. `on_navigation` intercepts it and calls
    `app.exit(0)`, so the page needs no IPC and no capability. Closing the window also ends
    the app.
- Test helpers in `testing`:
  - `temporary_database()`, `unmigrated_database()` and `temporary_database_with(…)`.
  - `TemporaryDatabase`, which derefs to `Database` and is deleted on drop.
  - `test_services(system)`, which returns `TestServices` with a `RecordingTheme`.
- Docs: `crates/zvs-host/src/data/README.md` is new, and the `zvs-host` and `zvs-contract`
  READMEs are updated.

**Deviated from plan:**
- There are no source comments, at the user's request.
- Backups use `VACUUM INTO`, not a file copy. In WAL mode a plain copy of `studio.sqlite` can
  miss committed pages that are still in `-wal`.
- Old backups are pruned by file name, which is an ISO stamp from the injected clock, not by
  mtime. That keeps the order deterministic.
- The baseline text is byte-identical to Drizzle's output, including the backticks and the
  `ALTER TABLE … ADD` tails, so `sqlite_master` matches exactly.
- The checksum failure message is in Russian because the recovery window shows it.
- Window geometry has no `getNormalBounds` in Tauri, so the last normal geometry is tracked
  instead: the logical outer position and logical inner size, on `Moved` and `Resized`.
  - tao 0.37.1 emits both events before it sets its `MAXIMIZED` flag.
  - `run_on_main_thread` runs inline when it is called on the main thread.
  - Because of both, the sample is posted from the async runtime so it runs after the flag is
    set. Without this, maximizing overwrote the normal bounds; this was seen live and fixed.
  - `x` and `y` are rounded to integers when the state is read.
- `settings.set` does not restart the health check for `health.check.*` keys, because
  `HealthCheckService` is not ported yet. The providers port must add it.
- `SettingDto` is an alias of `SettingsSetOutput`, which has the same shape, rather than a
  separate host type.

**Next session should know:**
- Repository recipe:
  1. Add a row struct in `data/schema`.
  2. Add the repository in `data/repositories` and a field on `Repositories`.
  3. Write SQL by hand.
  4. Test on `temporary_database()`.
- Never edit `0000_baseline.sql`; its checksum is journaled on every install. For a schema
  change, write `0001_*.sql` and append it to `MIGRATIONS`.
- Tests that build `Services` use `test_services(...)` and must bind it before the
  `IpcServer` (`let (_host, server) = …`). That way the connection closes before the temp
  directory is removed.
- Verified live with the release exe and `pnpm dev:tauri`, over WebView2 CDP and Win32:
  - A fresh `studio.sqlite` gets the baseline, and the next launch logs
    `Schema is up to date`.
  - `settings.get` and `settings.set` round-trip, and a bad key returns `VALIDATION_FAILED`.
  - Setting the theme to `light` flips `prefers-color-scheme` at once, and the choice
    survives a restart.
  - A moved and resized window comes back pixel-exact. A maximized window restarts maximized
    and restores to its earlier normal rect. Closing while minimized keeps the normal rect.
  - With a tampered journal checksum, only the recovery window opens. It shows the message
    and the path, its styles load under the production CSP, and it has no `window.zvs`. Quit
    exits the process. The checksum was restored afterwards.
- Gates:
  - `cargo clippy --workspace --all-targets -- -D warnings`: clean.
  - `cargo test --workspace -j 2`: 195 passed, 0 failed.
  - `node scripts/contract-channels.mjs --check` and `pnpm build:tauri`: green.
  - Per-workspace `build`, `typecheck` and `lint`, plus `typecheck:root` and `lint:root`:
    green.
- Through turbo, `pnpm typecheck` and `pnpm build` failed only at `//#build:rust`, with
  `EBUSY` copying `zvs-core.node`. An Electron process that was already running before this
  session had the file loaded. Running the same scripts per workspace passed.
- Full `vitest run`: 979 passed. Two tests failed with `EPERM` while deleting a temp
  directory, in the frozen TS `secrets.test.ts` and `mcp.test.ts`. Both pass on re-run, so it
  is a Windows flake and was not caused by this task.
