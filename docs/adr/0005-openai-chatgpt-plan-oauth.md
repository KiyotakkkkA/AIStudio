# ADR 0005 — OpenAI ChatGPT-plan OAuth

Accepted, 2026-10-30.

The `openai` account family uses OpenAI's documented public-client OAuth flow with PKCE,
OIDC nonce and state validation, verified ID tokens, a stable per-installation host ID, and
dynamic client registration. It is an exception to the browser-session transport described
in PASSPORT §12 and ADR 0001: browser cookies are not used for OpenAI inference.

Credentials stay in the existing encrypted secret store. The renderer receives masked
account metadata. Registrations are keyed by issued client ID and verified subject, so two
workspaces sharing an email remain distinct. Reauthorization uses the selected registration.
Refreshes are serialized. Disconnect attempts remote revocation before removing credentials.
Clearing embedded-browser cookies does not invalidate these OAuth registrations.
An OpenAI provider whose account was removed requires an explicit account selection; it
never adopts another registration automatically.

The composer persists `openaiMode` on the provider: `direct` uses Responses HTTP/SSE;
`codex` uses an installed Codex app-server with the same selected OAuth grant. This switch
selects transport and execution integration, not a separately guaranteed quota or billing
plan. Codex runs in an isolated temporary home and ephemeral thread with environment access
disabled. ZVS retains tool execution and permission decisions.

`providers.usage` is a nullable, provider-independent usage contract. Drivers without usage
support return null. OpenAI returns authoritative windows when available and an explanatory
unavailable state otherwise. The app does not invent reset times or infer subscription
usage from local token totals. `accounts.openUsage` opens only the fixed OpenAI usage page.

No changes are made to API-key providers or to the CLI pipe's channel allow-list.
