---
task: TASK_048
title: Integration framework — accounts, triggers, tool exposure
phase: 9 — Integrations
status: todo
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

`/design/Integrations.dc.html` shows four tabs — Mail, Telegram, Git hosting, Webhooks — sharing one three-column shape: accounts list, configuration form, and an automation rail listing what the account exposes. This task builds the framework they all plug into, before any specific integration exists.

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
- Channels and DTOs

**Out of scope** (do not do these, even if tempting)
- Any specific integration (TASK_049, TASK_050)
- The page (TASK_051)
- Webhooks — the tab exists; implement it only if it falls out cheaply

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

- [ ] One framework serving all integration kinds
- [ ] Credentials are secret references by construction — no password field is representable
- [ ] Trigger dispatch starts runs and deduplicates by event id
- [ ] Tools register per account into the registry
- [ ] One failing account cannot block others
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
