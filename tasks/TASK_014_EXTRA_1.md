---
task: TASK_014_EXTRA_1
title: Account auth mode — schema, AccountTransport, identity probes
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_014, TASK_045]
unblocks: [TASK_014_EXTRA_2, TASK_015_EXTRA_1]
estimate: 4 h
---

# TASK_014_EXTRA_1 — Account auth mode: schema, AccountTransport, identity probes

## Context

Some vendors the user actually works with — Qwen and DeepSeek today, more later — have no
practical API-key path for an individual. What they do have is a web app the user is already
signed into. TASK_045 shipped an integrated browser inside the studio window whose pages run
in a persistent Electron session partition, `persist:browser-work`, and cookies set there
survive restarts. **That session is a credential**, and this task is where the AI layer learns
to use it.

The rule from PASSPORT §12, decided 2026-09-10: account-mode requests are issued from the host
with Electron's `net.fetch({ session })` bound to that partition, so the vendor's cookies ride
along exactly as they do in the browser tab. Cookies are never read out, never copied into a
jar, never held in app memory, never shown to the renderer. Where a vendor additionally
requires a bearer token — Qwen returns one from its identity endpoint — that token is stored
through `SecretService` like any other credential and attached per request.

TASK_014 built the `Transport` seam for precisely this. This task adds the second
implementation and the one branch in `ProviderRegistry` that selects it. **If you find
yourself editing an adapter file in this task, stop — the seam has leaked** and the fix
belongs in the transport, not the adapter.

The second half of the task is identity. Each account-mode family has a "who am I" endpoint
that answers only when the session is valid:

| Family | Identity endpoint | Notes |
| --- | --- | --- |
| `deepseek-web` | `https://chat.deepseek.com/api/v0/users/current` | `data.biz_data` carries `id`, masked `email`, `id_profile.name`, `id_profile.picture`, and a `token` |
| `qwen-web` | `https://chat.qwen.ai/api/v1/auths/` | flat object: `id`, `email`, `name`, `profile_image_url` (a data URI), `token`, `token_type: "Bearer"`, `expires_at` (unix seconds) |

Both shapes collapse onto one `AccountIdentity`. Both response bodies are **untrusted input**
(PASSPORT §14): validate with a zod schema, take only the fields we mapped, and never log or
persist the raw payload — it contains a live credential.

## Why

Without this, "connect by account" cannot exist and the Accounts tab has nothing behind it.
It is also the task that keeps the browser session from spreading through the codebase: one
transport, one identity mapper per family, and no other file knows a cookie exists.

## Depends on

- `TASK_014` — ports, `Transport` seam, `ProviderRegistry`, error normalisation, fakes.
- `TASK_045` — `BrowserViewManager`, the `persist:browser-work` partition, cookie tooling.
- `TASK_010` — `SecretService` for storing captured bearer tokens.

## Read first

- `/docs/PASSPORT.md` §12 (the two axes; the Accounts subsection), §21 "Session and identity"
  including the 2026-09-10 update, §14 (untrusted input; credentials never leave the host).
- `apps/studio/src/host/browser/BrowserViewManager.ts` — how the partition is obtained
  (`session.fromPartition(BROWSER_PARTITION)`) and how its permissions are locked down.
- `packages/shared/src/browser.ts` — existing browser DTOs, for naming consistency.

## Scope

**In scope**
- Migration `0004`: the `account` table and `provider.account_id`
- `AccountRepository`
- `host/drivers/ai/transport/AccountTransport.ts` over `net.fetch({ session })`
- The `account` branch in `ProviderRegistry`
- `host/drivers/ai/identity/` — `AccountIdentity` plus the Qwen and DeepSeek mappers
- `PROVIDER_SESSION_EXPIRED` added to the error model and mapped from account-mode 401/403
- Bearer-token capture into `SecretService`, with expiry recorded on the account row
- Unit tests over a fake session/fetch — no live vendor calls in tests

**Out of scope** (do not do these, even if tempting)
- The Qwen and DeepSeek chat/model adapters (TASK_014_EXTRA_2)
- The login flow, polling, and `accounts.*` channels (TASK_015_EXTRA_1)
- Any UI (TASK_016_EXTRA_1)
- Reading, exporting or displaying cookie **values** — metadata only, and only where TASK_045
  already does it
- A second browser partition per account — the schema allows many accounts, the runtime keeps
  one live session per vendor for now

## Steps

1. Migration `0004_account.sql` and the Drizzle schema:
   ```
   account
     id                text primary key
     adapter           text not null          -- qwen-web | deepseek-web | …
     partition         text not null          -- 'persist:browser-work' for now
     external_id       text not null          -- the vendor's own user id
     email_masked      text                   -- store exactly what the vendor returned; never unmask
     display_name      text
     avatar_url        text                   -- may be an https URL or a data: URI
     token_secret_id   text references secret(id) on delete set null
     token_expires_at  integer                -- unix seconds, when the vendor tells us
     status            text not null default 'linked'   -- linked | needs-relink | revoked
     status_detail     text
     last_checked_at   integer
     linked_at         integer not null
     updated_at        integer not null
   unique (adapter, external_id)
   ```
   Plus `provider.account_id text references account(id) on delete set null`.
   Add a CHECK-equivalent guard in the repository (SQLite CHECKs across columns are fine too):
   an `auth_mode = 'account'` provider must have `account_id` and no `secret_id`; an
   `auth_mode = 'api'` provider must have neither `account_id` nor a partition. Test both.
2. `AccountRepository` following the TASK_006 repository base: `create`, `getById`,
   `findByAdapterAndExternalId`, `listByAdapter`, `updateStatus`, `remove`. Removing an account
   must not cascade-delete providers — it nulls `account_id` and the provider goes
   `needs-relink`, so the user does not lose their model settings.
3. `AccountIdentity` in `packages/shared` (it is renderer-visible, so keep it clean):
   ```ts
   interface AccountIdentity {
     externalId: string;
     emailMasked?: string;
     displayName?: string;
     avatarUrl?: string;
     expiresAt?: number;    // unix seconds
   }
   ```
   **No token field.** The token never enters a DTO; it goes straight to `SecretService` on the
   host and only the resulting `token_secret_id` is stored.
4. `host/drivers/ai/identity/qwen.ts` and `deepseek.ts` — each exports a zod schema for the
   vendor's response and a mapper onto `AccountIdentity` plus an optional
   `{ token, tokenType }` handed to the caller in-process. DeepSeek's payload nests under
   `data.biz_data`; its `code`/`biz_code` are `0` on success, so a 200 with a non-zero code is
   a failure, not a success — assert that, it is easy to miss. Qwen's payload is flat and
   carries `expires_at`; record it.
5. `host/drivers/ai/identity/IdentityProbe.ts` — one interface, one registry keyed by adapter
   family: `probe(session, signal): Promise<AccountIdentity>`. A 401/403 or a redirect to a
   login page is `PROVIDER_SESSION_EXPIRED`, not `PROVIDER_AUTH_FAILED`; a network failure is
   `PROVIDER_UNREACHABLE`. Following a redirect to HTML instead of JSON is the normal
   "signed out" signal for both vendors — handle it explicitly rather than letting a JSON
   parse error surface.
6. `AccountTransport`:
   - Constructed with the partition name, the account row, and a token resolver.
   - Uses `net.fetch(url, { session, credentials: 'include' })` so partition cookies are
     attached by Chromium. Do not construct a `Cookie` header by hand.
   - Sets a realistic `Origin`/`Referer` for the vendor and reuses the browser's user agent.
     Add a comment: these are what make the request indistinguishable from the user's own
     tab, and are not an attempt to hide anything from the vendor.
   - Attaches `Authorization: <tokenType> <token>` when the account has a token secret,
     resolving it per request through `SecretService` and never caching the plaintext.
   - Implements the same `request`/`stream` shape as `ApiTransport`, including timeout
     composition with the caller's signal.
7. `ProviderRegistry`: one new branch in the auth-mode switch — `account` builds an
   `AccountTransport` from `provider.account_id`. A provider whose account is missing or
   `needs-relink` fails fast with `PROVIDER_SESSION_EXPIRED` and a detail the UI can render;
   it must not attempt the request. Cache invalidation now also keys on the account row's
   `updated_at`.
8. Error model: add `PROVIDER_SESSION_EXPIRED` to `AppErrorCode` in `packages/shared` and map
   it in `drivers/ai/errors.ts` **only** for account-mode transports — an API-key 401 stays
   `PROVIDER_AUTH_FAILED`. Distinguishing the two is the whole point of the state.
9. Token refresh reality check: neither vendor gives us a refresh endpoint we own. When a
   token expires but session cookies are still valid, re-running the identity probe yields a
   fresh token — so treat "token expired" as "re-probe identity once, then retry"; only if the
   probe itself fails is the account `needs-relink`. Implement that single retry here and
   comment why.
10. Tests, using a fake session and a scripted `net.fetch`: both vendor payload shapes map to
    `AccountIdentity` correctly; DeepSeek's non-zero `biz_code` on a 200 is a failure; a login
    redirect produces `PROVIDER_SESSION_EXPIRED`; the token is written through `SecretService`
    and never appears in a DTO, a log line, or the account row; an account-mode provider with
    no account fails without issuing a request; expired-token retry re-probes exactly once.
    Add one assertion that no test file imports `electron` cookies APIs.
11. Commit: `TASK_014_EXTRA_1: account auth mode, transport and identity probes`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "When you unlink an account or clear that vendor's cookies, should the app also delete the
> captured bearer token from the secret store immediately, or keep it until it expires so a
> re-link is instant?"
>
> **Why it matters:** it is a credential-lifetime question, and PASSPORT §14 leans toward not
> keeping credentials we no longer need. Keeping it is friendlier; deleting it is safer.
> **Options:** (a) delete on unlink and on cookie clear; (b) keep until `token_expires_at`
> then delete; (c) keep until the user deletes the secret manually on the Secrets page.
> **If the user is unavailable:** take (a) — deleting is the reversible choice, since a
> re-link costs one click.

## Verify

```bash
pnpm test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check, with a real signed-in session: open `/browser`, sign into DeepSeek, then from a
host-side scratch script run the DeepSeek identity probe against the partition — it must
return your masked email and display name and store a token secret. Sign out in the browser
tab (or clear that domain's cookies in Sites & cookies) and run it again — it must return
`PROVIDER_SESSION_EXPIRED`, not a stack trace and not `PROVIDER_AUTH_FAILED`. Repeat for Qwen,
and confirm the recorded `token_expires_at` matches the `expires_at` the endpoint returned.

## Definition of done

- [ ] `account` table and `provider.account_id` exist, with the auth-mode integrity guard tested
- [ ] `AccountTransport` sends via `net.fetch({ session })`; no hand-built `Cookie` header
- [ ] No adapter file was modified by this task
- [ ] Identity probes map both vendor payloads onto one `AccountIdentity`
- [ ] Captured tokens live only in `SecretService`; no token in any DTO, row or log
- [ ] `PROVIDER_SESSION_EXPIRED` is distinct from `PROVIDER_AUTH_FAILED` and correctly produced
- [ ] Expired token re-probes identity once before declaring `needs-relink`
- [ ] Tests pass with no live vendor calls
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- Migration `0004_account.sql` — the `account` table (unique on `adapter` + `external_id`,
  `token_secret_id` → `secret` `ON DELETE SET NULL`) plus `provider.account_id`
  → `account(id)` `ON DELETE SET NULL`. Schema in `data/schema/account.ts`.
- `AccountRepository` — `create`, `getById`, `findByAdapterAndExternalId`, `listByAdapter`,
  `update`, `updateStatus`, `remove`. `remove` returns `{ removed, freedSecretId,
  detachedProviderIds }`: it nulls `provider.account_id`, sets those providers to
  `needs-relink` and drops the `secret_usage` row, so model settings survive an unlink.
- The auth-mode integrity guard lives in `ProviderRepository` (`requireAuthModeIntegrity`),
  run inside the create/update transactions. `account` mode ⇒ an `account_id` and no
  `secret_id`; `api` mode ⇒ no `account_id`. Both directions tested.
- `drivers/ai/transport/SessionGateway.ts` — the port (`partition`, `userAgent()`, `fetch`).
  Its only Electron implementation is `host/browser/sessionGateway.ts`, which calls
  `session.fromPartition(...).fetch(url, { credentials: 'include' })`. No other file imports
  `electron`, and no test does.
- `AccountTransport` — same `request`/`stream` shape as `ApiTransport`, timeout composed with
  the caller's signal, realistic `Origin`/`Referer`/`User-Agent` taken from the partition, and
  `Authorization: <tokenType> <token>` resolved per request and never cached.
- `drivers/ai/identity/` — `IdentityProbe` (shared fetch + signed-out detection), `qwen.ts`
  and `deepseek.ts` (zod schema + mapper each), `registry.ts` (keyed by family) and
  `AccountCredentialStore` (`AccountCredentials` implementation: probe → write the token
  through `SecretService`, record `token_expires_at`, flip the row to `needs-relink` on a
  dead session, and `unlink()` for the credential-lifetime decision).
- `PROVIDER_SESSION_EXPIRED` was already in `AppErrorCode`; `errors.ts` now produces it —
  `httpFailure` takes an optional `authMode` and only maps 401/403 to it for `account`, so an
  API-key 401 still yields `PROVIDER_AUTH_FAILED`. `isSignedOutResponse` catches a 3xx or an
  HTML content type, which is how both vendors signal "signed out".
- `ProviderRegistry` gained optional `accounts`, `sessions` and `credentials`, the `account`
  branch, `invalidateByAccount`, and a fingerprint that now also keys on the account row's
  `status` and `updated_at`.
- `AccountIdentity`, `ACCOUNT_FAMILIES`, `AccountFamily`, `isAccountFamily`,
  `ACCOUNT_STATUSES` and `AccountStatus` are in `@zvs/shared`. `AccountIdentity` has no token
  field and nothing puts one there.
- Tests: `apps/studio/tests/accounts.test.ts` (12 cases) over `test/helpers/FakeSession.ts`.
  No live vendor call, and one case asserts no test file imports `electron` or touches a
  cookies API.

**Deviated from plan:**

- **No explanatory comments.** The task asked for two (the `Origin`/`Referer` note in
  `AccountTransport`, the "why re-probe once" note on the retry); the session's instruction was
  to write none, so both behaviours are implemented and documented here instead. Worth adding
  back if the next session disagrees.
- The token type is not a column. Qwen's `token_type` is read and used within the probe result,
  but the row stores only `token_secret_id`/`token_expires_at`, so a resolved token is attached
  as `Bearer` (`DEFAULT_TOKEN_TYPE`). Neither vendor returns anything else today.
- `PROVIDER_STATUSES` gained `needs-relink` and `SECRET_CONSUMER_KINDS` gained `account`. Both
  are `text` columns with no CHECK, so this is a TypeScript-only, append-only widening — no
  migration.
- The guard tolerates one documented case the task's wording forbids: an `account`-mode
  provider with a null `account_id` **while its status is `needs-relink`**. That is exactly the
  state `AccountRepository.remove` leaves behind, and without the exemption every later
  `providers.update` on an unlinked row would throw.
- `zod@4.5.4` is now a direct dependency of `studio`; it was previously only reachable through
  `@zvs/shared`.
- `AccountCredentialStore` is one class per account id, constructed by the caller. TASK_015's
  `accounts.*` channels should build one per account and hand it to `ProviderRegistry` through
  the `credentials` factory.

**Next session should know:**

- **Decision (a) is in force:** unlinking deletes the captured bearer token immediately.
  `AccountCredentialStore.unlink()` implements it. The *cookie-clear* half is not wired yet —
  `BrowserViewManager`'s `clearSite`/`removeCookie` commands must call the same `unlink()` for
  the affected vendor. That belongs with TASK_015_EXTRA_1, which owns those channels.
- Nothing is wired into `main.ts`. `sessionGateways()` in `host/browser/sessionGateway.ts` is
  the factory `ProviderRegistry.sessions` expects; the registry itself still has no composition
  root because no AI service exists yet.
- Both account adapter families are still stubs, so `ProviderRegistry.driver()` on an
  account provider builds the transport and then throws `adapter … is not implemented yet`.
  The registry test asserts exactly that boundary. TASK_014_EXTRA_2 replaces it.
- No adapter file was touched. The seam held.
- The manual check in **Verify** has not been run — it needs a real signed-in session, and this
  session had no running app.
