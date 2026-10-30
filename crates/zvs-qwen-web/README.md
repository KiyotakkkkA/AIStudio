# zvs-qwen-web

The chat.qwen.ai web-client protocol as the vendor's own page speaks it: request bodies, the
model list and the streamed turn decoder. The crate does no I/O. The host
(`crates/zvs-host/src/drivers/ai/adapters/qwen_web.rs`) sends the requests through the
signed-in browser session and maps the decoded events onto `TextDelta`.

Everything here was observed on the live client, not taken from a published API. Each
observation carries a date (`protocol::*_OBSERVATION`); when the client changes, re-observe,
update the code, the tests and this file together.

## Read this before touching the Qwen integration

- Changing a body, a header or a decoded field means re-checking it against the live client
  (Chrome DevTools or Claude in Chrome on `https://chat.qwen.ai/`), then updating the
  observation date in `src/protocol.rs` and the matching section below.
- Mirror what the client sends. Do not invent fields, and do not drop one the client sends
  without re-observing that it is gone.
- Parse only the fields the app uses. The payloads are large and change often; unknown fields
  are ignored on purpose.

## Endpoints

Base `https://chat.qwen.ai/api/v2` (`protocol::API_BASE`). Headers on every call:
`source: web`, `Version: <frontend build>` (`WEB_VERSION`, read from the page's script path
by `WEB_VERSION_PATTERN`), `x-request-id`, and a `referer` on the page the client would be on.

| Call       | Path                                        | Transport                                | Referer       |
| ---------- | ------------------------------------------- | ---------------------------------------- | ------------- |
| Model list | `GET /models/`                              | fetch                                    | `/`           |
| New chat   | `POST /chats/new`                           | XHR (the anti-abuse SDK instruments XHR) | `/c/new-chat` |
| Turn       | `POST /chat/completions?chat_id=<id>` (SSE) | fetch                                    | `/c/<id>`     |

`page::page_plan` tells the gateway how each request must leave the page: the transport,
the verification-dialog pattern, the `localStorage` bearer key (`token`) and the session
refresh (`https://auth.qwen.ai/api/v2/auths/refresh`).

## Model list (`models.rs`)

Rows are read from `data` or `data.data`. Only these fields are used:

| Field                                                                  | Becomes                                 |
| ---------------------------------------------------------------------- | --------------------------------------- |
| `id`                                                                   | `QwenModel::id` (trimmed, deduplicated) |
| `name`, then `info.name`                                               | `display_name`                          |
| `owned_by`                                                             | `owned_by`                              |
| `info.is_active == false`                                              | row dropped                             |
| `info.meta.max_context_length`                                         | `context_window`                        |
| `info.meta.max_generation_length`                                      | `max_output`                            |
| `info.meta.capabilities.thinking`, else `info.meta.abilities.thinking` | `thinking`                              |
| `info.meta.auto_thinking` (only when `thinking`)                       | `auto_thinking`                         |

Flags arrive as booleans or numbers (`abilities.thinking` is `1` or `3`). A model with
`auto_thinking` offers the «Авто» mode; without it the client offers only «Мышление» and
«Быстрый». Observed 2026-10-26: `qwen3.7-plus`, `qwen3.8-max`, `qwen3.6-plus` and
`qwen3.5-plus` carry `auto_thinking: true`; `qwen3.7-max` and `qwen3.8-omni-flash` do not.

The host maps `thinking` to `ModelCapability::Reasoning` and `auto_thinking` to
`ModelCapability::AdaptiveReasoning`.

## New chat (`request::new_chat_body`)

```json
{
  "chatId": "",
  "chat_mode": "normal",
  "chat_type": "t2t",
  "models": ["<model>"],
  "project_id": "",
  "timestamp": 1793026435555
}
```

`chat_type` is `t2t`, or `search` when web search is on (`ChatType`). `timestamp` is in
milliseconds.

## Turn (`request::CompletionTurn`)

Top-level keys, in the client's order: `stream`, `version` (`2.1`), `incremental_output`,
`chatId`, `parentId` (`""` on a first turn), `chat_id`, `chat_mode`, `model`, `parent_id`
(`null` on a first turn), `messages`, `timestamp` (seconds). There is no top-level
`chat_type`. Follow-up turns hang off the previous answer's `response_id`.

The single user message carries `chat_type`, `sub_chat_type` and
`extra.meta.subChatType`, all equal to the chat type, and `feature_config`:

| Mode («…» in the client) | `thinking_mode` | `thinking_enabled` | `auto_thinking` | `thinking_format` |
| ------------------------ | --------------- | ------------------ | --------------- | ----------------- |
| «Авто»                   | `Auto`          | `true`             | `true`          | `summary`         |
| «Мышление»               | `Thinking`      | `true`             | `false`         | `summary`         |
| «Быстрый»                | `Fast`          | `false`            | `false`         | absent            |

Always present: `output_schema: "phase"`, `research_mode: "normal"`, `auto_search: true`.
The client sends `auto_search: true` for every chat type; the crate does the same and never
exposes it.

The host picks the mode: `Fast` for a model without `Reasoning`; the requested mode
otherwise, with «Авто» sent as `Thinking` to a model without `AdaptiveReasoning`.

Deep research (`chat_type: deep_research`, `sub_chat_type: deep_thinking`,
`research_mode: normal | advanced`) and the other `+` menu modes are observed but not
implemented.

## Stream (`stream::TurnDecoder`)

`data:` frames. The first is `response.created` with `response_id` and `response_index`; only
the primary response (`response_index` `0`) is followed. Then `choices[0].delta` frames, each
with a `phase` and a `status` (`typing` or `finished`):

| `phase`                  | Read                                                                     | Event                                    |
| ------------------------ | ------------------------------------------------------------------------ | ---------------------------------------- |
| `thinking_summary`       | `extra.summary_thought.content[]` (whole snapshots)                      | `Reasoning` (new suffixes only)          |
| `answer`                 | `content`                                                                | `Text`; `Finished` on `status: finished` |
| `web_search`, `finished` | `extra.web_search_info[]`: `url`, `title`, `snippet`, `date`, `hostlogo` | `WebSources`                             |

`usage` on any frame becomes `Usage`. Error frames (`success: false`, `error`, `status >= 400`)
become `Failure`, flagged `auth` for 401, 403 and session-like codes. Vendor messages are never
kept.

With web search on, the model may write a short answer, finish it, search, and then answer
again: `answer: finished` can arrive more than once, so it marks the turn complete but does
not end the stream. Search results arrive only on the `finished` `web_search` frame (the
`typing` frames carry the query in `function_call.arguments`). The answer cites them as
`[[n]]`, 1-based over the results in arrival order. Rows without an `http(s)` URL are
skipped; an empty title falls back to the URL; empty `snippet` and `date` are dropped.
`hostlogo` becomes the source's `icon` when it is an `http(s)` URL; rows also carry `hostname`,
which is not read. Both keys are present in the frames recorded 2026-10-26; `hostlogo` was empty
in those recordings, so the app falls back to the site's own `/favicon.ico` (2026-10-27).
`web_extractor` phases (pages the model read) are not decoded.

## Calls in the vendor's format (`TurnEvent::ToolCall`)

Observed 2026-10-29 (`protocol::NATIVE_CALL_OBSERVATION`, FE `0.3.22`). The app offers its tools
in the prompt (the host's injected dialect). The model usually answers with a
`{"zvs_call_tool": …}` object in `content`, but it may instead call a tool in its own format,
and the vendor then tries to run that tool on its side:

| Frame (all `phase: answer`)                                 | Meaning                                     |
| ----------------------------------------------------------- | ------------------------------------------- |
| `content: ""`, `function_call: { name, arguments }`         | the call; `arguments` is a growing snapshot |
| `content: "Tool <name> does not exists."`, `name: "<name>"` | the vendor's reply to a call it cannot run  |
| `status: finished`, then more `content`                     | the model's reply to that refusal           |

`arguments` is a string. It starts at `""` and each frame repeats the whole value so far,
sometimes twice. A frame with another `name`, or with arguments that do not extend the current
snapshot, starts the next call. The first frame of a call may carry
`extra.display_position: think`. There is no `function_id` on these frames, unlike
`web_search`.

The decoder emits each finished call as `ToolCall`, then `CallsEnded` on the first answer frame
that is not a call (the vendor's reply). After that it decodes nothing more: the vendor's reply
is vendor text, and the rest is the model answering a refusal the app never made. A stream that
ends during a call is handed over by `TurnDecoder::end`. The host passes the calls to the chat
loop, stops reading the response, and does not chain the next turn onto it.

## Vendor-side tools

`EXCLUDED_POLICY_CHECK_TOOLS_NAMES` (now empty) lists tools that Qwen runs itself. When
the model emits a call with one of these names and the app did not offer a tool by that name,
the chat loop drops the call with a log line instead of refusing it as unoffered: the search
already ran on the vendor's side and its results arrive as `WebSources`. `zvs-zai-web` exposes
the same constant, empty for now.

## The host adapter

The adapter generates and streams text over the linked browser account. Requests run inside
the hidden gateway webview of the browser's work profile (PASSPORT D10), so the vendor's
cookies are never copied out. It is an unofficial web-protocol integration, not the Qwen OAuth
or Qwen Code API.

- Discovery advertises only `Streaming`, `Reasoning` and `AdaptiveReasoning`. Vision, files,
  audio, video and image generation are not exposed; turns carry text with an empty `files`.
  The adapter capability `image` means image generation through `ImageDriver`, not vision
  input. Refreshing the provider's model list updates stored capability badges.
- The model list is cached for an hour (`MODELS_TTL_MS`). A stale list is kept when a refresh
  fails for any reason but a cancellation or a session expiry. A model missing from the list
  gets `Fast`.
- Malformed rows are skipped and logged by count while valid rows stay. Invalid JSON, an error
  envelope or an unusable outer structure fails the list. Diagnostics carry the status, the
  content type, the JSON shape, whitelisted field names and counts, never the body.
- The first turn of a dialog creates a chat and sends the dialog flattened into one prompt:
  a lone user message as is, otherwise role-labelled blocks (`System: …`, `User: …`,
  `Assistant: …`). A follow-up whose earlier dialog matches a remembered answer continues that
  chat with the answer's `response_id` as parent and sends only the new messages. Up to 256
  chains are kept (`CHAINS_MAX`), in memory only. A chained turn refused before any delta
  (anything but a cancellation, a session expiry, `PROVIDER_UNREACHABLE` or `RATE_LIMITED`)
  forgets that chat and is retried once in a new one. A session expiry forgets every chain.
- Chats are created one at a time per adapter.
- Qwen providers default to a 600-second timeout, since thinking turns can take over a minute
  before the first token; an explicit provider timeout wins.

## Browser verification and HTML answers

A capture of FE `0.3.20` on 2026-10-19 showed `chats/new` sent over XHR with the Baxia headers
`bx-ua`, `bx-umidtoken` and `bx-v`. The same request over `fetch` without them got the
«Access Verification» slider, which leaves a hidden gateway request hanging. Hence the XHR
transport above (decision: `docs/adr/0002-chat-ai-web-browser-client-emulation.md`). The page
loads the app root and calls the app's own `fetch`/XHR in the main frame, so the vendor's hooks
run; local fixture pages use an isolated context. Those headers are never read, generated or
logged by the app.

An HTML answer where JSON is expected is classified from the page: verification markers give
`browser-verification`, a login page `login-required`, anything else `unexpected-html`. An
HTML 403 does not refresh the credential. A verification shows the gateway window to the
person (see the gateway README); nothing solves it automatically. Logs record the request and
final URL paths without query strings, so `/chats/new` and `/chat/completions` can be told
apart.

## Session and bearer

Requests with `Authorization` carry the client's own `localStorage.token`; the page substitutes
it before sending.

Rejected bearer, observed 2026-10-19. Qwen answers a bearer it no longer accepts with HTTP 200
and `{"success": false, "data": {"code": "unauthorized", …}}`, not 401. `/models/` answers that
envelope for any stale or foreign bearer and the list for no bearer at all; `GET /api/v1/auths/`
on the cookie session mints a fresh token. `AccountTransport` recognises the envelope
(`errors::auth_envelope_code`), also on a streamed call, and recovers in logged steps: refresh
and retry, then retry on the cookie session alone, then `PROVIDER_SESSION_EXPIRED` with
`reason: "token-rejected"`. Envelopes with other codes become «Qwen отклонил запрос списка
моделей» with `vendorCode`.

Renewal, observed in FE `0.3.20` on 2026-10-20:

- `localStorage.token` is an access token living about 15 minutes. The client keeps
  `at_expire_time` (ms) and `qwen_access_token_state` (`version`, `stateId`, `token`,
  `expiresAt`, `updatedAt`, `sourceId`) next to it.
- It renews with `GET https://auth.qwen.ai/api/v2/auths/refresh` (with credentials; the
  `.qwen.ai` cookies `refresh_token` and `auth-session`), reads `data.access_token` and
  `data.expires_at` (seconds), persists the state and posts `TOKEN_UPDATED` on the
  `BroadcastChannel` `qwen-auth-token`. Without `x-request-origin: chat.qwen.ai` the auth host
  answers `Bad_Request`; a dead session answers `Unauthorized`.
- The client renews only around its own requests, so the gateway renews the same way
  (`SESSION_REFRESH`): with less than a minute left (`leeway_ms`) before a request that carries
  a bearer, it refreshes, persists the state and announces it. Concurrent renewals share one
  call, and the token never leaves the page.
- `SessionGateway::renew` runs the refresh on demand after a refusal, logging status,
  `success`, `data.code`, `data.details` and field names, never values. Identity probes that
  end in a session expiry renew once and probe again.
- A refresh refused with one of `SESSION_REFUSALS` (`unauthorized`, `invalid token`,
  `forbidden`) is a session expiry with `reason: "refresh-revoked"`. Linking and the account
  check confirm the session renews after the identity probe passes: `/api/v1/auths/` still
  accepts the cookies of a session whose refresh token is revoked.
- An account the app marked `needs-relink` for an expired session
  (`AccountRow::session_lapsed`) keeps its transport and is re-checked after startup and every
  15 minutes. A different identity, cleared cookies or a revoked refresh token still need a
  re-link.
- Rejected: reopening the gateway page so the app renews on load. The app kept its rejected
  bearer and each reload started a new verification.
- Not covered: an `unauthorized` frame inside a stream that is already answering ends the turn
  as an expired session.

## References

Compared on 2026-10-18 with
[Rfym21/Qwen2API](https://github.com/Rfym21/Qwen2API/tree/1b38a501c833a147d3ee3fc6317d1bc2a87d6482)
and its
[FE 0.3.19 change](https://github.com/Rfym21/Qwen2API/commit/32b6bf0e7ed0ebe0f55c723a7b95859595195023)
(source of the `source`/`Version` headers, the referers, `chat_mode: normal` and the 10-minute
timeout), and with
[gpt4free's Qwen provider](https://github.com/xtekky/gpt4free/blob/main/g4f/Provider/Qwen.py).
These are unofficial proxies, not a Qwen API contract; the Qwen OAuth/device flow they mention is
a separate integration.

## Tests

`cargo test -p zvs-qwen-web` covers the bodies, the model list and the decoder. The host's
adapter tests (`qwen_web/tests.rs`) replay recorded streams from
`test/fixtures/providers/qwen*.{json,sse}`; record a new fixture when the stream shape changes.

The live check is opt-in: set `LIVE_QWEN_TOKEN` to a captured bearer and run
`cargo test -p zvs-host -- --ignored live_qwen`. It checks identity and discovery, then one
short generation each for a thinking and a fast model, so it can create two vendor chats. It
prints statuses and counts, never credentials or bodies. It calls the API directly without the
profile's cookies, so a verification can block it even with a valid token.
