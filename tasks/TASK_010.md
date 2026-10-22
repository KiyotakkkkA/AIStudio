---
task: TASK_010
title: Crypto service (safeStorage) and SecretService
phase: 1 — First vertical slice: Secrets
status: done
depends_on: [TASK_009]
unblocks: [TASK_011]
estimate: 3 h
---

# TASK_010 — Crypto service (`safeStorage`) and SecretService

## Context

TASK_009 created a `secret` table with an empty `cipher` column. This task fills it, and
builds the service that is the only code in the application allowed to see a decrypted
credential.

The encryption approach is Electron's `safeStorage`, which delegates to the OS keychain —
DPAPI on Windows, Keychain on macOS, libsecret on Linux. The app never handles a master
password and never rolls its own crypto. The consequence, which is worth being explicit about
because it will surprise someone later: **secrets are bound to the OS user account.** Copy the
database to another machine and the metadata survives while the values do not. PASSPORT §18
assumption 4 records this, and the decision block below is the moment to confirm it.

The service rules are absolute and should be enforced by structure, not vigilance:

- A decrypted value never appears in a DTO. Not once, not "just for the form".
- A decrypted value is never logged, not even at debug level, not even truncated.
- Decryption happens at point of use, inside a host service, and the plaintext does not
  outlive the call.
- The renderer never receives a secret value. The Secrets page shows a masked hint that was
  computed at write time and stored in plaintext deliberately.

The design mockup reflects this: `/design/Main.dc.html` shows the API key field as
`osk_live_••••••••••••4f2a` with an eye icon. That eye reveals nothing from the store — it is
a separate, deliberate, logged action (the decision below covers whether it exists at all).

## Why

Every provider, MCP server and mail account resolves a credential through this service. If the
leak-prevention rules are not structural now, a value will end up in a log or a DTO within ten
tasks, and finding it will be luck.

## Depends on

- `TASK_009` — `SecretRepository`, including `findCipherById`.

## Read first

- `/docs/PASSPORT.md` §9 ("Secrets at rest"), §14 (security), §8 (services hold all logic).
- `/design/Main.dc.html` — the masked value, the eye and copy icons, the footer note about
  encryption at rest.

## Scope

**In scope**
- `CryptoService` wrapping `safeStorage` with a clear failure mode when unavailable
- `SecretService`: create, update, list, delete, and `resolve(id)` for internal use
- Hint computation (the masked tail) at write time
- Rotation status derivation (`ok` / `due-soon` / `overdue`)
- Usage tracking helpers that other features will call
- Tests, including a fake crypto backend so tests do not touch the real keychain

**Out of scope**
- DTOs and channels (TASK_011) — this service is host-internal for now
- UI (TASK_012)
- Key rotation of the encryption key itself; `cipherVersion` exists for that, unused for now

## Steps

1. `host/services/CryptoService.ts`:
   - `isAvailable()` → `safeStorage.isEncryptionAvailable()`.
   - `encrypt(plaintext: string): Buffer`, `decrypt(buf: Buffer): string`.
   - If encryption is unavailable, **fail loudly**: throw `AppError(SECRET_DECRYPT_FAILED)`
     with a message explaining the OS keychain is not accessible. Do not fall back to
     storing plaintext, ever, under any circumstance.
   - Constructor takes the backend so tests can inject a fake.
2. `host/services/SecretService.ts`, constructed in the composition root with
   `SecretRepository`, `CryptoService`, the clock and the logger:
   - `create(input)` — encrypt the value, compute the hint, persist, return a summary
     entity with **no** value.
   - `update(id, patch)` — re-encrypt only if a new value was supplied; leaving the value
     untouched must be the common path, since editing a name should not require retyping a
     key.
   - `list(filter)` — summaries only, straight from `listSummaries`.
   - `remove(id)` — check usage first; on conflict throw `AppError(CONFLICT)` carrying the
     list of consumers so the UI can name them.
   - `resolve(id): Promise<string>` — **host-internal only.** Decrypts and returns the
     plaintext. Add a doc comment stating that callers must not store, log or forward the
     result, and that it must never be reachable from an IPC handler.
3. Hint computation: keep the last four characters of the value, prefix with the type's
   conventional prefix if the value has one (`sk-or-v1-…9c31`). Never store more than four
   characters. Put this in one function so the rule has a single home.
4. Rotation: derive status from `rotatesAt` and the injected clock — `overdue` if past,
   `due-soon` within 14 days, else `ok`. Pure function, easy to test.
5. Usage: `addUsage(secretId, kind, consumerId)` and `removeUsage(...)`, wrapped in the
   service so consumers do not touch the repository directly.
6. Leak prevention, structurally:
   - Give the service's return type a name like `SecretSummary` that has no value field, so
     returning a plaintext is a type error rather than a review catch.
   - Add a redaction helper in the logger that strips anything matching known key shapes,
     and unit-test it. Belt and braces — the primary defence is that values never reach a
     logger call in the first place.
7. Tests (fake crypto backend, temp database):
   - Round-trip: create then `resolve` returns the original value.
   - `list` and `create` return objects with no value field (assert at runtime too, not just
     by type).
   - Update without a new value leaves the ciphertext byte-identical.
   - Delete with usage throws `CONFLICT` and names consumers.
   - Rotation status boundaries at exactly 14 days and at expiry, using the fake clock.
   - Unavailable encryption throws rather than degrading.
   - The log redactor catches a realistic key string.
8. Commit: `TASK_010: crypto service and secret service`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Two things about secret values. First: should the app ever reveal a stored secret back to
> you in the UI — the eye icon in the mockup — or is a stored value write-only, where you can
> replace it but never read it back? Second: `safeStorage` ties secrets to your OS user
> account, so copying the workspace to another machine loses the values while keeping the
> names. Is that acceptable, or do you want a passphrase-derived key so workspaces are
> portable?"
>
> **Why it matters:** Reveal is an IPC channel that returns plaintext to the renderer — the one
> deliberate exception to the rule above, and it needs an audit log entry if it exists.
> Portability changes the entire key-management design and cannot be retrofitted cheaply.
> **Options:** reveal — (a) write-only, no reveal channel, safest; (b) reveal with a
> confirmation and an audit log entry. Portability — (c) `safeStorage`, machine-bound, as
> the passport currently says; (d) passphrase-derived key, portable, but the user must enter a
> passphrase at every launch and losing it loses the secrets.
> **If the user is unavailable:** take (a) and (c). Build no reveal channel; drop the eye icon
> from the form in TASK_012 and note it in Handoff.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks:
- Create a secret through a temporary host-side script or a test, then inspect the database
  with a SQLite browser: the `cipher` column must be unreadable bytes, and `hint`, `name` and
  `fields` readable.
- Grep the log files after a full test run for a known test secret value. Zero hits.

## Definition of done

- [x] Values encrypted via `safeStorage`; unavailable keychain fails loudly, never degrades
- [x] `SecretSummary` type makes returning a plaintext a type error
- [x] `resolve()` documented as host-internal and unreachable from any handler
- [x] Hint stores at most four characters
- [x] Rotation status derived from an injected clock, boundaries tested
- [x] Delete blocked by usage, with consumers named
- [x] No secret value appears in any log (verified by grep)
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Decision answered by the user:** (a) write-only — no reveal channel, ever — and (c)
`safeStorage`, machine-bound, no passphrase. PASSPORT §18 assumption 4 now records both as
confirmed. TASK_012 must drop the eye icon from the secret form; only the copy-of-hint and
replace-value affordances remain.

**Built:**
- `host/services/CryptoService.ts` — a thin wrapper over an injected `CryptoBackend`
  (`isEncryptionAvailable` / `encryptString` / `decryptString`, the exact shape of Electron's
  `safeStorage`, which the composition root passes in directly). Every call re-checks
  availability and throws `AppError(SECRET_DECRYPT_FAILED)` when the keychain is not there.
  There is no plaintext fallback path in the code at all.
- `host/services/secretPolicy.ts` — the pure rules: `computeHint`, `rotationStatus`,
  `rotationDueAt`, plus `KNOWN_VALUE_PREFIXES` and `DUE_SOON_WINDOW_MS`. Hint keeps the last
  four characters and prepends a conventional prefix only when the value is long enough that
  prefix and tail cannot overlap (`sk-or-v1-…9c31`, otherwise `…9c31`).
- `host/services/SecretService.ts` — `list`, `create`, `update`, `remove`, `usage`,
  `addUsage`, `removeUsage`, `resolve`. Built with `UnitOfWork`, `CryptoService`, an optional
  `Logger` and an injected clock, exactly like `SettingService`.
- `platform/logger.ts` gained `redactSecrets` and `REDACTED`. Every `log()` call now runs its
  message and its fields through the redactor before serialising: key shapes for OpenAI,
  OpenRouter, Anthropic, Ollama, GitHub, Slack, JWTs, AWS access keys and `Bearer` headers,
  plus Buffers, which are replaced wholesale.
- `test/helpers/fakeCrypto.ts` — a reversible XOR stand-in with `available` / `failNext`
  switches, so no test touches the real keychain.
- `apps/studio/tests/secretService.test.ts` — 9 tests covering the round trip, the absence of
  a value on every read path (asserted at runtime with `Object.hasOwn`), byte-identical
  ciphertext on a metadata-only update, the `CONFLICT` on delete with named consumers, the
  rotation boundaries at exactly 14 days and at expiry, the unavailable and failing keychain,
  the hint rules, the redactor, and a full service run whose log file is then read back and
  checked for the test value.
- `host/main.ts` constructs `CryptoService(safeStorage)` and `SecretService` after the
  database is ready and logs whether encryption is available.

**Deviated from plan:**
- The service-level summary type is `SecretSummary` in `SecretService.ts`, which collides by
  name with the repository row type from `schema/secret.ts`; the service imports the latter as
  `SecretRow`. The service type is richer than the row: `fields` and `tags` are decoded, and a
  derived `rotation` status is added, so TASK_011's DTO mapping is close to a pass-through.
- `SecretConsumer` is a `type` alias rather than an `interface`, because `ErrorDetails` is
  JSON-shaped and an interface has no implicit index signature.
- Added `usage(id)` beyond the listed methods — `remove` needs it to name consumers, and
  TASK_012 needs it to render the "used by" line.
- `rotationDays` now derives `rotatesAt` when the caller does not pass one. It was otherwise
  dead metadata.
- No `get(id)` on the service. TASK_011 will want one; it was not in scope here.

**Next session should know:**
- `resolve()` is the only method that returns a plaintext and it must never be wired into a
  channel. TASK_011 should register `secrets.list`, `secrets.create`, `secrets.update` and
  `secrets.remove` only. If a `secrets.reveal` channel ever appears in a diff, that is the
  decision above being reversed and needs asking again.
- `create` takes `value` and `update` takes an optional `value`; omitting it is the common
  path and leaves the cipher column untouched, which the test asserts byte for byte.
- `cipherVersion` is written as `1` and never read for branching yet. It is the hook for
  rotating the encryption key itself, still out of scope.
- Errors the DTO layer will need to map: `NOT_FOUND`, `CONFLICT` (details carry
  `{ secretId, consumers }`), `VALIDATION_FAILED`, `SECRET_MISSING`, `SECRET_DECRYPT_FAILED`,
  `DB_ERROR`.
- The redactor is a second line of defence only. The primary rule stands: a value never
  reaches a logger call.
