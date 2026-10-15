# Data layer

The Rust host owns SQLite, alone (PASSPORT D2 as revised by D10). One writer, one schema
owner, one migration runner. The frozen TypeScript layer in `apps/studio/src/host/data` is the
specification this module reproduces until TASK_076 deletes it.

## Layout

| Path                          | Role                                                                                                |
| ----------------------------- | --------------------------------------------------------------------------------------------------- |
| `database.rs`                 | `Database`: one `rusqlite` connection behind a mutex, the pragmas, `with_connection`, `transaction` |
| `migrate.rs`                  | `MIGRATIONS`, the runner, backups, `MigrationFailed`, `prepare_database`                            |
| `schema/`                     | One row struct per table (`SettingRow`), with `COLUMNS` and `from_row`                              |
| `repositories/`               | The only code that writes SQL. `Repositories::new(&connection)` hands out every repository          |
| `../../migrations/*.sql`      | Embedded with `include_str!`. `0000_baseline.sql` is Drizzle `0000`–`0021` squashed                 |
| `../../migrations/snapshots/` | `sqlite_master` after the baseline, as the TypeScript host produced it                              |

The connection is opened with `journal_mode = WAL`, `foreign_keys = ON`,
`busy_timeout = 5000` and `synchronous = NORMAL`, the same pragmas as `client.ts`.

## Rules for repositories

1. **Intention-revealing methods only.** `find_enabled_by_capability`, never a leaked
   statement and never a raw `WHERE` passthrough. SQL stays inside the repository, so the set
   of ways a table can be read stays finite and reviewable. There is no ORM and no query
   builder.
2. **Repositories return rows**, the structs in `schema/`. Mapping a row to a DTO happens in a
   service, because the two shapes are allowed to diverge.
3. **Repositories take `&Connection`** and return `rusqlite::Result`. A `&Transaction` derefs
   to `&Connection`, so the same repository runs inside or outside a unit of work.
4. **Repositories never open a transaction.** A service calls
   `database.transaction(|tx| …)` and builds `Repositories::new(tx)`; every repository inside
   shares that transaction, so one operation can span several aggregates. An `Err` or a panic
   rolls the whole unit back.
5. Prefer `prepare_cached` for statements that run more than once.

`SettingRepository` is the template every later repository copies.

`ProviderRepository::create`, `update` and `remove` keep `secret_usage` in step, and
`ModelRepository::replace_for_provider` deletes, reinserts and may clear the default model.
Each writes more than one row, so a service calls them inside `database.transaction`. The
auth-mode integrity rule is checked before the first write, so a refused row leaves nothing
behind even outside a transaction. `provider.adapter` and `provider.auth_mode` stay `String` in
`ProviderRow`, so an unknown value reaches the registry and fails there with the TS message.

`0001_graph_relation_seed` inserts the four starter relations Drizzle `0015` seeded
(`INSERT OR IGNORE`, same ids). The baseline snapshot holds only `sqlite_master`, so the rows
had to come back as data. `VectorStoreRepository::update` writes only the columns a
`VectorStorePatch` sets, `updated_at` included, as the Drizzle `update` did.

## Errors

Closures passed to `with_connection` and `transaction` return `Result<T, DataError>`, so both
`AppError` and `rusqlite::Error` work with `?`. On the way out an `AppError` passes through
unchanged. A SQLite or I/O error is logged at `error` (`data`, `Database operation failed`)
and becomes `UNKNOWN` / `Внутренняя ошибка`, the same envelope a raw Drizzle error produced in
the TypeScript host, with the detail kept host-side.

## Concurrency

There is one connection behind a `std::sync::Mutex`. Every database call is synchronous and
finishes before it returns. **No lock is ever held across an `.await`.** The crate denies
`clippy::await_holding_lock`. A poisoned mutex is recovered, because a panic inside
`transaction` has already rolled back.

## Migrations

`MIGRATIONS` is an ordered list of `Migration { id, name, sql }`. Ids ascend and names are
unique; the runner refuses a list that breaks either rule. The journal is
`__migrations(id, name, checksum, applied_at)`, and the checksum is SHA-256 of the SQL with
`\r\n` normalised to `\n`, so a Windows checkout does not change it.

`prepare_database` runs in the composition root **before any service is constructed**:

1. Open the file. A file that is not a database fails here as `MigrationFailed`.
2. Read the journal. An applied migration whose checksum changed is a hard failure.
3. With nothing pending, log `Schema is up to date` and stop.
4. If the database already has tables, back it up with `VACUUM INTO` to
   `backups/db-<timestamp>.sqlite` and keep the newest three. An empty database is not backed up.
5. Apply each pending migration in its own transaction, together with its journal row, and
   log `Applied migration`. A failure rolls that migration back, logs `Migration failed`
   and returns `MigrationFailed { message, backups_dir }`.

On `MigrationFailed`, `zvs-studio` opens the recovery window (`recovery/index.html`) instead
of the studio. It shows the message and the backups path and has a Quit link. No services are
constructed, so the app never runs against a half-migrated schema.

### Adding a migration

1. Write `migrations/NNNN_<change>.sql` by hand. Never edit an applied file: its checksum is
   recorded, and changing it stops every installed copy at startup.
2. Append it to `MIGRATIONS` with the next id.
3. DDL inside a transaction is atomic in SQLite, but `PRAGMA foreign_keys` is a no-op there.
   A table rebuild must keep foreign keys valid without switching them off.
4. Add or update the repository and its tests. `cargo test -p zvs-host` must stay green.

The baseline is frozen. `the_baseline_schema_equals_the_drizzle_schema_snapshot` (Rust) and
`the Rust baseline snapshot equals the schema the Drizzle migrations produce` (Vitest) pin it
to the Drizzle output from both sides while the TypeScript host still exists.

## Tests

`testing::temporary_database()` is the only way a test opens SQLite: a migrated database in a
temporary directory, deleted on drop. `unmigrated_database()` skips the migrations, and
`temporary_database_with` takes a logger. `testing::test_services(system)` builds `Services`
on a temporary database with a recording theme port. Declare it before the `IpcServer` that
uses it, so the server is dropped first and the file is closed before the directory goes.
