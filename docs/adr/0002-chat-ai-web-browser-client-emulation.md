# ADR 0002 — Qwen account mode emulates the web client, Baxia tokens included

- **Status:** accepted, 2026-10-19
- **Task:** TASK_074 follow-up (Qwen account generation)
- **Supersedes:** ADR 0001, decisions 4 and 5 and the «Qwen transport clarification»
  section, for the `qwen-web` family only. The DeepSeek decisions of ADR 0001 stand.

## Context

Qwen account mode must work: a linked Qwen account is a first-class text provider, not
a fallback to the official API.

A live capture on 2026-10-19 (Qwen web client, FE `Version: 0.3.20`, signed-in profile)
showed why our gateway fails while the web client succeeds:

- The client sends `POST /api/v2/chats/new` through its own HTTP client with the Alibaba
  Baxia headers `bx-ua` (~1.7 KB), `bx-umidtoken` and `bx-v` (`2.5.37`), plus
  `x-request-id`, `timezone`, `source: web`, `Version` and the bearer token. The body
  matches ours (`chat_mode: normal`, `chat_type: t2t`).
- The tokens come from the Baxia SDK loaded by the page itself
  (`window.__baxia__.getFYModule.getFYToken()` / `getUidToken()`), fresh for each request.
- The same request sent from the page's main context **without** `bx-*` was punished by the
  WAF: the page's Baxia hook opened an «Access Verification» slider and the `fetch` never
  settled. In our hidden gateway webview nobody sees that dialog, so the turn hangs until
  the timeout or comes back as HTML.
- `POST /api/v2/chat/completions` was sent by the client through `fetch` without `bx-*`
  headers at the application level.

Cookies, the bearer token and a correct body are therefore not enough. The request has to
carry the tokens the vendor's own SDK produces in the user's own browser profile.

## Decision

1. **`qwen-web` emulates the Qwen web client.** Every call is sent from the main context of
   the loaded Qwen application in the user's work profile, through the same page API the
   client uses for it: `chats/new` over the page's `XMLHttpRequest`, completions and the
   model list over its `fetch`. The vendor's Baxia SDK instruments those APIs in the page and
   attaches its own per-request tokens (`bx-ua`, `bx-umidtoken`, `bx-v`) exactly as it does
   for the client. The adapter adds the client's `source: web`, `Version` and referer.
   The bearer is the one the client keeps in `localStorage.token` and renews through its own
   session refresh (`/api/v2/auths/refresh`): the page puts it on every request that carries
   `Authorization`, so it never leaves the page. A request the vendor rejects inside a 2xx
   answer (`{"success": false, "data": {"code": "unauthorized"}}`) is retried with a refreshed
   token, then on the cookie session alone, then fails as an expired session.
2. **Tokens are produced only by the vendor's SDK running in the user's browser.** We do not
   call into, reimplement, forge or replay Baxia token generation, do not share tokens
   between profiles or accounts, and never see or log them.
3. **CAPTCHAs are completed by the user, never solved by us.** While a request waits for its
   response, the page watches for the vendor's verification dialog. When it appears, or the
   request stays silent for 10 s, the hidden gateway window is shown so the user can complete
   the check; the vendor's SDK then releases the original request, the turn continues and the
   window hides again. After 5 minutes without an answer the turn fails with
   `PROVIDER_UNREACHABLE`, `reason: "browser-verification"`. Closing the window only hides it.
   No solver, no automated slider interaction.
4. **Traffic stays client-like.** One gateway page per profile, protected calls serialized per
   account, no parallel `chats/new` bursts, no retries in a loop after a challenge.
5. **The Qwen protocol lives in its own crate** (see `crates/zvs-qwen-web/README.md`): wire types, request
   builders, SSE parsing and the page request plan (page API, dialog pattern, bearer key),
   versioned with the date of the capture it reflects, so it can be updated without touching
   the host.
6. The official API (`openai-compatible` over DashScope) remains available but is not offered
   as the answer to a Qwen account failure.

## Consequences

- Qwen account generation depends on the vendor's SDK and frontend contract, which change
  without notice. Every change of `Version`, `bx-v` or the payload is a protocol update in the
  Qwen crate, re-validated by a live capture.
- Using a consumer web client programmatically may conflict with the vendor's terms; the
  account owner accepts that risk when linking the account. Challenges are surfaced, not hidden.
- The account gateway gains a «show verification» path (the hidden window becomes visible
  while a verification is pending) and a per-origin request plan (`zvs_qwen_web::page_plan`):
  page API and dialog pattern.
- Verified live on 2026-10-19: the SDK attaches `bx-ua`, `bx-umidtoken` and `bx-v` to the
  gateway's `chats/new` XHR, a shown verification releases the request once passed, and a
  full turn streams with the page bearer. Tokens minted by `GET /api/v1/auths/` from a stale
  stored bearer are rejected by the v2 API, which is why the page bearer is used.
- Account-mode providers never wait to be reattached by hand. Linking an account binds the
  family's providers that have no account (`AccountRepository::attach_orphans`), and the
  provider registry binds such a provider to the family's only linked account on first use.
  A replaced identity therefore moves its providers to the new account.
- Rule 5 of ADR 0001 («a vendor behind an anti-automation challenge is API-only») no longer
  applies to Qwen. It still applies to any other vendor, and in particular DeepSeek's
  proof of work stays out: computing a vendor's challenge ourselves remains excluded.
