---
task: TASK_006
title: Data layer — Drizzle, SQLite, migration runner, repository base
phase: 0 — Foundation
status: done
depends_on: [TASK_002, TASK_004]
unblocks: [TASK_009]
estimate: 4 h
---

# TASK_006 — Data layer: Drizzle, SQLite, migration runner, repository base

## Context

The app boots and talks across IPC, but forgets everything on quit. This task adds
persistence — and, more importantly, the *access discipline* around it.

Two architectural rules from the passport govern this task and are non-negotiable:

**TypeScript owns SQLite, alone** (decision D2). Rust does heavy work — indexing, embedding,
vector search — but it never opens the database. It returns results across the boundary and
the host persists them through a repository. One writer, one schema owner, one migration tool.
When TASK_028 wires up indexing, this rule will feel inconvenient for about ten minutes; hold
it anyway, because two writers on one SQLite file with independent migration histories is a
class of bug that is miserable to diagnose.

**Only repositories touch Drizzle** (§8). Services orchestrate, repositories persist,
handlers stay thin. TASK_001 already wrote the lint rule pointing at
`src/host/data/repositories/**` — this task creates the directory that makes it bite.

The native module needs attention: `better-sqlite3` must be rebuilt against Electron's Node
ABI, not the system Node's. This is the single most common bootstrap failure in Electron
projects. Wire `electron-rebuild` into `postinstall` in this task so a fresh `pnpm install`
on any machine produces a working binary.

## Why

TASK_009 (the first real entity) and everything after it needs a database that migrates
deterministically at boot. Doing the boot gate properly now — refuse to run on a
half-migrated schema — avoids data corruption that would otherwise surface much later.

## Depends on

- `TASK_002` — `paths.ts` (for `dbPath`), logger, composition root with its startup gaps.
- `TASK_004` — error codes and the `Result` envelope for surfacing DB failures.

## Read first

- `/docs/PASSPORT.md` §9 (data layer — this task implements it), §8 (host layering, the
  repository rule, transactions belong to services), §2 decision D2.

## Scope

**In scope**
- `better-sqlite3` + Drizzle wired, with `electron-rebuild` in `postinstall`
- Connection setup: WAL, `foreign_keys = ON`, `busy_timeout`
- `drizzle-kit` configured; migrations generated to a committed folder
- Migration runner that executes **before any service is constructed**
- Pre-migration backup, keeping the last three
- Failure path: a failed migration blocks boot and shows a recovery screen
- `Repository` base with the intention-revealing-methods rule documented
- A `setting` table as the first real table, proving the whole chain
- Tests against an in-memory / temp-file database

**Out of scope**
- Feature tables — `secret` is TASK_009, everything else follows its own task
- Any Rust access to the DB, now or ever
- Sync, replication, encryption of the database file itself (individual secret *values* are
  encrypted in TASK_010; the file is not)

## Steps

1. Install `better-sqlite3`, `drizzle-orm`, `drizzle-kit`, `@electron/rebuild`. Pin current
   stable versions. Add `postinstall` running `electron-rebuild -f -w better-sqlite3`.
2. `host/data/client.ts` — open the database at `paths.dbPath()`. Set pragmas in this order:
   `journal_mode = WAL`, `foreign_keys = ON`, `busy_timeout = 5000`, `synchronous = NORMAL`.
   Export a single client instance created by the composition root, never a module-level
   singleton constructed on import (that breaks tests).
3. `host/data/schema/` — one file per table, plus a barrel. Create `setting.ts`:
   `key` (text, pk), `value` (text, JSON-encoded), `updatedAt` (integer, epoch ms).
4. `drizzle.config.ts` pointing at the schema barrel, output `host/data/migrations/`.
   Generate the first migration and **commit it**. Migrations are source, not artefacts.
5. `host/data/migrate.ts` — the runner:
   - Before applying anything, copy the DB file to `userData/backups/db-<timestamp>.sqlite`
     and prune to the last three. Skip the backup if the file does not exist yet.
   - Apply pending migrations inside a transaction where the migration allows it.
   - Log every applied migration name.
   - On failure: log fully, then throw a typed `MigrationFailedError`.
6. Composition root: call `migrate()` in the startup gap TASK_002 left, **before**
   constructing services and before creating the main window. On `MigrationFailedError`,
   open a minimal recovery window instead of the app — plain HTML is fine — showing the
   error, the backup folder path, and a "Quit" button. The app must never run against a
   half-migrated schema.
7. `host/data/repositories/Repository.ts` — a small base giving `db` access to subclasses
   and nothing else. Document the two rules in a comment at the top of the file:
   - Repositories expose intention-revealing methods (`findEnabledByCapability`), never a
     leaked query builder or a raw `where` passthrough.
   - Repositories return **entities** (row shapes). Mapping entity → DTO happens in a
     service, because the two shapes are allowed to diverge.
8. `host/data/repositories/SettingRepository.ts` — `get(key)`, `set(key, value)`,
   `all()`. First real repository; it is the template every later one copies.
9. Transactions: expose `withTransaction(fn)` on the client. Document that a **service**
   opens the transaction and passes the handle down, so one unit of work can span several
   repositories. Repositories never open their own.
10. Wire `settings.get` / `settings.set` channels through the contract so the chain
    renderer → IPC → service → repository → SQLite is proven end to end. Store the window
    size on quit and restore it on boot as the first genuine use.
11. Tests: migration runner applies cleanly to an empty file; a deliberately broken
    migration is caught and surfaces `MigrationFailedError`; `SettingRepository` round-trips;
    `withTransaction` rolls back on throw. Use a temp-file database, not the real one.
12. Commit: `TASK_006: data layer, migrations and repository base`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Where should the SQLite database live — the standard Electron userData folder, or a
> workspace folder the user picks so they can keep several separate workspaces or put it on a
> synced drive?"
>
> **Why it matters:** It changes `paths.ts`, whether the app needs a workspace picker at boot,
> and whether multiple databases can exist. Retrofitting multi-workspace support later touches
> every path call site.
> **Options:** (a) single database in userData — simplest, matches the passport as written;
> (b) user-chosen workspace folder with a picker on first run — supports several workspaces,
> adds a boot flow and a "recent workspaces" concept.
> **If the user is unavailable:** take (a), which is what PASSPORT §9 currently specifies.
> Note it in Handoff, and keep `paths.dbPath()` as the only call site so (b) stays cheap.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks:
- First run creates the database; the log shows migrations applied.
- Second run applies nothing and boots faster.
- Resize the window, quit, reopen — the size is restored (proves the full chain).
- Corrupt the database file deliberately (write junk into it) and confirm the recovery
  window appears rather than a crash or an empty app. Restore from the backup folder
  afterwards to confirm backups are real.
- Try importing the Drizzle client from a service file — `pnpm lint` must fail.

## Definition of done

- [x] Database created at the agreed location, WAL and foreign keys on
- [x] Migrations generated, committed, applied at boot before services start
- [x] Backup written before migrating; last three kept
- [x] Failed migration blocks boot with a recovery window
- [x] `SettingRepository` works through the full IPC chain
- [x] Lint forbids Drizzle access outside repositories (demonstrated)
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Decision answered:** (a) — a single database in `userData`. `paths.dbPath()` remains the only
call site, so a workspace picker stays cheap to add.

**Built:**

- `better-sqlite3@13.0.3`, `drizzle-orm@0.45.2`, `drizzle-kit@0.31.10`, `@electron/rebuild@4.2.0`
  pinned. `postinstall` is `install-electron && electron-rebuild -f -w better-sqlite3`, and
  `better-sqlite3` is added to `allowBuilds` in `pnpm-workspace.yaml`.
- `host/data/client.ts` — `openDatabase({ file })` sets `journal_mode = WAL`, `foreign_keys = ON`,
  `busy_timeout = 5000`, `synchronous = NORMAL` in that order, and closes the connection if any
  pragma throws. No module-level singleton; the composition root creates it.
- `host/data/types.ts` (`Database` / `Transaction` / `DatabaseHandle`) and
  `host/data/UnitOfWork.ts` — the port services depend on, so no service imports the client.
- `host/data/schema/setting.ts` + barrel; `drizzle.config.ts`; `migrations/0000_setting.sql` and
  its journal generated and committed.
- `host/data/migrate.ts` — `migrate()` reads the journal and `__drizzle_migrations` to list
  pending tags, backs the file up to `userData/backups/db-<timestamp>.sqlite` (last three kept),
  applies, logs each tag, and wraps any failure in `MigrationFailedError`. `prepareDatabase()`
  opens and migrates in one step so a corrupt file is a migration failure too.
- `host/data/repositories/Repository.ts` (base), `SettingRepository.ts` (`get` / `all` / `set` /
  `remove`), and `repositories/index.ts` binding every repository to one handle.
- `host/services/SettingService.ts` — entity → DTO mapping, `setMany` as one transaction.
- `settings.get` / `settings.set` on the contract, handlers in `host/ipc/settings.ts`.
- Composition root migrates before constructing any service and before creating the window; on
  `MigrationFailedError` it opens `createRecoveryWindow` (plain HTML, no preload, no renderer)
  showing the error, the backup folder and a Quit button, and returns.
- Window geometry stored under the `window.main` setting on close and restored on boot; the
  renderer reads the same setting over IPC, proving renderer → IPC → service → repository → SQLite.
- 28 tests in `apps/studio/tests` (pragmas, migrate-once/no-op, backup pruning, broken migration,
  missing journal, corrupt file, repository round-trip, transaction rollback, IPC chain, window
  state) plus extended boundary tests in `packages/eslint-config`.

**Deviated from plan:**

- **Tests now run under Electron, not the system Node.** `electron-rebuild` builds
  `better-sqlite3` for Electron's ABI (modules 149 vs the system Node's 147), so
  `node --test` could no longer load it. `apps/studio/scripts/run-tests.mjs` spawns the Electron
  binary with `ELECTRON_RUN_AS_NODE=1`; `pnpm --filter studio test` is unchanged from the caller's
  side. One pre-existing test (`system.demoStream`) had a fixed 100 ms sleep that is too short
  under Electron's coarser Windows timer; it now waits on `bus.open` instead.
- **No code comments** — the user asked for none this session. The two repository rules the task
  wanted documented at the top of `Repository.ts` live in `apps/studio/src/host/data/README.md`
  instead, together with the transaction rule and the migration/native-module notes.
- **Services reach persistence through `UnitOfWork`, not `DatabaseClient`.** The passport puts
  transactions in services, while the TASK_001 lint rule forbids services importing the client;
  the port satisfies both. `client.ts` is importable only from `data/`, `main.ts` and the tests,
  and a new `no-restricted-imports` rule blocks `drizzle-orm` / `better-sqlite3` anywhere in
  `apps/studio/src` outside `host/data/**`. Both are covered by `packages/eslint-config` tests.
- **`prepareDatabase()` was added** beyond the listed steps: corrupting the file makes the pragma
  throw at open, before `migrate()`, which otherwise crashed instead of showing the recovery
  window. Verified by corrupting the real database and booting.
- Backups are skipped when the database has no tables yet, not merely when the file is absent —
  `openDatabase` creates the file, so "does not exist" never held on a first run.
- `electron.vite.config.ts` copies `migrations/` to `out/host/migrations` on build, and
  `paths.migrationsDir` resolves next to the host entry, so dev, build and asar all agree.

**Next session should know:**

- `pnpm --filter studio db:generate --name=<change>` writes the next migration; commit the `.sql`
  and the updated `meta/`. `TASK_009` (`secret`) is the next table.
- Rust must never open the database (D2). Index metadata crosses the boundary and is persisted
  through a repository.
- A repository is constructed against a `DatabaseHandle`; register new ones in
  `data/repositories/index.ts` so they join `Repositories` and therefore transactions.
- Verified manually: first run applies `0000_setting` and logs it; a second run logs "Schema is up
  to date" and applies nothing; a window seeded at 1234 × 812 reopened at that size and wrote the
  same geometry back on close; a database overwritten with junk opens the recovery window instead
  of crashing; importing `../data/client` or `drizzle-orm` from a service fails `pnpm lint`.
- The backup folder is `%APPDATA%/ZVS AI Studio/backups`. It stays empty until the first migration
  that runs against a database that already has tables.
