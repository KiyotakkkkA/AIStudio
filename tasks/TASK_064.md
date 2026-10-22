---
task: TASK_064
title: Providers in Rust — ProviderService, probe outcomes, HealthCheckService, providers.* channels
phase: M — Migration to Tauri
status: done
depends_on: [TASK_063]
unblocks: [TASK_065]
estimate: 3 h
---

# TASK_064 — Providers in Rust — ProviderService, probe outcomes, HealthCheckService, providers.* channels

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

TASK_063 ported the drivers. This task ports the service layer above them and lights up the AI
Providers page under Tauri. `ProviderService.ts` covers create, update, remove, list and get,
default models, and probing. A probe hits the model-list endpoint, or the identity endpoint
first in account mode, persists discovered models, returns latency, and reports a **union**
outcome (`probeOutcome.ts`) that includes `account-not-linked` and `session-expired`.
`providerStatus.ts` derives the card status. `HealthCheckService.ts` runs background probing:
opt-in, off by default (the TASK_015 decision). The `providers.*` channels are `adapters`,
`create`, `get`, `list`, `probe`, `refreshAll`, `remove`, `setDefaultModel` and `update`.

## Why

This is the "talks to a model" milestone under Tauri: a user can configure a provider and
discover models before chat exists.

## Depends on

- `TASK_063` — drivers, registry, provider and model repositories.

## Read first

- `/docs/PASSPORT.md` §12 (test connection, outcome union)
- `apps/studio/src/host/services/ProviderService.ts`, `probeOutcome.ts`, `providerStatus.ts`,
  `HealthCheckService.ts`
- `apps/studio/src/host/ipc/providers.ts`, `packages/shared/src/providers/`, `ai.ts`
- `apps/studio/tests/providerService.test.ts`, `providers.test.ts`

## Scope

**In scope**
- `ProviderService`, the probe outcome union, provider status derivation
- `HealthCheckService` on a `tokio` interval with an injected clock, honouring the setting, a
  `start`/`stop`/`dispose` lifecycle, and events on the bus exactly as today
- The `providers.*` channels, DTOs and fixtures
- Graceful shutdown: the composition root stops health checks and disposes providers in the
  same order as `main.ts`'s `before-quit`. Implement this as a `ShutdownCoordinator` in
  `zvs-host` that `zvs-studio` calls from `RunEvent::ExitRequested`, and that later tasks
  extend.

**Out of scope** (do not do these, even if tempting)
- Account-mode linking (TASK_067). Account-mode providers report `account-not-linked` until
  then.
- Changing probe semantics or the default health-check policy

## Steps

1. Port `probeOutcome` and `providerStatus` as pure functions with their tests.
2. Port `ProviderService` with its test suite against fake drivers and `temporary_database()`.
3. Port `HealthCheckService` with fake-clock tests (interval, opt-in, stop mid-probe).
4. Channels, DTOs and fixtures.
5. `ShutdownCoordinator`, wired to Tauri's exit event. Exit is deferred until disposal
   finishes, with a hard cap, as `before-quit` does today.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, add an Ollama provider and an OpenRouter provider with a
stored secret. Probe both and see models discovered and latency shown. Set a default model.
Break the API key and see the right outcome. Enable health checks and watch statuses update.
Quit and confirm in the log that shutdown ran in order.

## Definition of done

- [x] Service, outcome union, status derivation and health checks are ported with their tests
- [ ] The AI Providers page (API and Local tabs) fully works under Tauri (manual check not run
      yet; every `providers.*` channel is served and tested through `IpcServer::dispatch`)
- [x] The ordered graceful shutdown exists and is extensible
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract`:
  - `ai`: `HonoursDto`, `AdapterCapabilitiesDto`, `AdapterDescriptorDto` and
    `ProvidersAdaptersOutput`.
  - `providers`: `ProviderSettingsDto`, `DiscoveredModelDto`, `ModelDto`,
    `ProviderSummaryDto`, `ProviderDto`, `ProviderConnectionInput`, `CreateProviderInput`,
    `UpdateProviderInput`, `ProviderListFilter`, `ProviderRef`, `ProvidersGetInput`,
    `SetDefaultModelInput`, `ProvidersRemoveOutput`, `ProbeOutcomeDto` (internally tagged on
    `kind`), `ProbeRequest` (untagged, `{ id }` before `{ draft }`, as the Zod union) and
    `ProbeResultDto`. The Zod defaults (`adapter`, `authMode`, `secretId`, `accountId`,
    `settings`, `enabled`) follow the README rule: an `Option` plus an accessor.
  - `validate`: `check::trimmed_name`, `check::url` (WHATWG parse of the trimmed value, then
    `max` on the value with tabs and newlines stripped, as Zod 4.5 does), `js_trim` and
    `normalize_url`. The crate now depends on `url`.
- Fixtures: 18 new names (36 files) in `packages/shared/fixtures/providers` and
  `packages/shared/fixtures/ai`, registered in both the Rust and the Vitest table.
- `zvs-host`:
  - `data::schema::AccountRow` and `data::repositories::AccountRepository::find_by_id`, a
    read-only slice of the `account` table, on `Repositories::accounts`.
  - `services::probe_outcome`: `ProbeOutcome`, `discovery_outcome`, `failure_outcome`
    (`retryAfter` rounded like `Math.round`), `to_dto`.
  - `services::provider_status`: `derive_status`, `derive_status_within`, `days_until`,
    `CredentialLifetime`, with the TS copy.
  - `services::providers`: `ProviderService` (`list`, `get`, `create`, `update`, `remove`,
    `set_default_model`, `models`, `is_probing`, `probe`, `refresh_all`, `dispose`), the
    `DriverSource` port (implemented by `ProviderRegistry`), `ProbeInput`, `ProbeResult`,
    `adapter_descriptors`, `capabilities_dto`, `summary_dto`, `model_dto`.
  - `services::health_check`: `HealthCheckService` (`start`, `stop`, `dispose`, `run_once`,
    `running`, `enabled`, `interval_minutes`) and the setting keys.
  - `platform::shutdown`: `ShutdownCoordinator`, `ShutdownStage::{Stop, Drain, Dispose}`,
    `ExitDecision`, `DEFAULT_SHUTDOWN_CAP` (5 s).
  - `ipc::providers`: all nine channels. `settings.set` on a health-check key restarts the
    check, as `createSettingHandlers` does.
  - `Services` gains `providers` and `health`; `secrets` is now `Arc<SecretService>` and
    `SettingService` is `Clone`.
  - `testing::fake_driver`: `FakeDriver` (models, failure, delay, manual hold with
    `when_holding` / `release`) and `FakeDriverSource` (swappable driver, a stalled factory).
    `TestServices` has `drivers`.
- `zvs-studio`: `compose` builds the registry (`HttpClient::new()`, `DatabaseProviders`,
  `FileSystemModelStore` on `downloads_dir`, the host logger), `ProviderService`,
  `HealthCheckService` on Tauri's tokio handle, and the coordinator, then starts the health
  check. `run` now goes through `build(…).run(on_run_event)`: `ExitRequested` prevents the exit
  once, runs the coordinator and calls `AppHandle::exit(0)`; `Exit` logs `Shutdown` and
  disposes the event bus.
- Tests: 34 new in `zvs-host` (262 in total, 4 ignored). Ported from
  `providerService.test.ts`: every case except the account-credential draft (see below), plus
  abort, per-provider timeout, dispose, CRUD normalisation, `selectedOnly`, the adapter
  catalogue, the channel tests, the settings restart and the coordinator.
- Docs: `crates/zvs-host/README.md` (Providers, Shutdown, module table) and
  `crates/zvs-contract/README.md`.

**Deviated from plan:**

- No source comments, at the user's request.
- The tests live in `services/providers/tests.rs` and `services/health_check/tests.rs`, not
  inline, because the files would otherwise pass 1 000 lines.
- A saved probe is a spawned tokio task joined through a `Shared` future, so it runs whether or
  not the caller polls it, like a JS promise. A draft probe is a plain future and only runs when
  awaited; every caller awaits it at once.
- Cancellation is one shutdown token per service with a child per probe, instead of a set of
  `AbortController`s. A caller's signal cancels its probe with `RUN_CANCELLED`, the same code
  `signal.throwIfAborted()` led to in the health check, where it ends the sweep as `failed`.
- An account-mode row whose account is not linked reports `session-expired` before any driver
  is built, as in TS. Because `AccountRepository` is read-only until TASK_067, the account
  tests insert rows with SQL.
- Stored ids, adapter and auth-mode strings that do not parse become `DB_ERROR`
  «Провайдер хранится в повреждённом виде» in the DTO mappers, where the TS host cast them.
- `create`, `update`, `remove` and `set_default_model` each run in a transaction (the README
  rule from TASK_063). The TS service called the repositories directly.
- The shutdown cap is new: `before-quit` had none. A stuck task is abandoned after 5 s and the
  exit goes ahead, with a `warn` line.
- Not ported: "account draft credentials verify identity without changing the database". It
  needs `probeAccountCredentials`, which arrives with account linking (TASK_067).

**Next session should know:**

- `accounts.list` is served by `ipc::accounts` and answers `[]`. The providers page loads it
  next to `providers.list`, and its `NOT_IMPLEMENTED` put «Эта функция ещё не перенесена в
  новую оболочку приложения.» on the page. Nothing can be linked before TASK_067, which
  replaces the handler with the real `AccountDto` list.
- Register shutdown work with `coordinator.register(stage, name, || async { … })`. Mirror
  `main.ts`: synchronous stops (`retention.stop()`) in `Stop`, `runs.dispose()` in `Drain`, and
  the `Promise.all` group (`mcp`, `accounts`, `sidecar`) in `Dispose`. The coordinator lives in
  `zvs-studio`'s `Lifecycle` state; pass it into `compose` sections as they are ported.
- `ProviderService` needs a tokio runtime to probe a saved provider (it spawns). IPC handlers
  and tests already run in one. `HealthCheckService` takes an explicit `runtime` because
  `compose` runs outside the async context.
- TASK_067 should replace the SQL-only account fixtures with `AccountRepository` writes and
  plug its `AccountPort` into `ProviderRegistryOptions::accounts`. `ProviderService` already
  reads `status`, `token_expires_at`, `email_masked` and `display_name`.
- `ProviderSettingsDto` floats are `f64`, so a stored `1` reads back as `1.0`. The wire value is
  the same number for JS, but a fixture that round-trips a whole-number temperature would not
  compare equal in Rust.
- Gates run on Windows on 2026-10-14, all exit 0: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm build`, `npx vitest run`
  (1165 passed, 2 skipped) and `node scripts/contract-channels.mjs --check`.
- Not done: the manual `pnpm dev:tauri` check (Ollama and OpenRouter providers, a broken key,
  health checks, the shutdown order in `studio.jsonl`). No Ollama was running for a live probe.
