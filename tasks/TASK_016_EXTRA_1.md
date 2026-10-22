---
task: TASK_016_EXTRA_1
title: Accounts tab — login, linked accounts, session health
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_016, TASK_015_EXTRA_1]
unblocks: []
estimate: 3 h
---

# TASK_016_EXTRA_1 — Accounts tab: login, linked accounts, session health

## Context

The Providers page has three capability tabs — Text Generation, Embeddings, Image Generation —
which are three filters over one table. This task adds a fourth tab that is **not** a
capability filter: **Accounts**. It lists vendor accounts linked through the browser session
(PASSPORT §12 "Accounts", §21) and is the only place in the app where linking happens.

Its centre is one button. **Login** picks a vendor, opens that vendor's real site in the
integrated browser, and waits. When the host's identity poll succeeds, an account card appears
with the avatar, display name and masked email the vendor returned — nothing more. There is no
login form in our UI, and there must never be one: the user types their password into the
vendor's own page, in a normal browser tab, exactly as they would outside the app.

The tab is small, and most of the work is honesty about state. An account is `linked`,
`needs-relink`, or approaching expiry — the last one reusing the same amber treatment the
mockup already gives "Token expires in 12d" for rotating secrets, because to the user it is
the same idea. And because one browser partition means one live session per vendor today, the
UI must say that plainly when someone links a second Qwen account, rather than implying both
stay active.

## Why

Without it, account linking exists only as a channel call, and an account-mode provider can
never be configured by a user.

## Depends on

- `TASK_016` — the Providers page, tab row, `ProviderStore`, the promoted `ui/` components.
- `TASK_015_EXTRA_1` — `accounts.*` channels, link/unlink/refresh, progress events.

## Read first

- `/design/Providers.dc.html` — the tab row, status dots, the amber expiry row, and the card
  and list styling to reuse. There is no Accounts artboard; build it from these parts rather
  than inventing a new visual language.
- `/design/Foundations.dc.html` — avatar, chip and button specimens.
- `/docs/PASSPORT.md` §12 (Accounts), §13 (renderer rules), §21.

## Scope

**In scope**
- `features/providers/AccountsTab.tsx` and an `AccountStore`
- Login flow UI: vendor choice, waiting state with cancel, success, timeout
- Account cards: avatar, display name, masked email, status, expiry, provider count
- Re-link and Unlink, with confirmation for unlink
- Empty state explaining what account mode is and which vendors support it
- Tests for the store and the waiting/cancel states

**Out of scope** (do not do these, even if tempting)
- Any credential input in our UI — no password field, ever
- Showing cookies or tokens; the DTO does not carry them and the UI must not ask
- Signing the user out of the vendor's site — that is the browser's Sites & cookies
- New visual components beyond what `ui/` already has
- A vendor picker that lists families with no adapter — read what the host reports

## Steps

1. `features/providers/AccountStore.ts` — MobX. State: `accounts`, `linking` (adapter + phase:
   `waiting` | `success` | `timeout` | `cancelled`), `error`. Actions: `load()`,
   `link(adapter)`, `cancelLink()`, `unlink(id)`, `refresh(id)`. Subscribe to the link
   `progress` events from TASK_015_EXTRA_1 rather than polling from the renderer.
2. Add `Accounts` to the Providers tab row. It carries a count like the others but is not a
   capability filter — keep that distinction in the code, and do not let the capability type
   grow an `accounts` member.
3. Login: a small vendor menu on the button (Qwen, DeepSeek — sourced from what the host
   reports as available account families, never hard-coded in the renderer). Pressing it calls
   `accounts.link`, and the tab shows a waiting panel: "Sign in to Qwen in the browser…", a
   spinner, and a **Cancel** button that calls `accounts.cancelLink`. The browser workspace is
   already showing the vendor's page at this point — the panel explains, it does not block.
4. Waiting outcomes: success renders the new card; timeout renders "No sign-in detected" with
   a Try again button; cancel returns silently to the list. All three must be reachable in a
   test.
5. Account card: avatar (may be a `data:` URI — render it, do not fetch it), display name,
   masked email exactly as stored, status dot with detail, expiry line when the account has
   `expiresAt` (amber inside the warning window, matching the mockup's rotation row), the
   number of providers using it, and Re-link / Unlink actions. `needs-relink` promotes
   Re-link to the primary action.
6. Unlink confirmation names what happens: providers using this account stop working and show
   "no account linked", their model settings are kept, and the vendor's browser session is
   *not* signed out — with a link to Sites & cookies for someone who wants that too.
7. Second-account-for-the-same-vendor: before linking, if an account for that adapter already
   exists, say so in the confirm step — "signing in as someone else will replace the linked
   Qwen account; only one session per vendor is supported today".
8. Empty state: one short paragraph on what account mode is (sign in to the vendor's site;
   the app uses that session; no API key needed), the supported vendors, and the Login button.
9. States and checks: loading skeleton; error from the channel rendered without a stack; the
   tab at 1440×900 and 1100 px.
10. Tests: store transitions across waiting → success / timeout / cancelled; cancel calls the
    channel; unlink confirms before calling; a `needs-relink` account promotes Re-link; no test
    or component references a token or cookie field. Presentational cards need no snapshots.
11. Commit: `TASK_016_EXTRA_1: accounts tab`.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks, both vendors:
- Press Login → DeepSeek with no session: the browser opens the site, the waiting panel shows,
  you sign in, and the card appears with your masked email and avatar.
- Press Cancel mid-wait: the panel closes and no further vendor requests are made.
- Clear that vendor's cookies in Sites & cookies: the card flips to `needs-relink` and any
  provider using it reports session expired on the other tabs.
- Re-link: the same card updates in place, and the provider's model settings and default model
  are still there.
- Unlink: the confirmation names the affected providers; after confirming, those providers
  still exist and read "no account linked".

## Definition of done

- [x] Accounts tab lists linked accounts with avatar, masked email, status and expiry
- [x] Login opens the vendor's own site; no credential input exists anywhere in our UI
- [x] Waiting, success, timeout and cancel are all reachable and correct
- [x] Re-link preserves attached providers and their settings; unlink detaches without deleting
- [x] Replacing a vendor's account is explained before it happens
- [x] Nothing in the renderer touches a token, cookie or partition
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- `features/providers/AccountStore.ts` — MobX store over `accounts.*`: `accounts`, `families`,
  `linking` (`adapter` + phase `waiting` | `success` | `timeout` | `cancelled` + the elapsed and
  total milliseconds the host reports), `error`, plus `load`, `link`, `cancelLink`, `unlink`,
  `refresh`, `dismissLinking`, `dismissError`. Wired into `RootStore` as `accounts`.
- `features/providers/providerTabs.ts` — `ProvidersTab = ProviderCapability | "accounts"`.
  `ProviderCapability` itself is untouched; `ProviderStore.tab` holds the visible tab and
  `requestTab` routes a capability tab through the existing `requestCapability` guard.
- `features/providers/AccountsTab.tsx`, `AccountCard.tsx`, `AccountLoginMenu.tsx` and
  `accountPresentation.ts`. The page renders the workspace or the accounts tab from
  `providers.tab`, and swaps its header actions for the Login menu on the accounts tab.
- `EventRouter.observe(handler)` — a subscription to every routed event, since a link stream's
  id is opened inside `AccountService` and never reaches the renderer.

**Deviated from plan:**

- Step 1 asks the store to subscribe to link progress "rather than polling". The renderer does
  not poll, but it also cannot subscribe by stream id: `accounts.link` resolves with the final
  `AccountLinkResult` and never hands back the `streamId` its progress rides on. The store
  therefore awaits `accounts.link` for the outcome and uses the new `EventRouter.observe` for
  the progress bar, latching the stream id from the first `{ domain: "accounts" }` step event so
  another domain's progress cannot move the waiting panel.
- The card's secondary action splits in two rather than being one Re-link button. A
  `needs-relink` account promotes **Перепривязать**, which calls `accounts.link` — signing in
  again is what actually recovers a dead session, and `AccountService.#save` matches on
  `externalId`, so the row and its providers survive. A healthy account instead offers
  **Проверить сессию** (`accounts.refresh`), which only re-probes.
- "Сайты и куки" is a plain link to `/browser`; TASK_045's cookie screen has no route of its own
  yet.
- The vendor menu is a small absolutely-positioned list built from existing atoms, not the
  uikit `Dropdown` — that one drives its popup through the Popover API, which jsdom does not
  implement, and the menu had to be reachable in a test.

**Next session should know:**

- The Accounts tab count comes from `AccountStore`, so `ProvidersPage` loads accounts on mount
  even when another tab is showing.
- Switching to Accounts deliberately skips the unsaved-changes guard: the form lives in
  `ProviderStore`, so it survives the tab being unmounted. Only capability switches discard it.
- `ProvidersWorkspace` reloads providers whenever it remounts, which is what keeps the account
  picker in the connection form fresh after a link or unlink — no cross-store call was added.
- The app itself was not launched (Electron): the tab is covered by renderer tests and the
  1440/1100 rules were written against the mockup, not verified on screen. The manual checks in
  Verify still need a real Qwen/DeepSeek sign-in.
