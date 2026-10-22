---
task: TASK_009
title: Secret schema, repository and first feature migration
phase: 1 — First vertical slice: Secrets
status: done
depends_on: [TASK_008]
unblocks: [TASK_010]
estimate: 2 h
---

# TASK_009 — Secret schema, repository and first feature migration

## Context

Phase 0 produced a working shell with no features. Secrets is the first real one, chosen
deliberately: it is the smallest complete vertical slice (table → repository → service → DTO →
channel → page), and nothing else can be built without it, because every AI provider, MCP
server and mail account references a stored credential rather than holding its own.

The domain model that the design implies (`/design/Main.dc.html`): a secret has a **type**
(`ollama-cloud`, `openrouter`, `anthropic`, `mistral`, `qdrant`, `mail`, `custom`), a display
name, a **scope** (`personal` / `shared` / `public`), an encrypted value, free-form metadata
(tags, note), and rotation tracking. The list shows a masked value, the scope, how many
providers use it and when it was updated.

The split that matters for this task: **metadata is plaintext, the value is ciphertext.** The
Secrets page lists, filters and sorts secrets without ever decrypting one. Only the moment a
provider actually makes a request does a value get decrypted, inside a host service. That
split has to exist in the schema itself, which is why it is decided here rather than in
TASK_010.

## Why

Everything downstream references a secret id. Getting the table shape and the
plaintext/ciphertext split right now avoids a migration through data that is, by construction,
awkward to migrate.

## Depends on

- `TASK_008` — full test harness, `tempDb` helper.
- (transitively) `TASK_006` — Drizzle, migration runner, `Repository` base.

## Read first

- `/design/Main.dc.html` — the Secrets page. Read the list rows and the form's field groups;
  they tell you what the entity carries.
- `/docs/PASSPORT.md` §9 (data layer, aggregate list, secrets at rest), §8 (repository rules).

## Scope

**In scope**
- `secret` table, plus `secret_usage` for the "used by N providers" count
- Drizzle schema, generated migration, committed
- `SecretRepository` with intention-revealing methods
- Repository tests against a temp database

**Out of scope**
- Encryption itself (TASK_010) — store an opaque `bytes` column now and leave it empty
- DTOs and channels (TASK_011)
- UI (TASK_012)
- The `identity` aggregate — the page's "Identity & Profile" tab is TASK_012's stub

## Steps

1. `host/data/schema/secret.ts`:
   - `id` text pk (UUID v7 from `platform/ids.ts`)
   - `type` text, not null — the secret-type key, e.g. `ollama-cloud`
   - `name` text, not null — display name
   - `scope` text, not null — `personal` | `shared` | `public`
   - `cipher` blob, nullable — the encrypted value, written by TASK_010
   - `cipherVersion` integer, not null, default 1 — so a future re-encryption can migrate
   - `hint` text, nullable — the masked tail shown in the list (`…4f2a`), plaintext by design
   - `fields` text, not null — JSON of the non-secret schema fields (base URL, org, …)
   - `tags` text, not null, default `'[]'` — JSON array
   - `note` text, nullable
   - `rotationDays` integer, nullable; `rotatesAt` integer, nullable (epoch ms)
   - `createdAt`, `updatedAt` integer, not null
   - Indices: `type`, `scope`, and a unique index on `(name, type)` so duplicates are caught
     at the database rather than in the UI.
2. `host/data/schema/secretUsage.ts` — `secretId` fk, `consumerKind`
   (`provider` | `mcp_server` | `integration` | `vector_store`), `consumerId`, unique on all
   three. This is how "used by 2 providers" is computed without scanning every table, and how
   deletion can refuse to orphan a live consumer.
3. Foreign key from `secret_usage.secretId` to `secret.id` with `ON DELETE RESTRICT` — not
   cascade. Deleting a secret that something still uses must fail loudly and tell the user
   what is using it, rather than silently breaking a provider.
4. Generate the migration with `drizzle-kit`, review the SQL by eye, commit it.
5. `host/data/repositories/SecretRepository.ts`:
   - `listSummaries(filter?: { scope?, type?, query? })` — returns rows **without** `cipher`.
     Make this the default read path so a decrypted value cannot leak by accident.
   - `findById(id)`, `findCipherById(id)` — the second is the only method that returns
     ciphertext, and its name says so.
   - `create(entity)`, `update(id, patch)`, `remove(id)`
   - `countUsage(id)`, `listUsage(id)`, `addUsage(...)`, `removeUsage(...)`
   - `listRotatingBefore(ts)` — for the amber "rotates in 12 d" state on the page.
6. Tests against `tempDb`: create/read/update/delete; unique constraint on `(name, type)`
   fires; `listSummaries` never returns `cipher`; deleting a secret with usage is rejected;
   usage counting is correct across consumer kinds.
7. Commit: `TASK_009: secret schema and repository`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "When a secret is deleted but something still uses it — say a provider — should the app
> refuse the delete and list what depends on it, or allow it and mark those consumers broken?"
>
> **Why it matters:** It is the foreign-key rule in this migration (`RESTRICT` vs `SET NULL`)
> and changing it later means a migration plus reworking the provider health logic.
> **Options:** (a) refuse and explain — safest, the user resolves dependencies first;
> (b) allow and degrade — the provider shows "credential missing" and stops working until
> repointed.
> **If the user is unavailable:** take (a), `ON DELETE RESTRICT`, as written in step 3. Note
> it in Handoff.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: launch the app, confirm the migration applies once and the log names it. Open
the database with any SQLite browser and confirm the `secret` table shape matches the schema
file, including the indices.

## Definition of done

- [x] `secret` and `secret_usage` tables exist with the documented columns and indices
- [x] Migration generated, reviewed, committed, applies cleanly on a fresh database
- [x] `SecretRepository` exposes intention-revealing methods; only one returns ciphertext
- [x] `listSummaries` provably cannot leak `cipher` (test asserts it)
- [x] Deleting a used secret is rejected with a clear error
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- **`host/data/schema/secret.ts`** — the `secret` table exactly as specified: `id` text pk,
  `type`, `name`, `scope`, `cipher` blob (nullable, still empty — TASK_010 writes it),
  `cipher_version` default 1, `hint`, `fields`, `tags`, `note`, `rotation_days`, `rotates_at`,
  `created_at`, `updated_at`. Indices on `type` and `scope`, unique on `(name, type)`.
  Exports `SECRET_TYPES`, `SECRET_SCOPES`, `SecretEntity`, `SecretInsert`,
  `SecretSummary` (`Omit<SecretEntity, "cipher">`) and `SecretCipher`.
- **`host/data/schema/secretUsage.ts`** — `secret_usage` with a composite primary key on
  `(secret_id, consumer_kind, consumer_id)`, plus `created_at`. FK to `secret.id` with
  `ON DELETE RESTRICT ON UPDATE CASCADE`. `SECRET_CONSUMER_KINDS` is
  `provider | mcp_server | integration | vector_store`.
- **`migrations/0001_secret.sql`** — generated by `drizzle-kit`, reviewed, committed. Applies
  after `0000_setting` on a fresh database and is a no-op on a second run; the runner logs both
  tags.
- **`host/data/repositories/SecretRepository.ts`** — `listSummaries(filter?)`, `findById`,
  `findCipherById`, `listRotatingBefore`, `create`, `update`, `remove`, `countUsage`,
  `listUsage`, `addUsage`, `removeUsage`. Every read path except `findCipherById` selects an
  explicit column list that omits `cipher`, including the `returning(...)` of `create` and
  `update`, so a decrypted-value leak is structurally impossible rather than a convention.
  `remove` reads usage first and throws `AppError(CONFLICT)` whose `details` name every
  consumer; the FK is the backstop under it. Registered in `repositories/index.ts` as
  `repositories.secrets`.
- **`apps/studio/tests/secrets.test.ts`** — eight tests: CRUD round trip, the `(name, type)`
  unique constraint firing (and the same name under a different type being allowed), no read
  path returning `cipher` while `findCipherById` does, delete refused with the consumer list
  plus the raw FK refusing a bypass, usage counted across all four consumer kinds with
  `addUsage` being idempotent, `listSummaries` filtering by scope/type/query (including a
  literal `%` matching nothing), `listRotatingBefore` ordering, and a rolled-back unit of work.

**Deviated from plan:**

- **Secret types are `ollama`, `openrouter`, `mistral`, `custom`** — the user cut `anthropic`,
  `qdrant` and `mail` from the task text's list and said it may be extended later. `type` is
  therefore a plain `text` column, not a constrained union: `SECRET_TYPES` is a TypeScript
  const list, so adding a type later needs no migration. `scope` and `consumer_kind` are
  `$type<...>()`-narrowed because those lists are closed.
- **DECISION answered: (a) refuse and explain.** `ON DELETE RESTRICT`, as written in step 3.
- **`secret_usage` has no surrogate `id`.** The task asked for uniqueness on all three columns;
  a composite primary key gives that and is also the index `countUsage`/`listUsage` need, so a
  separate id column would have carried no information.
- **`fields` defaults to `'{}'`** (the task only specified a default for `tags`). It keeps
  `create` callable without a fields payload for secret types that have none.
- **`data.test.ts` updated** — three assertions listed `["0000_setting"]` as the applied set and
  now list `0001_secret` too, and the backup test drops the new tables along with `setting`
  before re-running the migrations.

**Next session should know:**

- `cipher` is `blob("cipher", { mode: "buffer" })`, so TASK_010 hands `safeStorage` output
  straight in as a `Buffer` and reads it back as one. `cipherVersion` is written but never yet
  varied; bump it when the encryption scheme changes and migrate on read.
- `findCipherById` is the *only* method that returns ciphertext, by name and by test. Keep that
  property: if a new read method is added, select from `SUMMARY_COLUMNS`.
- `remove` throws `AppError(CONFLICT)` with `details.consumers` — an array of
  `{ kind, id }`. TASK_011's channel can map that straight into the copy the page shows.
- `listSummaries({ query })` matches `name` only, case-insensitively, with `%`/`_`/`!` escaped
  under `ESCAPE '!'`. If the page needs tag or hint search, widen it there rather than in the
  caller.
- Nothing populates `secret_usage` yet — providers (TASK_02x) call `addUsage`/`removeUsage`
  when they bind or unbind a credential. Until then every secret reports zero usage and is
  deletable.
- The `identity` aggregate is untouched, as scoped. Only `secret` and `secret_usage` landed.
