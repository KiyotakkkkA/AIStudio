---
task: TASK_067
title: Accounts in Rust — AccountTransport, identity probes, qwen-web and deepseek-web, AccountService
phase: M — Migration to Tauri
status: done
depends_on: [TASK_066]
unblocks: [TASK_068]
estimate: 5 h
---

# TASK_067 — Accounts in Rust — AccountTransport, identity probes, qwen-web and deepseek-web, AccountService

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

With the `SessionGateway` from TASK_066 in place, the rest of the account half of the AI layer
(TASK_014_EXTRA_1 – TASK_016_EXTRA_1) is a straight port:
- `transport/AccountTransport.ts`: the `Transport` implementation over the gateway. It handles
  credentials (`current`/`refresh`), sends the bearer when the family requires it, detects
  signed-out responses and maps them to `session-expired`, applies timeouts, and retries once
  on refresh.
- `identity/`: per-family probes (`qwen.ts` → `chat.qwen.ai/api/v1/auths/`, `deepseek.ts` →
  `/api/v0/users/current`), the probe registry, `AccountCredentialStore` (the captured bearer
  is stored through `SecretService`, encrypted, never in a DTO) and `ProbeAccountCredentials`.
- `adapters/qwenWeb.ts`, `deepseekWeb.ts` and `accountFamily.ts`: the wire shapes for those
  vendors, and Qwen account-mode generation and streaming (TASK_014_EXTRA_3).
- `services/AccountService.ts`: the link flow (open the vendor login in the browser, wait for
  the identity, persist the account, navigate back to `/providers`), cancel-link,
  auto-refresh, unlink (deletes the captured token immediately, the TASK_014_EXTRA_1
  decision), and reacting to `cookies-cleared` and closed link tabs from `BrowserLifecycle`.
- The `accounts.*` channels: `cancelLink`, `link`, `list`, `refresh`, `unlink`.

## Why

This completes the AI Providers page under Tauri, including the Accounts tab, and replaces the
last consumer of Electron's `session` API in the AI layer.

## Depends on

- `TASK_066` — the `SessionGateway` implementation and bearer capture.

## Read first

- `/docs/PASSPORT.md` §12 (Accounts), §21 (session and identity update)
- `apps/studio/src/host/drivers/ai/transport/AccountTransport.ts`, `identity/*`,
  `adapters/qwenWeb.ts`, `deepseekWeb.ts`, `accountFamily.ts`
- `apps/studio/src/host/services/AccountService.ts`,
  `data/repositories/AccountRepository.ts`, `ipc/accounts.ts`
- `apps/studio/src/host/main.ts`: how `accounts`, `browser.openTab` and `navigateWorkspace`
  are wired
- `apps/studio/tests/accountAdapters.test.ts`, `accountService.test.ts`, `accounts.test.ts`,
  `liveQwen.test.ts`
- `docs/QWEN_WEB.md`, `docs/know-how/QWEN_DEEPSEEK_TOOLS_INJECTING.md`

## Scope

**In scope**
- `AccountTransport` over `SessionGateway`, with every behaviour in the TS tests
- Identity probes, registry, `AccountCredentialStore`, `ProbeAccountCredentials`
- `qwen-web` and `deepseek-web` adapters and the account family helpers, registered in
  `ProviderRegistry`, which replaces TASK_063's stub
- `AccountRepository`, `AccountService`, the `accounts.*` channels, DTOs and fixtures
- `BrowserWorkspace::open_tab(url)` and a "navigate the studio to a route" port, used by the
  link flow, as in `main.ts`
- Lifecycle wiring: `cookies-cleared` and closed link tabs move accounts to `needs-relink` or
  cancel linking, as today

**Out of scope** (do not do these, even if tempting)
- DeepSeek account-mode generation beyond what the TS host already does (it was deferred in
  TASK_014_EXTRA_3)
- Several live sessions per vendor (one profile, one session per vendor, unchanged)

## Steps

1. Port `AccountTransport` with its tests against a fake `SessionGateway`.
2. Identity probes and the credential store, with tests that the token goes through
   `SecretService` and is deleted on unlink.
3. Adapters `qwen-web` and `deepseek-web`, with `accountAdapters.test.ts` ported against
   recorded fixtures. `liveQwen` becomes an `#[ignore]`d test.
4. `AccountService` and the repository, with `accountService.test.ts` and `accounts.test.ts`
   ported against fakes for the browser, lifecycle and clock.
5. Channels, wiring and the navigation port.
6. Extend `ShutdownCoordinator` with `accounts.dispose()`.

## Verify

```bash
cargo test -p zvs-host -p zvs-studio
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, Providers → Accounts → link Qwen. The browser opens, you
log in, the studio returns to Providers, and the masked identity shows. Create a Qwen
account-mode provider, probe it, and stream a reply through it. Cancel a link mid-flow. Clear
`qwen.ai` cookies and see the account turn `needs-relink` and the provider report
`session-expired`. Unlink, and confirm the stored token secret is gone.

## Definition of done

- [x] Account transport, probes, credential store, both web adapters and `AccountService` are
      ported with their tests
- [ ] The Accounts tab and account-mode providers work end to end under Tauri (manual check not
      run yet; every `accounts.*` channel is served and tested through `IpcServer::dispatch`)
- [x] Cookie clearing and unlinking behave exactly as decided in TASK_014_EXTRA_1
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract::accounts`: `AccountDto`, `AccountLinkInput`, `AccountRef`, `AccountLinkResult`
  (internally tagged on `status`), `AccountsCancelLinkOutput`, `AccountsUnlinkOutput` (both
  literals checked) and `AccountsListOutput`. Six fixture names (12 files) in
  `packages/shared/fixtures/accounts`, registered in the Rust and the Vitest table.
- `zvs-host`:
  - Data: `AccountRow` now has every column (`avatar_url`, `token_secret_id`,
    `status_detail`, `last_checked_at`; `token_expires_at` is `i64` seconds).
    `AccountRepository` gains `create`, `find_by_adapter_and_external_id`, `list_by_adapter`,
    `update` (`AccountPatch`), `update_status` and `remove` (`AccountRemoval`: detaches
    providers as `failed` «no account linked», frees the token secret), keeping
    `secret_usage` in step. `ProviderRepository::count_by_account`.
  - `SecretService::create_in`, `update_in` and `remove_in` take `&Repositories`, so a token
    secret is written in the same transaction as its account. `create`, `update` and `remove`
    go through the same code.
  - `drivers::ai::transport::account`: `AccountCredentials`, `AccountTransport` and its options.
  - `drivers::ai::identity`: `IdentityProbe`, `ProbeResult`, `AccountIdentity`,
    `fetch_identity`, `QwenIdentityProbe`, `DeepSeekIdentityProbe`, `identity_probe(s)`,
    `account_session` (`StoredTokenSession`), `ProbeAccountCredentials` and
    `AccountCredentialStore`.
  - `drivers::ai::adapters`: `account_family` (`AccountAdapterCore`, `AccountAdapterOptions`,
    `flatten_prompt`, the malformed/truncated errors), `qwen_web` (`QwenWebAdapter`,
    `map_qwen_models`, the observations) and `deepseek_web` (`DeepSeekWebAdapter`, the curated
    list). Both families are `implemented: true` and build a text-only driver.
  - `drivers::ai::registry`: `DatabaseAccounts` (the real `AccountPort`) and `SessionFactory`.
  - `platform::navigation`: `WorkspaceRoute`, `WorkspaceNavigator`, implemented by `EventBus`
    as the TS `navigateWorkspace` (one `navigation` step, then `ok`).
  - `services::accounts`: `AccountService` (`list`, `link`, `cancel_link`, `unlink`,
    `refresh`, `start_auto_refresh`, `dispose`), the `AccountBrowser` port and `mask_email`.
    `Services` gains `accounts`.
  - `ipc::accounts`: `accounts.list`, `link`, `cancelLink`, `unlink`, `refresh`.
  - `testing`: `FakeSessionGateway` gains per-URL queues, `Stall`, held bodies and recorded
    cancel scopes. `fake_accounts` adds `FakeIdentityProbes` (scripted once/always/hang and
    release), `RecordingBrowser`, `RecordingNavigator` and `TokioClock`. `test_services`
    builds an `AccountService` on them.
- `zvs-studio`:
  - `BrowserWorkspace::open_link_tab(url)`: navigates the studio to `/browser`, creates the
    chrome and a tab on the URL, and tracks it as a link tab (`model::LinkTabs`). Closing the
    tab, a crash of its page, or typing a new address into it emits `link-tab-closed`;
    `link-finished` forgets it; in-page navigation (OAuth redirects) and hiding do not.
  - `browser::accounts`: `WorkspaceOpener` (the `AccountBrowser` port, run on
    `spawn_blocking`) and `gateway_sessions` (every partition maps to the one
    `AccountGateway`).
  - Composition: the lifecycle, workspace and gateway are built in `compose`, before the
    services. The registry gets `DatabaseAccounts` over the gateway, `AccountService` gets the
    opener, the lifecycle and the event bus as navigator, and auto-refresh starts before the
    health check, as in `main.ts`. `accounts` and `account-gateway` are registered in the
    `Dispose` stage.
- Tests: 67 new in `zvs-host` (337 passed, 5 ignored), 2 in `zvs-contract`, 1 in `zvs-studio`.
  Ported: every case of `accounts.test.ts` except the migration-0004 and no-electron-import
  checks (the first is covered by a column test on the baseline, the second has no Rust
  equivalent), every case of `accountAdapters.test.ts`, every case of `accountService.test.ts`
  (on paused tokio time), the Qwen case of `toolCalling.test.ts`, and the account-draft case of
  `providerService.test.ts` deferred in TASK_064. `liveQwen.test.ts` is
  `live_qwen_identity_models_and_thinking_and_fast_generation` (`#[ignore]`,
  `LIVE_QWEN_TOKEN`), through `AccountTransport` over a direct session.
- Docs: `crates/zvs-host/README.md` (Account transport, Identity, Account adapters, Accounts,
  the module tables, the shutdown list) and `crates/zvs-contract/README.md`.

**Deviated from plan:**

- No source comments, at the user's request.
- The registry's credentials are `ProbeAccountCredentials`, as `main.ts` wires them.
  `AccountCredentialStore` is ported and tested but, as in TS, has no production caller.
- `AccountService` takes a `WorkspaceNavigator` instead of an `onLinked` callback; it
  navigates to `/providers` exactly where `onLinked` ran. `AccountBrowser::open_tab` is async,
  because opening a tab must not block a tokio worker on the main thread.
- A link or refresh runs as a spawned task joined through a `Shared` future, like
  `ProviderService` probes, so it runs whether or not the caller polls it. The 180 s deadline
  cancels the same token the probe receives, which is how TS's timer aborted the controller.
- An aborted refresh rejects with `CONFLICT` «Account operation cancelled» (the TS
  `abortable` error) instead of a DOM `AbortError`.
- A refresh or link that fails for a reason other than a signed-out or unreachable vendor
  still maps to `PROVIDER_UNREACHABLE` «Could not check the vendor session», as in TS. An
  account row that does not parse becomes `DB_ERROR` «Аккаунт хранится в повреждённом виде»
  where TS threw a Zod error.
- `AccountDto.expiresAt` and `lastCheckedAt` are `i64` milliseconds, not `f64`: the DTO is
  output-only and always holds whole milliseconds.
- `fetch_identity` returns `RUN_CANCELLED` on a cancelled body read and `timed_out(20)` on the
  identity deadline; TS swallowed a failed body read into an empty string, which then failed
  as a malformed profile.
- The Qwen stream frame and model list are validated with serde structs that mirror the Zod
  schemas: a frame that does not match is skipped, a model list with one bad entry fails as a
  whole.

**Next session should know:**

- Not done: the manual `pnpm dev:tauri` check (link Qwen, stream through a Qwen account-mode
  provider, cancel a link, clear `qwen.ai` cookies, unlink). It needs a real Qwen sign-in.
  TASK_066's note still applies: DeepSeek may answer with an AWS WAF challenge until the
  profile has passed it.
- Auto-refresh starts at boot. An account linked more than 15 minutes ago is probed through the
  gateway right away, which creates the hidden gateway page for that vendor before the
  studio window exists.
- TASK_068 and later: services that need the browser go through
  `app.state::<BrowserWorkspace>()`; anything that should open a vendor page can reuse
  `WorkspaceOpener`. Register shutdown work in `compose`, where the coordinator now also holds
  `accounts` and `account-gateway`.
- `SecretService::*_in` is the way to write secrets inside another service's transaction.
  The connection mutex is not re-entrant, so calling `create`/`update`/`remove` from inside
  `database.transaction` deadlocks.
- Gates run on Windows on 2026-10-15, all exit 0: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` (also with `--features e2e` on
  `zvs-studio`), `cargo test --workspace`, `pnpm typecheck`, `pnpm lint`,
  `pnpm format:check`, `pnpm build`, `pnpm build:tauri`, `npx vitest run` (1232 passed,
  2 skipped) and `node scripts/contract-channels.mjs --check`. The `e2e` gateway harness was
  rebuilt but not rerun.
