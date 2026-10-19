# ADR 0001 — Account-mode providers: Qwen only, no DeepSeek web account

- **Status:** accepted, 2026-10-19; decisions 4–5 and the Qwen clarification superseded for
  `qwen-web` by [ADR 0002](0002-qwen-web-browser-client-emulation.md)
- **Task:** TASK_074_EXTRA_1
- **Supersedes:** the `deepseek-web` adapter family declared in TASK_014_EXTRA_2

## Context

Account mode (PASSPORT §12) lets a user chat through a vendor's own web app with their own
signed-in session: the host sends ordinary API requests from inside the work profile, with the
vendor's cookies and the bearer token the vendor page itself sends. Nothing is solved, forged
or bypassed. The request is exactly what the vendor's page sends.

Qwen works partly. Its identity and model-list endpoints answer any signed-in session. Its
chat endpoints (`POST /api/v2/chats/new`, `POST /api/v2/chat/completions?chat_id=…`) are
behind Alibaba's browser verification: they can answer with an HTML verification page
instead of JSON (recorded in `docs/QWEN_WEB.md` on 2026-09-10 and seen again on 2026-10-19).
When that happens no account-mode generation is possible.

DeepSeek is different. Every `POST /api/v0/chat/completion` on `chat.deepseek.com` must carry
an `x-ds-pow-response` header: a proof of work the client computes per request from a
challenge (`POST /api/v0/chat/create_pow_challenge`, algorithm `DeepSeekHashV1`). A stored
session alone gets no answer. The proof of work is DeepSeek's anti-automation control. The
community proxies on GitHub get past it by running a solver, which means building
code to defeat a vendor's abuse protection. We will not ship that. Account bans are reported
for this use too.

## Decision

1. `qwen-web` is the only account-mode family. `deepseek-web` is removed from the contract
   (`AdapterFamily`, `AccountFamily`), both hosts, the renderer, the account gateway's origin
   allow-list, the fixtures and the tests.
2. Migration `0002_remove_deepseek_web` deletes any `deepseek-web` account, its stored token
   secret and its providers left from earlier builds. Conversations that pointed at such a
   provider stay readable; new turns in them fail with «Text provider unavailable» until the
   user picks another provider.
3. DeepSeek models stay available through API mode: an `openai-compatible` provider with the
   base URL `https://api.deepseek.com` and an API-key secret from platform.deepseek.com
   (native tool calling), or through OpenRouter.
4. `qwen-web` stays, because it works whenever Qwen does not challenge the request. A
   verification page is reported as what it is (`PROVIDER_UNREACHABLE`,
   `reason: "browser-verification"`), never as an expired session, and we do not try to pass
   it. The reliable Qwen path is its official API: an `openai-compatible` provider over
   Alibaba Cloud Model Studio (DashScope, OpenAI-compatible mode) with an API-key secret,
   which also has native tool calling.
5. A future account family is accepted only if plain authenticated requests from the user's
   session are enough. A vendor that gates its web API behind a proof of work, a CAPTCHA or
   another anti-automation challenge is offered through its official API or not at all.

## Consequences

- The chat provider picker offers linked Qwen accounts directly («Qwen · аккаунт»). Picking
  one creates (or repairs) the account-mode provider, discovers its models and selects them.
  Whether a turn then answers depends on Qwen not challenging the request.
- Account mode keeps its known limits for Qwen: one vendor chat is created per turn and never
  deleted, history is flattened into one prompt, tools are injected as text rather than
  called natively, and the wire format can change without notice (the observations carry
  their dates).
- Re-adding DeepSeek web is a new decision, not a bug fix.

## Qwen transport clarification, 2026-10-19

The follow-up Qwen fix loads the vendor application in the existing browser profile
and uses its main JavaScript context instead of an isolated `/robots.txt` page.
Cookies alone had not reproduced the application context. This change does not add
a challenge solver. A user may complete a browser check manually and retry.
HTML alone is not evidence of verification: login pages, verification pages and
unrecognized HTML now have distinct diagnostics. The earlier statement that any
such response makes account generation unavailable is not a permanent capability
decision. See `docs/QWEN_WEB.md` for the revised behavior and validation limits.
