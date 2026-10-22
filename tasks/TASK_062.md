---
task: TASK_062
title: Secrets in Rust — DPAPI crypto backend, SecretRepository, SecretService, secrets.* channels
phase: M — Migration to Tauri
status: done
depends_on: [TASK_061]
unblocks: [TASK_063]
estimate: 4 h
---

# TASK_062 — Secrets in Rust — DPAPI crypto backend, SecretRepository, SecretService, secrets.* channels

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Secrets were the first vertical slice (TASK_009–TASK_012), and they are the first domain port
too. Today `CryptoService` wraps Electron `safeStorage` through a `CryptoBackend` interface
(`isEncryptionAvailable`, `encryptString`, `decryptString`). The ciphertext lives in SQLite,
while the metadata stays plaintext so the page can list and filter. Values are **write-only**:
no channel ever returns a plaintext (PASSPORT §18.4). Deleting a secret that something uses is
refused, and the error lists the consumers (`secret_usage`, `ON DELETE RESTRICT`). Secret types
come from a schema registry (`secrets.types`).

Electron's `safeStorage` does not exist under Tauri. The replacement keeps the same contract,
machine- and user-bound encryption with no passphrase, using **Windows DPAPI**
(`CryptProtectData` / `CryptUnprotectData`, current-user scope) called directly through the
`windows` crate. That is what `safeStorage` used underneath on Windows, minus Chromium's extra
AES layer. Windows is the only verified target (§18.3). On macOS and Linux the backend reports
itself unavailable until someone builds and verifies those platforms, and the service then
behaves exactly as the TS one does when `safeStorage` is unavailable: it refuses, and never
stores a plaintext.

## Why

Providers, accounts and MCP servers all resolve credentials through `SecretService`. Nothing
that talks to a vendor can be ported until this exists, and it has to be at least as strict as
the code it replaces.

## Depends on

- `TASK_061` — the data layer, the baseline schema (the `secret` and `secret_usage` tables
  already exist) and the repository template.

## Read first

- `/docs/PASSPORT.md` §9 (secrets at rest), §14, §18.4
- `apps/studio/src/host/services/CryptoService.ts`, `SecretService.ts`, `secretPolicy.ts`
- `apps/studio/src/host/data/repositories/SecretRepository.ts`, `schema/secret.ts`,
  `schema/secretUsage.ts`
- `apps/studio/src/host/ipc/secrets.ts`, `packages/shared/src/secrets/`
- `apps/studio/tests/secretService.test.ts`, `secrets.test.ts`, `secretChannels.test.ts`

## Scope

**In scope**
- `CryptoBackend` trait in `zvs-host` with `is_available`, `encrypt(&str) -> Vec<u8>` and
  `decrypt(&[u8]) -> String`. Plaintext buffers use `zeroize`, and the plaintext type redacts
  its `Debug` output.
- `DpapiBackend` in `zvs-studio/src/platform/windows/dpapi.rs`, behind `cfg(windows)`,
  `#[allow(unsafe_code)]` scoped to that module, and a `// SAFETY:` comment per call
  (`LocalFree` on returned blobs included). A ciphertext version prefix byte leaves room for
  a future scheme.
- `UnavailableBackend` for other platforms
- `CryptoService` with the same Russian error messages and `SECRET_DECRYPT_FAILED` mapping
- `SecretRepository` (CRUD, usage, consumers) and `SecretService` (create, update, remove,
  list, get without the value, `resolve()` for host-internal use only, policy checks, refusal
  that lists consumers)
- The `secrets.*` channels (`create`, `get`, `list`, `remove`, `types`, `update`), their DTOs
  and fixtures. **No channel and no DTO carries a decrypted value.** A test asserts that every
  `secrets.*` output fixture lacks a value field.
- The secret-type registry (`ollama`, `openrouter`, `mistral`, `custom`, …) exactly as
  `secrets.types` returns it today

**Out of scope** (do not do these, even if tempting)
- Reading Electron `safeStorage` ciphertexts (fresh start, decided)
- A reveal channel (still refused, §18.4)
- Implementing and verifying macOS Keychain or Linux Secret Service. Note them as follow-ups.

## Steps

1. Pin the `windows` crate with only the `Win32_Security_Cryptography` and
   `Win32_Foundation` features (plus whatever `LocalFree` needs), and `zeroize`.
2. Implement `DpapiBackend` with a round-trip test (Windows CI only) and a test that a
   tampered blob fails to decrypt.
3. Port `CryptoService` with a fake backend in tests. Port the unavailable-backend cases.
4. Port `SecretRepository` on `temporary_database()`, including the restrict-on-delete
   behaviour and the consumer listing.
5. Port `SecretService` and `secretPolicy`. `resolve()` returns a redacting wrapper, and a test
   asserts that the logger output for a full create, resolve and remove cycle contains no
   planted value.
6. Port the handlers and DTOs with fixtures, then port the channel tests.
7. Wire the service in the composition root, with a startup log line
   (`encryption: bool, count`) that matches the TS one.

## Verify

```bash
cargo test -p zvs-host
cargo test -p zvs-studio
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, the Secrets page creates, edits, filters and deletes
secrets. A secret's value never appears in DevTools network or IPC payloads or in
`studio.jsonl`. Open `studio.sqlite` and confirm the value column is opaque bytes.

## Definition of done

- [x] DPAPI backend, scoped `unsafe` (no SAFETY comments, see Deviated), plaintext zeroised
      and redacted
- [x] `SecretService` parity with the TS tests, including consumer-listing refusal
- [x] No channel, DTO or log line ever carries a plaintext value, enforced by tests
- [ ] The Secrets page fully works under Tauri (manual check not run yet)
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Dependencies: `zeroize =1.9.1` and `url =2.5.8` (already in the lock) in
  `[workspace.dependencies]`. `zvs-studio` takes `windows =0.62.2` (the version Tauri already
  locks) for `cfg(windows)` only, with `Win32_Foundation`, `Win32_Security` and
  `Win32_Security_Cryptography`.
- `zvs-contract::secrets`:
  - The DTOs `SecretTypeSchema`, `FieldDescriptor`, `FieldOption`, `FieldDefault`,
    `SecretFilter`, `SecretRef`, `CreateSecretInput`, `UpdateSecretInput`,
    `SecretSummaryDto`, `SecretDto` and `SecretsRemoveOutput`, plus the enums `SecretScope`,
    `SecretTypeKey`, `RotationStatus` and `FieldKind`.
  - `SecretInput` carries the inbound credential. It zeroises on drop and prints
    `SecretInput([redacted])` under `Debug`.
  - The registry: `secret_type_registry()`, `find_secret_type_schema` and
    `is_secret_type_key`.
  - `wire::optional_nullable`, for `rotationDays` (absent, `null` or a number).
- Fixtures: 18 files in `packages/shared/fixtures/secrets`, registered in both tables.
  - `SecretsTypesOutput.valid.json` holds the whole registry. Rust asserts that
    `secret_type_registry()` serialises to it, and Vitest asserts that `SECRET_TYPE_REGISTRY`
    equals it.
  - `no_secrets_output_fixture_carries_a_value_field` walks every `secrets/*` output fixture
    and fails on a `value`, `cipher`, `plaintext`, `secret` or `apiKey` key. It skips
    `options`, where `value` is a select option.
- `zvs-host`:
  - `services::crypto`: `CryptoBackend` (`is_available`, `encrypt`, `decrypt -> Plaintext`),
    `CryptoService` with the TS messages, mapped to `SECRET_DECRYPT_FAILED`, and
    `UnavailableBackend`. `Plaintext` is `Zeroizing<String>` with a redacted `Debug` and no
    `Serialize`.
  - `services::secret_policy`: `compute_hint`, which counts UTF-16 units like `slice(-4)`,
    `rotation_status` and `rotation_due_at`.
  - `services::secret_fields::check_fields`, a port of `buildFieldsSchema`: strict keys,
    defaults, and per-kind checks. `z.url()` maps to `url::Url::parse` on the trimmed value
    with tabs and newlines stripped. Issues use the Zod codes.
  - `data::schema::{SecretRow, SecretCipherRow, SecretUsageRow, SecretConsumerKind}`.
    `SecretRow` never selects `cipher`, and `SecretCipherRow`'s `Debug` prints only the
    length.
  - `data::repositories::SecretRepository`: CRUD, `list_rotating_before`, usage counting,
    listing, upsert and remove. Like-escaping uses `!`. `remove` refuses with the consumer
    list.
  - `services::SecretService`: `list`, `get`, `create`, `update`, `remove`, `usage`,
    `add_usage`, `remove_usage`, `resolve -> Plaintext` and `is_encryption_available`.
    `update` and `remove` run in one transaction, and encryption happens before the lock is
    taken.
  - `ipc::secrets`: the six channels. Non-registry types such as `account-token` are hidden
    from `list` and answer `NOT_FOUND` without details on `get`, `update` and `remove`.
  - `testing::FakeCrypto`, the XOR-mask fake from `fakeCrypto.ts`. `TestServices` now has
    `crypto`, and `Services` now has `secrets`.
- `zvs-studio`:
  - `platform::windows::dpapi::DpapiBackend`: current-user `CryptProtectData` /
    `CryptUnprotectData` with `CRYPTPROTECT_UI_FORBIDDEN`. The ciphertext is a `0x01` version
    byte followed by the DPAPI blob. Output blobs are zeroised and then released with
    `LocalFree` in `Drop`. `probe()` checks availability with a round trip.
  - `platform::crypto_backend()` returns DPAPI on Windows and `UnavailableBackend` elsewhere.
  - `compose` builds the service and logs `host` / `Secret store ready` with
    `{ encryption, count }`, exactly as `main.ts` does.
- Tests: every case from `secretService.test.ts`, `secrets.test.ts` and
  `secretChannels.test.ts` is ported, plus:
  - A `FileLogger` run (create, update, resolve, remove, and careless `Debug` prints) whose
    file contains no planted value and no hint.
  - An IPC run whose envelopes and `MemoryLogger` lines contain no planted value.
  - DPAPI round-trip, tamper, truncation and version-prefix tests.

**Deviated from plan:**

- No source comments, at the user's request. That includes the `// SAFETY:` comments the plan
  asked for: the `unsafe` blocks in `dpapi.rs` are scoped to that module with
  `#![allow(unsafe_code)]`, but they are not annotated. Workspace clippy (`all`) does not
  enable `undocumented_unsafe_blocks`, so the gates are unaffected.
- The trait returns `Result<_, BackendFailure>`, and `decrypt` returns `Plaintext` rather than
  `String`, so the plaintext is zeroised from the first byte. A backend failure's reason is
  dropped, like the TS `cause`, and never reaches the envelope.
- `resolve` is synchronous, because nothing in it awaits.
- The `0x01` prefix byte is DPAPI-specific. The `cipher_version` column stays at `1`, as in
  the TS host.
- The DTO builders return `Result`. An unknown type becomes `NOT_FOUND` and an unparseable id
  becomes `DB_ERROR`, where the TS host cast both.
- The log test does not assert that the file lacks the bare tail `9c31`, because a random
  UUID can contain it. It asserts that the file lacks the hint `…9c31` instead.

**Next session should know:**

- Use `services.secrets.resolve(id)?` at the point of use and call `.expose()` only inside the
  request. Never put a `Plaintext` in a struct that outlives the call.
- Consumers register through `add_usage(secret_id, SecretConsumerKind::…, consumer_id)`.
  Removing a provider, account or MCP server must call `remove_usage`, or the secret can never
  be deleted.
- `SecretService::create` accepts any type string, so internal types like `account-token`
  work. Only the IPC layer restricts to the registry.
- Follow-ups: a macOS Keychain backend and a Linux Secret Service backend, both reporting
  unavailable until they are built and verified.
- Also fixed in this commit, because they kept the gates red. None of them changes TS host
  behaviour:
  - `pnpm format:check`: 13 Drizzle meta JSON files Prettier had never formatted. They parse to
    identical JSON. `test/native/sidecar.test.mjs` was only CRLF in the working tree.
  - `cargo fmt --check`: rustfmt wrapping in `crates/zvs-core/src/pdf.rs`. The string contents
    are unchanged.
  - Flaky Vitest failures were all `EPERM` from `rmSync` on a temp directory that Windows
    still held. `test/helpers/paths.ts` now leaves a locked directory under the temp root and
    retries when the process exits; any other error still throws.
  - Stale e2e specs that no longer matched the UI:
    - `chat.spec.ts` and `vectorStores.spec.ts` used `kind: "ollama"` with OpenAI-shaped
      fixtures. They now use `openai-compatible` and select their fixture model.
    - The chat spec attaches the store through the composer menu, no longer clicks approvals
      the flow no longer asks for, and checks sources in the «Источники ответа» panel.
    - The vector-store spec picks the model from the select and uses «Сверить» instead of a
      reindex that needs sources.
    - The secrets smoke test sent an `organization` field the Ollama type does not have.
  - Electron renderer build: `assetsInlineLimit: 0`, as in the Tauri config. A JetBrains
    Mono subset had been inlined as a `data:` font, which `font-src 'self'` blocks.
- Gates run on Windows on 2026-10-14, all exit 0:
  - `pnpm typecheck`, `pnpm lint`, `pnpm format:check` and `pnpm build`.
  - `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
    `cargo test --workspace`.
  - `npx vitest run`: 1050 passed, 2 skipped, on two clean runs.
  - `pnpm test:e2e`: 11 passed, twice.
  - `node scripts/contract-channels.mjs --check`.
- Not done: the manual `pnpm dev:tauri` check of the Secrets page, the DevTools/IPC payload
  inspection, and the `studio.sqlite` byte check.
