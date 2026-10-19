# Qwen web adapter

The adapter implements text generation and streaming over the linked browser
account session. Requests run as `fetch()` inside a hidden webview of the browser's work
profile (PASSPORT D10), so the vendor's cookies are never copied out. It is an unofficial web
protocol integration, not the Qwen OAuth or Qwen Code API.

## Model capabilities and thinking

Discovery advertises only `streaming` and, when supported, `reasoning`. Vendor
vision, file, audio, video, search and image-generation abilities are not exposed:
the adapter currently sends text with an empty `files` array. The shared adapter
capability `image` means image generation through `ImageDriver`, not vision input.
Refresh the provider's model list to update previously stored capability badges.

Generation obtains the model list on first use and reuses it for that adapter
instance. An explicit list refresh replaces the cache. Malformed individual model
records are skipped while valid records are retained. A model with reasoning
enabled uses `Thinking`; other models, including unknown models, use `Fast`.
An explicit `capabilities.thinking` value takes precedence over legacy
`abilities.thinking`, so `false` or `0` cannot be overridden by an ability value.
Missing capability metadata falls back to legacy abilities; missing both disables
thinking. Authentication, transport and timeout errors still fail generation.
Malformed individual model records are skipped and logged by count, leaving valid
models available; an affected or unknown model uses `Fast`. Invalid JSON, an
explicit error envelope or an unsupported outer model-list structure still fails.
Unrecognized capability flag values are treated as disabled rather than making an
otherwise usable model record invalid.

## Conversation history

Every generation creates a new Qwen chat. A single user message is sent directly;
system instructions and conversation history are serialized as role-labelled text.
This preserves the supplied context as text but does not provide native vendor
conversation continuity or enforce native system/assistant roles. Vendor chat,
parent and response identifiers are not reused across generations.

## Validation on 2026-09-10

Fixture tests cover normal generation, reasoning and usage, cancellation,
truncation, malformed replies, and session expiration preserving partial text.
Standalone SSE `status: 401` and `status: 403`, authentication codes, and string
codes are recognized without requiring an `error` or `success: false` field.

Authenticated live checks accepted the provided credential at `/api/v1/auths/`
and returned six models from `/api/v2/models/`. Chat creation returned HTTP 200
with HTML containing browser-verification/WAF markers instead of JSON. Adding
the public implementation's request headers did not change this. Headless browser
attempts did not produce a usable completion capture. Consequently, live
generation and a fresh browser payload comparison remain unverified. At that time
the existing `local` chat mode, `2.1` version and phase output schema were retained;
the later compatibility sync below records the evidence for changing chat mode.

## Browser transport correction on 2026-10-19

The previous gateway fetched from `/robots.txt` in an isolated JavaScript world.
It shared cookies with the work profile, but did not load the Qwen application or
use its page-level request hooks. Qwen now loads its application root in that same
profile and calls fetch in the main frame's default JavaScript context. The Tauri
host bridge remains removed; the CDP binding carries response messages only.
Local fixture pages retain the isolated context.

Response metadata preserves the final URL and redirect flag internally. Diagnostics
omit URL query strings and response bodies. HTML is classified using page evidence:
known verification markers produce `browser-verification`, a recognized login page
produces `login-required`, and unrecognized HTML produces `unexpected-html`.
An HTML 403 does not trigger a credential refresh. A verification error asks the
user to open the account page, complete the check and retry; it does not claim that
account generation is permanently unavailable. No CAPTCHA solver is included.

A successful identity probe alone does not establish generation access. The new
transport still requires a live generation check with the user's browser profile;
fixture tests do not demonstrate acceptance by Qwen's current protection.

## Web-client emulation on 2026-10-19

Decision: [ADR 0002](adr/0002-qwen-web-browser-client-emulation.md). A live capture of the
client (FE `0.3.20`) showed `chats/new` sent over XHR carrying the Baxia headers `bx-ua`,
`bx-umidtoken` and `bx-v`; the same request sent with `fetch` and without them was answered
with the «Access Verification» slider, which leaves a hidden gateway request hanging.

- The protocol lives in `crates/zvs-qwen-web` (no I/O): constants and dated observations,
  model-list decoding and diagnostics, request bodies, the SSE turn decoder and the page
  request plan. `zvs-host` keeps the adapter glue; the gateway asks the crate how to send.
- `chats/new` goes through the page's `XMLHttpRequest`, which the vendor's SDK instruments;
  completions and models through `fetch`. Tokens are never read, generated or logged by us.
- Chats are created one at a time per adapter.
- A verification shows the gateway window to the user (see the gateway README); nothing
  solves it automatically.
- Requests with `Authorization` carry the client's own `localStorage.token`, which the
  client renews through `/api/v2/auths/refresh`; the page substitutes it before sending.
- Verified live on 2026-10-19: models, `chats/new` (with the SDK headers) and a streamed
  completion succeed through the gateway, a verification included.

## Rejected bearer in a 200 answer, 2026-10-19

Qwen rejects a bearer it no longer accepts with HTTP 200 and
`{"success": false, "data": {"code": "unauthorized", "details": …}}`, not with 401, so the
transport's 401/403 refresh never ran and the adapter reported an unknown model-list format.
Observed in the browser: `/api/v2/models/` answers that envelope for any stale or foreign
bearer, and the model list for no bearer at all (the cookie session suffices);
`GET /api/v1/auths/` on the cookie session mints a fresh 30-day token.

`AccountTransport` now recognises such an envelope (`errors::auth_envelope_code`) and
recovers in steps, logging each: refresh the token and retry; then retry on the cookie
session alone; then fail with `PROVIDER_SESSION_EXPIRED`, `reason: "token-rejected"`.
A JSON answer to a streamed call goes through the same steps. Envelopes with other codes
are reported as «Qwen отклонил запрос списка моделей» with `vendorCode`.

## Upstream compatibility sync on 2026-10-18

Compared the adapter with the then-current [Rfym21/Qwen2API request implementation](https://github.com/Rfym21/Qwen2API/tree/1b38a501c833a147d3ee3fc6317d1bc2a87d6482)
and its [FE 0.3.19 compatibility change](https://github.com/Rfym21/Qwen2API/commit/32b6bf0e7ed0ebe0f55c723a7b95859595195023).
The adapter now sends the observed `Version: 0.3.19` and `source: web` headers,
uses the app routes as the request referer, and creates chats with `chat_mode: normal`.
The Qwen `Authorization` bearer is still supplied by the existing account transport;
the browser profile continues to supply cookies.

That implementation also allows ten minutes for streamed Thinking/Max generations,
noting that they may take longer than a minute before producing an answer. Qwen web
providers therefore default to a 600-second timeout; an explicitly configured
provider timeout still takes precedence. These are observations from an unofficial
proxy, not a Qwen API contract. Recheck the frontend version and payload when Qwen
updates its web client, and validate generation in a live browser profile.

Malformed-model diagnostics include the HTTP status, content type, JSON shape,
whitelisted field names and counts; they never log the response body. Model
records with an unsupported shape are skipped individually. An outer error
envelope, invalid response or unusable model-list structure stops generation
before chat creation. Individual malformed rows are ignored only when the model
list still contains usable records.

An HTTP 200 HTML response from a later endpoint is a separate failure, commonly
an application/login/verification page returned in place of JSON. The account
endpoint warning now records request and final URL paths (without query strings)
so logs can distinguish `/chats/new` from `/chat/completions`. Gateway page-load
failures retain the WebView2 error code and sanitized page path. Neither the
Fast-mode fallback nor the know-how note on injected tool calling fixes these
browser/session/API failures.

The reviewed [public Qwen implementation](https://github.com/xtekky/gpt4free/blob/main/g4f/Provider/Qwen.py)
also uses dynamic browser state. Its OAuth/device flow is a separate integration;
it is not evidence that the current web identity endpoint should be replaced.

## Repeatable live test

Set `LIVE_QWEN_TOKEN` in the process environment, then run:

```sh
pnpm exec vitest run apps/studio/tests/liveQwen.test.ts
```

This opt-in test checks identity and discovery, then requests one short generation
for each of the Thinking and Fast model categories. It can create two vendor
chats. It prints endpoint status and counts, never credentials or response bodies.
It uses direct fetch without the browser profile's cookies, so browser verification
can block it even with a valid token. After TASK_067 this test is an `#[ignore]`d Rust test
(`cargo test -p zvs-host -- --ignored live_qwen`) with the same behaviour. Without the environment variable it is skipped.
