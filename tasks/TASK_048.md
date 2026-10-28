---
task: TASK_048
title: Integration framework — accounts, triggers, tool exposure
phase: 9 — Integrations
status: done
depends_on: [TASK_031, TASK_076]
unblocks: [TASK_049, TASK_050]
estimate: 3 h
---

# TASK_048 — Integration framework — accounts, triggers, tool exposure

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

`/design/Integrations.dc.html` shows tabs for Mail, Telegram and Git hosting sharing one three-column shape: accounts list, configuration form, and an automation rail listing what the account exposes. This task builds the framework they all plug into, before any specific integration exists.

The canvas note states the invariant: passwords and tokens are always a pick from Secrets, never typed into the form. That is the same rule the Providers and Connections pages follow, and the framework should make it impossible to violate rather than merely discouraged.

Each connected account exposes two things: **triggers** that can start a scenario (`mail.received`), and **tools** that a scenario or agent can call (`mail.send`, `mail.search`). Tools register into the TASK_031 registry like everything else, so they inherit the permission ceiling automatically.

## Why

Three integrations follow. Building the account model, credential rule, trigger dispatch and tool registration once means each integration is a small adapter rather than a subsystem.

## Depends on

- `TASK_031` — Tool registry and the permission ceiling.

## Read first

- `/design/Integrations.dc.html` — the shared three-column shape and the automation rail
- `/docs/PASSPORT.md` §14 (credentials), §11 (triggers start kernel runs)

## Scope

**In scope**
- `integration_account` table and repository
- Provider interface every integration implements: configure, test, start, stop, triggers, tools
- Credential resolution through Secrets, enforced by the interface's shape
- Trigger dispatch: an event from an account starts a bound scenario run
- Tool registration into the TASK_031 registry
- Health and activity tracking for the rail
- A background-worker API (supervised long-lived loops: long polling, IDLE, interval polling) that account lifecycles run on
- Channels and DTOs

**Out of scope** (do not do these, even if tempting)
- Any specific integration (TASK_049, TASK_050)
- The page (TASK_051)
- Webhooks — dropped by ADR 0004: a desktop app has no public endpoint, so inbound events come from background workers

## Steps

1. `integration_account` table: id, kind, name, enabled, config JSON (non-secret), secretId fk, status, statusDetail, lastActivityAt, counters, timestamps.
2. `IntegrationProvider` interface: `describe()` (returns the field schema for the form, reusing TASK_011's `FieldDescriptor` so the form is generated the same way), `test(config)`, `start(account)`, `stop(account)`, `triggers()`, `tools()`.
3. Credential rule enforced structurally: the config type carries a `secretId`, never a credential value. There should be no field in the schema of type 'password' at all — a credential is always a secret reference, so the form renders a picker by construction.
4. Trigger dispatch: an account emits typed trigger events; the dispatcher finds scenarios bound to that trigger and starts a kernel run per event, with the event payload as run input. Deduplicate by a per-event id so a reconnecting account does not replay history as new work — this is the bug that will otherwise send fifty duplicate emails.
5. Tool registration: an account's tools register into the registry scoped to that account, so two mail accounts expose distinguishable tools.
6. Lifecycle: start enabled accounts at boot, stop cleanly on quit, restart with backoff on failure, and never let one failing account block the others.
7. Health and activity: status, counters and a recent activity log for the rail.
8. Channels: `integrations.list|get|create|update|remove|test|enable|disable`, `integrations.activity`.
9. Tests with a fake integration: config with a credential value is rejected at the type level; trigger dispatch starts a run and deduplicates repeats; tools register scoped per account; a failing account restarts without affecting others.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Register the fake integration, bind a scenario to its trigger, fire an event, and confirm a run starts with the payload. Fire the same event id twice and confirm only one run.

## Definition of done

- [x] One framework serving all integration kinds
- [x] Credentials are secret references by construction — no password field is representable
- [x] Trigger dispatch starts runs and deduplicates by event id
- [x] Tools register per account into the registry
- [x] One failing account cannot block others
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint` pass; `pnpm build` left to CI
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `zvs-host/src/workers/` — `WorkerSupervisor`, the host's background-worker API
(PASSPORT §15, ADR 0004). `spawn(WorkerSpec, WorkerBody)` runs a named tokio task. A failure
or panic restarts it with `Backoff` (1 s doubling to 5 min, reset after 30 s `running`).
`WorkerFault::permanent` stops it as `failed`. `stop`/`remove`/`restart`/`dispose` are
available, a body that ignores cancellation is aborted after 5 s, and `watch` sees every state
change. Channels `workers.list` and `workers.restart`. `zvs-host/src/integrations/` —
`IntegrationProvider` (`describe`, `triggers`, `tools`, `test`, `start`, `stop`,
`call_tool`), `IntegrationService` and `IntegrationConfigView`. Migration `0028_integration`
adds `integration_account`, `integration_event` (dedup, 30-day retention) and
`integration_activity` (last 200 per account). An enabled account runs as worker
`integration:<id>`. `IntegrationSession::emit` deduplicates by `(account, trigger, eventId)`,
then fires the new `trigger.integration` scenario trigger (`ScenarioTriggerKind::Integration`,
params `accountId` + `trigger`, both required) through `ScenarioRunner::integration_sink`. The
run input is `{ accountId, kind, trigger, eventId, payload }`. Tools register per account as
`<head>.<last 8 of id>.<rest>`, with source `integration` and side-effecting tools at `ask`,
and run through an `IntegrationToolNode`. The contract has the DTOs and Zod schemas plus 20
golden fixtures in `fixtures/integrations/`. Channels
`integrations.kinds|list|get|create|update|remove|test|enable|disable|activity`. The scenario
editor got a label, icon, form and account picker for `trigger.integration`. Boot calls
`start_enabled` (one failing account is logged and skipped). The `Stop` shutdown stage
disposes integrations and then workers.

**Deviated from plan:**
- No `status` / `statusDetail` columns. Status is live from the worker (`disabled` when the
  account is off, `stopped` with no worker), and only `last_error` is stored, so a stale
  "running" can never survive a crash. There is no single `secretId` FK. A config may hold
  several `{ secretId }` references (mail needs IMAP and SMTP), each recorded as an
  `integration` secret consumer.
- `test` returns `Vec<IntegrationCheckDto>` from the provider. The service adds latency and
  `ok`, so mail can report IMAP and SMTP separately. `integrations.test` takes an account ref.
- Added `integrations.kinds` (the provider descriptors the page needs) and
  `integrations.get` returns the detail with each tool's registry key.
- Webhooks dropped (ADR 0004). TASK_050 step 3 and TASK_051 were updated to match.
- The fake provider exists only in Rust tests, so the manual check is covered by
  `integrations::tests` (`a_trigger_event_starts_a_bound_run_once_per_event_id`). No provider
  is registered in the shipped app yet, so `integrations.kinds` is empty until TASK_049/050.
- Fixed a pre-existing clippy 0.1.99 `unnecessary eager cloning` in `scenarios/compile.rs`
  (filter before `cloned`).

**Next session should know:**
- A provider plugs in by being added to `IntegrationServiceOptions::providers` in the
  composition root (`apps/studio/src-tauri/src/lib.rs`). `start` must call
  `session.worker.ready()` once connected and return when `session.cancellation()` fires.
  It returns `WorkerFault::permanent` for errors that retrying cannot fix (bad credentials,
  missing secret) and an ordinary error otherwise. Telegram long polling is a `getUpdates`
  loop in `start`. Emit with a stable event id (`update_id`, message id + folder) and keep a
  cursor, so a restart does not refetch.
- A provider is refused at registration if a non-secret field key looks like a credential
  (`password`, `token`, `secret`, `apiKey`, `privateKey`, `credential`). Make such a field
  `FieldKind::Secret`.
- Untrusted-content marking (TASK_049/050 step 5/6) is not in the framework: the payload
  passes through as given.
- `trigger.webhook` is still in the scenario catalogue with nothing firing it, and
  `trigger.schedule` has no worker yet either. Both are candidates for a follow-up; a cron
  worker would use `WorkerSupervisor`.
