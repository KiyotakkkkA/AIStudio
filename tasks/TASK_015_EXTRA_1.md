---
task: TASK_015_EXTRA_1
title: Account linking service, login flow and accounts channels
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_015, TASK_014_EXTRA_2]
unblocks: [TASK_016_EXTRA_1]
estimate: 3 h
---

# TASK_015_EXTRA_1 — Account linking service, login flow and `accounts.*` channels

## Context

`AccountTransport` and the identity probes exist (TASK_014_EXTRA_1), the two account families
can list models (TASK_014_EXTRA_2), and `ProviderService` already treats account-mode
providers as ordinary providers (TASK_015). What is missing is the act of linking: the user
presses **Login**, signs into the vendor's own site, and an account appears in the app.

The flow was decided on 2026-09-10 and it is deliberately dumb, because dumb is safe here:

1. The renderer calls `accounts.link({ adapter })`.
2. The host opens the vendor's login URL as a tab in the existing `/browser` workspace —
   the same `persist:browser-work` partition, the same `BrowserViewManager` from TASK_045.
   The user is now in a normal browser tab, on the vendor's real site, typing into the
   vendor's real form. **We never render a login form, never read keystrokes, never script
   the page.**
3. The host polls that family's identity endpoint through `AccountTransport` on a modest
   interval until it succeeds, the user cancels, or a timeout expires.
4. On success it writes the `account` row, stores any bearer token through `SecretService`,
   emits `progress`/`done` events, and returns the linked identity.

Polling rather than page inspection is what keeps this on the right side of PASSPORT §21: the
only thing we observe is whether the vendor considers the session valid, which is the same
question the app asks on every request anyway.

Two states need care. **Cancellation** — the user closes the tab or navigates away, and the
poll must stop rather than run for ten minutes against a vendor. **Re-link** — an account
already exists but its session expired; linking again must update the existing row (matched on
`adapter` + `external_id`) and leave every provider pointing at it intact, so model settings
and the default model survive. Linking a *different* vendor account replaces the previous
identity for that vendor: today one browser partition means one live session per vendor, and
the UI must say so plainly rather than implying both can be active.

## Why

Without this, accounts can only be created by hand in the database. It is the feature the
Accounts tab is made of.

## Depends on

- `TASK_015` — `ProviderService`, probe union, status derivation, channels pattern.
- `TASK_014_EXTRA_1` — `account` schema, `AccountRepository`, identity probes, transport.
- `TASK_014_EXTRA_2` — model listing, so a freshly linked account has something to show.
- `TASK_045` — `BrowserViewManager` and the browser workspace.

## Read first

- `/docs/PASSPORT.md` §12 (Accounts), §21 including the 2026-09-10 session-and-identity update,
  §14 (no unprompted outbound traffic; untrusted vendor payloads).
- `apps/studio/src/host/browser/BrowserViewManager.ts` — how tabs are opened and how the
  browser workspace is shown, including the existing Qwen/DeepSeek shortcuts in Sites & cookies.
- `apps/studio/src/host/services/` — the service and channel conventions from Phase 1.

## Scope

**In scope**
- `host/services/AccountService.ts`: link, re-link, unlink, list, refresh identity
- The polling login flow, cancellable, with a timeout
- A host-side hook to open a tab in the existing browser workspace by URL
- `accounts.*` channels and DTOs
- Session-health refresh that feeds `ProviderService` status derivation
- Reaction to a vendor's cookies being cleared in Sites & cookies
- Tests using a fake identity probe and a fake browser opener

**Out of scope** (do not do these, even if tempting)
- UI (TASK_016_EXTRA_1)
- Generation over accounts (TASK_014_EXTRA_3)
- Scripting, filling or reading the vendor's login page in any way
- A second browser partition, profile switching, or more than one live session per vendor
- Storing anything the identity endpoint returned beyond the mapped `AccountIdentity` fields

## Steps

1. `AccountService` in the composition root with `AccountRepository`, `ProviderRepository`,
   `SecretService`, the identity-probe registry, a browser opener, clock, logger, event bus.
2. The browser opener is a **narrow injected interface** — `openTab(url: string): void` —
   implemented over `BrowserViewManager`. Do not give `AccountService` the manager itself; the
   AI layer has no business with tab state, and the narrow port is what makes this testable.
3. `link({ adapter })`:
   - Resolve the vendor login URL from a small per-family constant
     (`https://chat.qwen.ai/`, `https://chat.deepseek.com/`) alongside the identity endpoint,
     so a new family is one entry.
   - Show the browser workspace and open the tab.
   - Poll the identity endpoint every ~2 s with jitter, ceiling ~3 minutes, emitting
     `progress` events so the UI can show "waiting for sign-in".
   - On success: upsert the account by (`adapter`, `external_id`), store the token via
     `SecretService`, record `token_expires_at`, set status `linked`, emit `done`, return the
     `AccountDto`.
   - On timeout or cancel: write nothing, return a clear outcome the UI can render as
     "no sign-in detected".
4. Cancellation: `accounts.cancelLink({ adapter })`, and also stop automatically if the
   browser workspace is closed or the tab is gone. Every poll loop must be bound to an
   `AbortSignal` and be provably stoppable — assert it in a test, because an orphaned loop
   polling a vendor is exactly the quiet outbound traffic §14 forbids.
5. `unlink(accountId)`: null out `provider.account_id` for referencing providers, set those
   providers to `failed` with detail "no account linked", delete the token secret per the
   credential-lifetime decision recorded in TASK_014_EXTRA_1, and remove the row. **Do not**
   clear the vendor's browser cookies — the user may still want the tab signed in; say so in
   the DTO/comment so the UI can offer signing out separately.
6. `refresh(accountId)`: re-run the identity probe, update `last_checked_at`, `status`, and
   `token_expires_at`. On `PROVIDER_SESSION_EXPIRED` set `needs-relink` and let
   `ProviderService`'s existing derivation turn that into a provider status — do not write
   provider status from this service.
7. Cookie-clear reaction: when TASK_045's Sites & cookies clears a domain that belongs to a
   linked account, mark that account `needs-relink`. Wire it through an event rather than a
   direct call, so `BrowserViewManager` gains no knowledge of the AI layer.
8. DTOs and channels: `AccountDto` (id, adapter, masked email, display name, avatar, status,
   detail, `expiresAt`, `lastCheckedAt`, linked providers count) — **no token, no cookie, no
   partition secrets**. Channels: `accounts.list`, `accounts.link`, `accounts.cancelLink`,
   `accounts.unlink`, `accounts.refresh`.
9. Tests with a fake probe and fake opener: a successful poll links and stores the token; a
   timeout writes nothing; cancelling stops the loop immediately; re-linking the same identity
   updates the row and keeps providers attached, preserving `defaultModelId`; linking a
   different identity for the same vendor replaces it and reports that clearly; unlink nulls
   provider references without deleting providers; a cookie-clear event marks `needs-relink`;
   no DTO ever contains a token.
10. Commit: `TASK_015_EXTRA_1: account linking, login flow and channels`.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "After a successful sign-in, should the app close the vendor tab and switch you back to the
> Providers page automatically, or leave the browser open where you are?"
>
> **Why it matters:** automatically moving the user's view is a small thing that feels either
> helpful or rude depending on taste, and it is cheap to change now and annoying to change later.
> **Options:** (a) close the tab and return to Providers; (b) leave the tab open but switch
> back to Providers; (c) change nothing — the Accounts tab just updates in the background.
> **If the user is unavailable:** take (b) — nothing is destroyed, and the result appears where
> the user pressed the button.

## Verify

```bash
pnpm test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check, end to end: with no DeepSeek session, call `accounts.link({ adapter:
'deepseek-web' })`; the browser opens the site; sign in; the account appears with your masked
email and avatar within a few seconds. Repeat with Qwen. Then clear that vendor's cookies in
Sites & cookies and confirm the account flips to `needs-relink` and its providers report
`session-expired` rather than `auth-failed`. Finally, start a link and cancel it — confirm no
further requests to the vendor in the network log.

## Definition of done

- [x] Login opens the vendor's real site in the existing browser workspace; nothing is scripted
- [x] Polling links the account, stores the token via `SecretService`, and emits progress
- [x] Cancel and timeout both stop the loop and write nothing
- [x] Re-link updates the existing row and preserves attached providers and their settings
- [x] Unlink detaches providers without deleting them
- [x] Clearing a vendor's cookies marks the account `needs-relink`
- [x] `AccountDto` carries no token, cookie or raw vendor payload
- [x] Tests pass with no live vendor calls
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** AccountService and all five accounts channels, cancellable polling with jitter and a
three-minute ceiling, transactional account/token persistence, same-identity re-link, explicit
identity replacement, unlink, and identity refresh. Browser lifecycle events cancel linking on
tab closure/navigation or workspace closure and mark matching accounts needs-relink when cookies
are cleared. ProviderService derives expired-session results without issuing another vendor
request. DTOs project mapped identity fields and mask email; token references never cross IPC.
The narrow browser opener uses the existing partition and workspace. No code comments added.

**Deviated from plan:** Reused the existing identity probes and their SessionGateway identity
request path. Completion uses the existing step event with phase `done`, followed by `end`,
instead of introducing a new event type. Minimal AppFrame navigation handling supports the host
workspace hook; no Accounts UI was added. Used decision fallback (b): retain the vendor tab and
return to Providers after success.

**Next session should know:** `accounts.link` resolves to a discriminated result: `linked` with
`account`, `replacedPreviousIdentity`, and `detail`, or `cancelled`/`timeout` with a clear detail.
Events identify the adapter in `step.domain = accounts`; progress shares that stream ID.
Replacing a different identity detaches its providers and deletes its token; same-identity
re-link preserves provider settings and defaultModelId. `accounts.unlink` returns
`browserSessionPreserved: true`. Account expiresAt is Unix seconds; lastCheckedAt is milliseconds.
Provider status remains the existing `failed` enum with session-expired detail/probe outcome.
Validation: 226 tests passed, two opt-in live tests skipped; typecheck, lint, build, and diff check
passed. Interactive Qwen/DeepSeek sign-in was not performed and remains a manual smoke check.
