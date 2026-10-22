---
task: TASK_015
title: Test-connection and model-discovery service
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_014]
unblocks: [TASK_016, TASK_015_EXTRA_1]
estimate: 3 h
---

# TASK_015 — Test-connection and model-discovery service

## Context

The ports can reach vendors (TASK_014) and provider rows can be stored (TASK_013). This task
is the service that joins them, built around the single interaction the Providers page is
designed around: **you fill in a connection, press Test connection, and the app comes back
with a latency and a list of models.**

Look at `/design/Providers.dc.html`: after the button there is a line reading
`200 OK · 412 ms · 14 models — checked 2 min ago`, and below it a grid of model cards. That
one probe does four things: it validates the credential, records a latency, refreshes the
stored model list, and sets the provider's health status which the sidebar dot and the
"Token expires in 12d" warning both read from.

The same probe backs a periodic health check, so the machinery must be callable both
interactively (with a result the UI awaits) and in the background (writing status quietly).

One subtlety worth getting right: a probe can succeed at the connection level while returning
zero models, and it can fail with a valid credential (vendor outage). Those are different
states and the UI shows them differently. Model the outcome as a small union rather than a
boolean.

**This service is auth-mode agnostic** (PASSPORT §12, rewritten 2026-09-10). A provider is
credentialed either by an API key (`auth_mode: 'api'`) or by a browser session linked to a
vendor account (`auth_mode: 'account'`), and `ProviderRegistry` already hides that difference
behind the driver ports. `ProviderService` must not branch on auth mode anywhere except two
places, both of them honest reporting rather than logic: the outcome union gains
`account-not-linked` and `session-expired`, and status derivation reads account health the way
it already reads secret rotation. `TASK_015_EXTRA_1` adds the linking flow on top of this
service; if it has to reshape `probe()` when it lands, this task got the seam wrong.

## Why

Without this, a provider is an unverified guess and the model cards have nothing to show.
Every later feature that picks a model reads the rows this service writes.

## Depends on

- `TASK_014` — driver ports, registry, error normalisation, `FakeDriver`.
- `TASK_013` — provider and model repositories.

## Read first

- `/design/Providers.dc.html` — the result line, the model cards, the provider list's status
  dots, and the amber "Token expires in 12d" state.
- `/docs/PASSPORT.md` §12 (test connection is the same call the health check reuses; the two
  axes and why this service ignores one of them), §8.

## Scope

**In scope**
- `ProviderService`: CRUD over providers, mapping entities to DTOs
- `probe(providerId | draft)` returning a structured outcome
- Model discovery persisted through `replaceForProvider`
- Health status derivation, including secret rotation and account session state
- A background health check on an interval, cancellable, quiet
- Provider DTOs and channels
- Tests using `FakeDriver`

**Out of scope** (do not do these, even if tempting)
- UI (TASK_016)
- Account linking, the Login button and `accounts.*` channels (TASK_015_EXTRA_1)
- Retry/backoff policy beyond one attempt — note it for TASK_054
- Cost or quota tracking

## Steps

1. `host/services/ProviderService.ts`, constructed in the composition root with
   `ProviderRepository`, `ModelRepository`, `ProviderRegistry`, `SecretService`, clock,
   logger, event bus. Where `TASK_014_EXTRA_1` has already landed, `AccountRepository` joins
   that list; if it has not, leave the constructor parameter out rather than stubbing it.
2. `probe(input)` accepts either a saved provider id **or** an unsaved draft — the mockup
   lets you test a connection before saving, and that is the right behaviour. For a draft,
   build a throwaway client and never persist anything. A draft in `account` mode references
   an existing linked account; it never creates one.
3. Probe outcome as a union, not a boolean:
   - `{ kind: 'ok', latencyMs, models: DiscoveredModel[], live: boolean }`
   - `{ kind: 'ok-empty', latencyMs }` — reachable and authorised, zero models returned
   - `{ kind: 'auth-failed' }` — an API key was rejected
   - `{ kind: 'account-not-linked' }` — `auth_mode: 'account'` with no account attached
   - `{ kind: 'session-expired' }` — the linked account's browser session is no longer valid
   - `{ kind: 'unreachable', detail }` · `{ kind: 'rate-limited', retryAfter? }` ·
     `{ kind: 'error', code, detail }`
   `live: false` marks a curated static list from a vendor with no discovery endpoint, so
   TASK_016 can label it honestly instead of implying a probe listed them. Keep
   `auth-failed` and `session-expired` distinct — they lead the user to different actions
   (fix a key vs. sign in again), and collapsing them is the main thing to avoid here.
4. On a successful probe of a **saved** provider: `replaceForProvider` inside a transaction,
   set status `ok`, record latency and `lastProbeAt`. Preserve `defaultModelId` if that model
   still exists; clear it and log if it vanished from the vendor.
5. Status derivation is a pure function, separately testable:
   - probe failed → `failed` with the reason
   - `session-expired` or `account-not-linked` → `failed`, detail "sign in again" /
     "no account linked"
   - probe fine but the referenced secret is `overdue` → `degraded`, detail "credential
     overdue"; `due-soon` → `degraded`, detail "expires in N days" (this is the amber
     Anthropic row in the mockup)
   - probe fine but the linked account's `token_expires_at` is within the warning window →
     `degraded`, detail "session expires in N days" — the same amber treatment, one rule
   - otherwise → `ok`
   Write it so a new credential-lifetime source is one more case, not a new branch structure.
6. `HealthCheckService` — runs probes for enabled providers on an interval (default 15
   minutes; make it a setting). Never runs against a provider currently being probed
   interactively. Emits `progress` events on the bus so the UI can show a refresh happening,
   and writes status quietly otherwise. Must be stoppable on app quit.
7. DTOs: `ProviderDto`, `ProviderSummaryDto`, `ModelDto`, `ProbeResultDto`,
   `CreateProviderInput`, `UpdateProviderInput`. Model settings are a nested object matching
   the mockup's fields. `ProviderDto` carries `authMode`, `adapter`, and the
   `AdapterCapabilities` descriptor from TASK_014 so the page can grey out controls the family
   ignores. No secret values, ever — a provider DTO carries `secretId` and the secret's
   display name, nothing more; an account-mode provider carries `accountId` and a masked
   identity, never a token and never a cookie.
8. Channels: `providers.list` (with a capability filter for the three tabs), `providers.get`,
   `providers.create`, `providers.update`, `providers.remove`, `providers.probe`,
   `providers.setDefaultModel`, `providers.refreshAll`.
9. Concurrency: two probes of the same provider must not race into
   `replaceForProvider`. Guard with a per-provider in-flight map; a second request either
   joins the first or is rejected with `CONFLICT`. Pick one, document it in a comment.
10. Tests with `FakeDriver`: each outcome kind maps to the right status and detail;
    `auth-failed` and `session-expired` produce different details; discovery replaces stale
    models atomically; a vanished default model is cleared and logged; rotation state produces
    `degraded`; a draft probe writes nothing to the database; concurrent probes do not corrupt
    the model list; the health check can be stopped. Add one test asserting `ProviderService`
    contains no `auth_mode` branch outside status derivation and outcome mapping — a grep-style
    assertion is fine, and it is what keeps EXTRA_1 cheap.
11. Commit: `TASK_015: test connection and model discovery`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Should the app probe your providers automatically in the background — say every fifteen
> minutes — so the status dots stay current, or only when you press Test connection? Automatic
> probing means the app makes network calls on its own schedule: with metered API providers
> that is a small ongoing cost, and with account providers it means touching your signed-in
> vendor session without you asking."
>
> **Why it matters:** It decides whether `HealthCheckService` starts by default, and it is a
> quiet-outbound-traffic question — PASSPORT §14 says no outbound calls the user did not
> configure, so this needs to be a deliberate choice rather than a default I pick.
> **Options:** (a) manual only — dots reflect the last manual probe, zero surprise traffic;
> (b) automatic on an interval, on by default, interval configurable in Settings;
> (c) automatic but off by default, opt-in from Settings; (d) split — automatic for account
> providers (free, and session expiry is worth catching early), manual for metered API ones.
> **If the user is unavailable:** take (c) — build the service, ship it disabled, expose the
> toggle for TASK_052. Note it in Handoff.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks against a real provider:
- Probe a correct configuration: latency and a real model list come back, rows land in the
  `model` table.
- Break the key: `auth-failed`, status `degraded`/`failed`, and **no** stack or vendor
  payload reaches the renderer.
- Point at an unreachable URL: `unreachable`, and the call returns within the configured
  timeout rather than hanging.
- Probe an unsaved draft: nothing is written to the database.

## Definition of done

- [x] `probe` works for both saved providers and unsaved drafts
- [x] Outcome is a union covering ok, ok-empty, auth, account-not-linked, session-expired,
      unreachable, rate-limited, error
- [x] The service does not branch on auth mode outside outcome mapping and status derivation
- [x] Successful probe replaces models transactionally and records latency
- [x] Status derivation includes secret rotation and account session state; pure and tested
- [x] Health check exists, respects the decision above, and stops cleanly on quit
- [x] Concurrent probes cannot corrupt the model list
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** Provider CRUD and all eight IPC channels; validated shared DTOs; saved and draft
probes; structured error outcomes; transactional discovery with default-model preservation;
credential lifetime status derivation; configurable opt-in health checks wired to startup,
settings changes and shutdown. Added 19 regression cases for failures, rollback, concurrency,
timeouts, credential lifetimes, read-only account drafts, IPC and cancellation.

**Deviated from plan:** Preserved the draft's option (c): background checks are off by default.
Concurrency policy is documented here instead of a code comment, per the user's instruction:
a second saved-provider probe joins the existing promise; connection updates and removal
return CONFLICT while probing. ProviderService uses UnitOfWork for repository access and
transactions. Probe credentials verify account identity without writing accounts or secrets;
account linking and credential persistence remain TASK_015_EXTRA_1. Used root `pnpm test`
because studio has no package-level test script. Changes remain uncommitted for review.

**Next session should know:** `providers.healthcheck.enabled` defaults to false;
`providers.healthcheck.minutes` defaults to 15 (minimum 1). Settings IPC applies changes
immediately; TASK_052 supplies the settings UI. Account expiry is Unix seconds, converted to
milliseconds for status derivation. Concurrent probes join, including a manual request during
a background probe. Stopping background work cancels its active probe and prevents subsequent
probes in that sweep. App quit aborts provider work before closing the database. Retry/backoff
remains TASK_054. Anthropic remains the pre-existing adapter stub and returns a structured error.

Validation: `pnpm test` passed (205 tests, one live-provider test skipped); `pnpm typecheck`,
`pnpm lint`, `pnpm build` and `git diff --check` passed. No live vendor calls were made.
