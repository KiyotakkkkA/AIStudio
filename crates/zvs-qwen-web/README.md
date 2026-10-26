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

| `phase`                  | Read                                                         | Event                                    |
| ------------------------ | ------------------------------------------------------------ | ---------------------------------------- |
| `thinking_summary`       | `extra.summary_thought.content[]` (whole snapshots)          | `Reasoning` (new suffixes only)          |
| `answer`                 | `content`                                                    | `Text`; `Finished` on `status: finished` |
| `web_search`, `finished` | `extra.web_search_info[]`: `url`, `title`, `snippet`, `date` | `WebSources`                             |

`usage` on any frame becomes `Usage`. Error frames (`success: false`, `error`, `status >= 400`)
become `Failure`, flagged `auth` for 401, 403 and session-like codes. Vendor messages are never
kept.

With web search on, the model may write a short answer, finish it, search, and then answer
again: `answer: finished` can arrive more than once, so it marks the turn complete but does
not end the stream. Search results arrive only on the `finished` `web_search` frame (the
`typing` frames carry the query in `function_call.arguments`). The answer cites them as
`[[n]]`, 1-based over the results in arrival order. Rows without an `http(s)` URL are
skipped; an empty title falls back to the URL; empty `snippet` and `date` are dropped.
`web_extractor` phases (pages the model read) are not decoded.

## Vendor-side tools

`EXCLUDED_POLICY_CHECK_TOOLS_NAMES` When
the model emits a call with one of these names and the app did not offer a tool by that name,
the chat loop drops the call with a log line instead of refusing it as unoffered: the search
already ran on the vendor's side and its results arrive as `WebSources`. `zvs-zai-web` exposes
the same constant, empty for now.

## Tests

`cargo test -p zvs-qwen-web` covers the bodies, the model list and the decoder. The host's
adapter tests (`qwen_web/tests.rs`) replay recorded streams from
`test/fixtures/providers/qwen*.{json,sse}`; record a new fixture when the stream shape changes.
