# zvs-zai-web

The chat.z.ai web-client protocol as the vendor's own page speaks it: request signing, request
bodies, the model list and the streamed turn decoder. The crate does no I/O. The host
(`crates/zvs-host/src/drivers/ai/adapters/zai_web.rs`) sends the requests through the
signed-in browser session and maps the decoded events onto `TextDelta`.

Everything here was observed on the live client, not taken from a published API. Each
observation carries a date (`protocol::*_OBSERVATION`); when the client changes, re-observe,
update the code, the tests and this file together.

## Read this before touching the Z.ai integration

- Changing a body, a header, the signature or a decoded field means re-checking it against the
  live client (Chrome DevTools or Claude in Chrome on `https://chat.z.ai/`), then updating the
  observation date in `src/protocol.rs` and the matching section below.
- Mirror what the client sends. Do not invent fields, and do not drop one the client sends
  without re-observing that it is gone.
- Parse only the fields the app uses. The payloads are large and change often; unknown fields
  are ignored on purpose.

## Endpoints

Base `https://chat.z.ai/api` (`protocol::API_BASE`). Headers on every call:
`x-fe-version: <frontend build>` (`FE_VERSION`, read from the page's script path by
`FE_VERSION_PATTERN`), `x-region: overseas` (`REGION`), and a `referer` on the page the client
would be on.

| Call       | Path                                       | Transport | Referer   |
| ---------- | ------------------------------------------ | --------- | --------- |
| Model list | `GET /models`                              | fetch     | `/`       |
| Account    | `GET /v1/auths/`                           | fetch     | `/`       |
| New chat   | `POST /v1/chats/new`                       | fetch     | `/`       |
| Turn       | `POST /v2/chat/completions?<signed>` (SSE) | fetch     | `/c/<id>` |

Observed 2026-10-24 with FE `prod-fe-1.1.115` (`MODELS_OBSERVATION`,
`GENERATION_OBSERVATION`). `CONFIG_PATH` (`/config`) is recorded but not called.

The gateway builds the page plan for this origin from the crate's constants
(`apps/studio/src-tauri/src/browser/gateway/mod.rs`): every request is a plain `fetch`, the
verification-dialog pattern is `CHALLENGE_PATTERN` (`aliyunCaptcha`), the `localStorage`
bearer key is `token` (`PAGE_BEARER_KEY`), and there is no session refresh. `is_turn` marks
`chats/new` and completions as paced; `needs_captcha` marks only completions.

## Account (`/v1/auths/`)

Only `id` is read: it is the `user_id` the signature covers. The host fetches it before the
first turn and keeps it until the session expires.

## Model list (`models.rs`)

Rows are read from `data`. A row that does not deserialize is counted in
`ModelList::skipped`; the host treats a list with no usable rows and at least one skipped row
as malformed. Only these fields are used:

| Field                                     | Becomes                                |
| ----------------------------------------- | -------------------------------------- |
| `id`                                      | `ZaiModel::id` (trimmed, deduplicated) |
| `name`, then `info.name`                  | `display_name`                         |
| `info.is_active == false`                 | row dropped                            |
| `info.params.max_tokens`                  | `max_output`                           |
| `info.meta.capabilities.think`            | `thinking`                             |
| `info.meta.capabilities.reasoning_effort` | `reasoning_efforts` offered at all     |
| `info.meta.info.thinking_effort_level[]`  | `reasoning_efforts` (`low/high/max`)   |
| `info.meta.hidden`                        | `hidden`                               |

Effort levels outside `low`, `high` and `max` are ignored; a model that offers efforts without
listing a known one gets `high` and `max`.

The payload has no context size. `known_context_window` assigns the published GLM sizes by a
prefix of the display name or the id (`CONTEXT_WINDOWS`):

| Prefix    | Context window |
| --------- | -------------- |
| `glm-5.3` | 1 000 000      |
| `glm-5.2` | 1 000 000      |
| `glm-5`   | 204 800        |

Any other model gets the host's `ZAI_DEFAULT_CONTEXT_WINDOW` (131 072), and `max_output` is
clamped to the window. The host drops `hidden` rows, maps `thinking` to
`ModelCapability::Reasoning` and sets the family `glm`. `reasoning_efforts` are parsed but not
used yet.

## New chat (`request::NewChat`)

```json
{
  "chat": {
    "id": "",
    "title": "New Chat",
    "models": ["<model>"],
    "params": {},
    "history": {
      "messages": {
        "<message id>": {
          "id": "<message id>",
          "parentId": null,
          "childrenIds": [],
          "role": "user",
          "content": "<prompt>",
          "timestamp": 1792867619,
          "models": ["<model>"]
        }
      },
      "currentId": "<message id>"
    },
    "tags": [],
    "flags": [],
    "features": [],
    "mcp_servers": [],
    "enable_thinking": true,
    "auto_web_search": false,
    "message_version": 1,
    "extra": {},
    "timestamp": 1792867619851
  }
}
```

The chat already holds the first user message; its id is the `current_user_message_id` of the
turn that follows. The message `timestamp` is in seconds, the chat's in milliseconds. The
answer's `id` is the chat id (`parse_chat_created`).

## Turn (`request::CompletionTurn`)

Top-level keys: `stream: true`, `model`, `messages` (`role`, `content`), `signature_prompt`,
`params: {}`, `extra: {}`, `features`, `variables: {}`, `chat_id`, `id` (the answer's id),
`current_user_message_id`, `current_user_message_parent_id` (`null` on a first turn, the
previous answer's `id` on a follow-up).

`features` mirrors the client with everything off but thinking: `image_generation`,
`web_search`, `auto_web_search`, `preview_mode`, `vlm_tools_enable`, `vlm_web_search_enable`,
`vlm_website_mode` are `false`, `flags` is `[]`, `enable_thinking` follows the model.
`reasoning_effort` is added only with thinking on and an effort given; the host gives none.

`signature_prompt` is the last user message, trimmed. Unlike Qwen, every turn carries the
whole dialog in `messages`. The host sends roles `user` and `assistant` (a tool result goes as
`user`); the system prompt is prefixed to the first user message as `System: …\n\nUser: …`,
or sent as a leading user message when there is none.

## Signature (`signature::SignedRequest`)

Every completion is signed the way the client signs it:

1. `window = floor(timestamp_ms / 300 000)` (`SIGNING_WINDOW_MS`, five minutes).
2. `key = hex(HMAC-SHA256(SIGNING_SECRET, window))`, the secret being the client's constant.
3. `message = "requestId,<id>,timestamp,<ms>,user_id,<user>|<base64(signature_prompt)>|<ms>"`.
4. `signature = hex(HMAC-SHA256(key, message))`, sent as the `x-signature` header.

The query carries, in this order: `timestamp`, `requestId`, `user_id`, `version` (`0.0.1`),
`platform` (`web`), `token`, `signature_timestamp`. The host leaves `token` empty; the page
fills it with the bearer its app keeps (`BEARER_QUERY`), so the token never leaves the page.
`hmac_sha256_hex` is a local HMAC checked against RFC 4231.

## Captcha (`page::CAPTCHA`)

Completions need a token from Alibaba Cloud's captcha, which only its SDK in the page can
produce. The gateway passes `CAPTCHA` to the page: the SDK script the vendor's app loads, its
`region` (`sgp`), `prefix` and `scene_id` (`didk33e0`), `popup` mode, elements of the app's own
(`zvs-captcha-element`, `zvs-captcha-trigger`, never the app's `chat-captcha-element`) and a
10-second timeout. The token goes into the body field `captcha_verify_param`. The crate's body
never sets that field. Nothing is solved or forged; how a puzzle reaches the person is in the
gateway README (`apps/studio/src-tauri/src/browser/gateway/README.md`, «Z.ai»).

## Stream (`stream::TurnDecoder`)

`data:` frames of `type: chat:completion`; other types (`chat:title`) and `[DONE]` are
ignored. Each frame's `data` has a `phase`:

| `phase`    | Read                                           | Event       |
| ---------- | ---------------------------------------------- | ----------- |
| `thinking` | `delta_content`                                | `Reasoning` |
| `answer`   | `delta_content`, or the tail of `edit_content` | `Text`      |
| `done`     | —                                              | `Finished`  |

Reasoning arrives as Markdown quoted inside `<details type="reasoning">`: the first chunk drops
the `<details …>` tag and any `<summary>`, and every chunk drops the `> ` quote prefixes. An
`answer` frame with `edit_content` repeats the closing `</details>`; only the text after the
last one is kept. `done: true` on any frame also means `Finished`.

`usage` on any frame becomes `Usage`, `prompt_tokens_details.cached_tokens` included. An
`error` in `data` or in `data.data` becomes `Failure` with the code from `error_code`, else
`code`; `detail` is never kept. It is flagged `captcha` for a `captcha_error_type` or
`FRONTEND_CAPTCHA_REQUIRED`, and `auth` for 401, 403 and codes containing `unauth`,
`forbidden`, `token`, `login` or `expire`.

The host maps `auth` to a session expiry, `captcha` to a browser verification, and the rest to
«Вендор прервал генерацию» with `vendorCode`. A stream that ends without `Finished` is a
truncated turn. Web search, images and tool calls in the vendor's format are not decoded.

## Chats in the host

A follow-up whose earlier dialog matches an answer the adapter remembers continues that chat,
with the previous answer as the parent; otherwise the turn creates a chat. Up to 256 chains are
kept (`ZAI_CHAINS_MAX`), in memory only. A chained turn refused before any delta (anything but
a cancellation, a session expiry, `PROVIDER_UNREACHABLE` or `RATE_LIMITED`) forgets that chat
and is retried once in a new one. A session expiry forgets every chain and the account id.
Chats are created one at a time. The model list is cached for an hour, and a stale list is kept
when a refresh fails for any reason but a cancellation or a session expiry. Z.ai providers
default to a 600-second timeout (`DEFAULT_TIMEOUT_SECONDS`).

## Vendor-side tools

`EXCLUDED_POLICY_CHECK_TOOLS_NAMES` is empty: no Z.ai call is left to the vendor. See the
`zvs-qwen-web` README for what the list means.

## Tests

`cargo test -p zvs-zai-web` covers the signature, the bodies, the model list, the page rules
and the decoder. The host's adapter tests (`zai_web/tests.rs`) replay recorded payloads from
`test/fixtures/providers/zai*.{json,sse}`; record a new fixture when the stream shape changes.
