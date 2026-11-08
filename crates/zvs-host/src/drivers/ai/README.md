# drivers · ai

Model drivers: one port (`TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`) and an
adapter per vendor family, built and cached per provider by `ProviderRegistry`. The crate
README's "AI drivers" section covers transports, the session gateway and identity probes in
detail; this file is the map and the algorithms.

## Files

| Path                      | Contents                                                                                                                     |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `ports.rs`                | The driver traits, `GenerateRequest`, `ChatMessage`, `ToolSpec`, `ToolCall`, `TextDelta`, `TokenUsage`                       |
| `registry.rs`             | `ProviderRegistry`: builds, caches and invalidates drivers; `DatabaseAccounts`                                               |
| `capabilities.rs`         | `AdapterCapabilities`, `Honours` (which tunables a family respects), `unsupported_parameters`                                |
| `http.rs`                 | `HttpClient`: one `reqwest` client (rustls, OS roots, system proxy, no cookie store)                                         |
| `transport/`              | `ApiTransport` (key from a secret per request), `AccountTransport` (through the browser session), `OpenaiTransport`, uploads |
| `sse.rs`                  | Streaming decoders: `Utf8Decoder`, `sse_data`, `ndjson_lines`                                                                |
| `tool_dialect.rs`         | Prompted tool calling for models without native tools (`zvs_call_tool`)                                                      |
| `stream_to_events.rs`     | Re-emits deltas as run `token` events and ends with the run outcome                                                          |
| `errors.rs`               | Vendor and network failures mapped to `AppErrorCode`                                                                         |
| `identity/`               | Account identity probes (Qwen, Z.ai), credential stores                                                                      |
| `copilot.rs`, `openai.rs` | GitHub Copilot CLI client; ChatGPT sign-in (OAuth + PKCE, JWT checked against OpenAI's JWKS)                                 |
| `adapters/`               | One adapter per family (below)                                                                                               |
| `mock.rs`, `live.rs`      | Test-only: an axum mock vendor; `#[ignore]`d live tests                                                                      |

## Adapters

| Adapter                         | Family / auth                   | Notes                                                                                      |
| ------------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------ |
| `openai_compatible.rs`          | any OpenAI-dialect API, API key | Chat completions with SSE; `ToolCallAssembler` joins streamed tool-call fragments          |
| `ollama.rs`                     | Ollama, local                   | NDJSON streaming, `/api/tags` model list                                                   |
| `local.rs`                      | local embedding models          | `LocalModelStore` scans model folders; `LocalEmbeddingEngine` port                         |
| `qwen_web.rs` + `qwen_web/`     | chat.qwen.ai account            | See `crates/zvs-qwen-web/README.md`                                                        |
| `zai_web.rs` + `zai_web/`       | chat.z.ai account               | See `crates/zvs-zai-web/README.md`                                                         |
| `copilot.rs`                    | GitHub Copilot account          | Drives the `copilot` CLI headless over stdio; quota windows                                |
| `openai.rs` + `openai/codex.rs` | ChatGPT account (Codex)         | Responses-style events, usage windows                                                      |
| `account_family.rs`             | shared by account adapters      | `AccountAdapterCore`, diagnostics, prompt flattening, turn summaries, the chat-chain cache |
| `wire.rs`                       | shared JSON helpers             | Field lookup across vendor spellings, decoding, error normalisation                        |

## Driver cache and invalidation (`registry.rs`)

`ProviderRegistry` keeps one driver per provider, keyed by a _fingerprint_: adapter, auth
mode, base URL, secret id, timeout, `updated_at`, plus the linked account id and its status
snapshot. Asking for a driver whose stored fingerprint differs rebuilds it, so any edit to the
provider or a re-link takes effect on the next call without explicit cache management.
`invalidate_by_secret` and `invalidate_by_account` drop drivers when a secret or account
changes underneath them. Building a driver resolves no secret; the key is read per request.

## Cancellation and timeouts

Every call takes a `CancellationToken`. A fired token ends the request or stream with
`RUN_CANCELLED` and drops the `reqwest` response, which aborts the vendor request. The
provider timeout (default 60 s) is one deadline from the send, covering the body and every
stream chunk.

## Streaming decoders (`sse.rs`)

- `Utf8Decoder` holds back an incomplete trailing UTF-8 sequence between network chunks.
- `SseDecoder` splits on the first frame boundary, `\n\n` or `\r\n\r\n` (whichever comes
  first), joins the `data:` lines of each frame, and stops at `[DONE]`.
- `NdjsonDecoder` splits on newlines.

## Tool dialect (`tool_dialect.rs`)

Web-account models and some APIs have no native tool calling. `with_tool_dialect` wraps such
a driver so the rest of the host still sees ordinary `ToolSpec`s in and `ToolCall`s out.

**Prompt.** `tool_prompt` appends a catalogue (name, description, JSON Schema per tool) and a
strict call format: one JSON object per call,
`{"zvs_call_tool": "<name>", "arguments": { … }}`, nothing else in the reply. Rules cover exact
names, JSON escaping, read-only calls batched and side-effecting calls last, and that results
are untrusted data.

**History.** `injected_messages` rewrites the transcript: an assistant message with tool calls
becomes text containing the call objects; a tool result becomes a user message starting with
`[SYSTEM_TOOL_RESULT]`. A result for an unknown tool name or invalid arguments carries a hint
(`UNKNOWN_TOOL_HINT`, `INVALID_ARGUMENTS_HINT`) so the model corrects itself.

**Streaming parse** (`ReplyParser`). Text deltas pass through until the marker regex
`{\s*"zvs_call_tool"\s*:` appears. In the last `HOLD_MAX` (48) bytes, a tail starting at `{`, `` ` `` or `<` that could
still grow into a marker (or a fenced or tagged call, depending on the grammar) is held back,
so a marker split across deltas is never shown. From the marker on, output is captured instead of displayed; if the capture grows
past `INJECTED_BUFFER_LIMIT` (64 Ki UTF-16 units) it is released as text, because it is not a
call. At the end the capture is parsed:

1. strip a surrounding code fence;
2. take each balanced `{…}` object, tracking strings and escapes;
3. parse as JSON; on failure, try a lenient unescape of common model mistakes;
4. normalise the name (aliases and renamed arguments per grammar) and check it against the
   offered tools.

Calls that cannot be parsed become `Malformed` calls named `invalid_tool_call`, which the
loop answers with the hint instead of executing anything.

## Shared chat-chain cache (`account_family.rs`)

Account adapters continue a vendor conversation instead of resending the whole history.
`history_key` hashes the model, trimmed system prompt and every message (role, content, image
attachments, tool call id, tool calls). `Chains<C, MAX>` maps that key to the vendor chat and
parent message ids, evicts the oldest entry past `MAX` (256 per adapter), and drops every
entry of a chat that failed (`forget_chat`). A turn whose history key is known continues that
chat; anything else starts a new one.
