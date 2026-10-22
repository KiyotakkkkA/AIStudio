---
task: TASK_063
title: AI drivers in Rust — HTTP client, ApiTransport, SSE, openai-compatible, ollama and local adapters, ProviderRegistry
phase: M — Migration to Tauri
status: done
depends_on: [TASK_062]
unblocks: [TASK_064]
estimate: 5 h
---

# TASK_063 — AI drivers in Rust — HTTP client, ApiTransport, SSE, openai-compatible, ollama and local adapters, ProviderRegistry

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

The AI layer (`host/drivers/ai/`) has no vendor SDK. Every adapter is hand-written over one
`Transport` seam (PASSPORT §12):
- `ports.ts` defines the driver ports (`TextGenerationDriver`, `EmbeddingDriver`).
- `AdapterCapabilities.ts` declares what each family supports.
- `transport/ApiTransport.ts` is the `api` auth mode. It resolves the secret at the point of
  use, adds an `Authorization` header and does an ordinary fetch.
- `adapters/sse.ts` is a 43-line SSE parser.
- `adapters/openaiCompatible.ts`, `ollama.ts` and `local.ts` are the API-mode families. `local`
  reads models from disk and embeds through the local engine.
- `errors.ts` maps HTTP and network failures to `AppError` codes.
- `streamToEvents.ts` re-emits deltas as `token` events.
- `ProviderRegistry.ts` maps a `provider` row to a configured driver, caches it per provider,
  invalidates it on provider, secret or account change, and **never caches a resolved
  plaintext**.
- `injectedTools.ts` covers tool calling for families without native tools.

In Electron, API requests go through `net.fetch` "for Chromium's certificate and proxy
handling". In Rust that becomes one shared `reqwest` client built from the workspace pin
(`rustls-tls-native-roots`, so OS-installed corporate roots apply), honouring the system proxy.

This task ports the API-mode half. Account mode (`AccountTransport`, `qwen-web`,
`deepseek-web`, identity probes) needs the browser profile and lands in TASK_065–TASK_067.
`ProviderRegistry` is built here with the account branch left as a port that, until TASK_067,
answers `account-not-linked`.

## Why

Chat, health checks, embeddings for indexing and the Providers page all go through these
drivers. Porting them behind the same ports and capability descriptors keeps services and UI
honest without guessing.

## Depends on

- `TASK_062` — `SecretService::resolve` for API keys.

## Read first

- `/docs/PASSPORT.md` §12 (two independent axes; adding a vendor that speaks a known dialect
  is a row, not a file)
- `apps/studio/src/host/drivers/ai/` in full, except `transport/AccountTransport.ts`,
  `identity/`, `adapters/qwenWeb.ts`, `deepseekWeb.ts` and `accountFamily.ts`
- `apps/studio/tests/aiDrivers.test.ts`, `localAdapter.test.ts`, `toolCalling.test.ts`,
  `providers.test.ts`
- `docs/know-how/QWEN_DEEPSEEK_TOOLS_INJECTING.md`, for `injectedTools`

## Scope

**In scope**
- `drivers::ai::ports`: `TextGenerationDriver` (generate, stream returning a `Stream` of
  deltas, list models) and `EmbeddingDriver`, async traits that are object-safe for the
  registry cache
- `drivers::ai::http::HttpClient`: one `reqwest::Client` with timeouts, `credentials: omit`
  semantics (no cookie store), the native root store and the system proxy
- `Transport` trait, `ApiTransport` and `RequestSpec`/`TransportResponse` with the same
  semantics as the TS ones, including the timeout default
- An SSE parser ported with its tests, including partial chunks, CRLF, comments and
  multi-line `data:`
- Adapters `openai-compatible`, `ollama` and `local`, each with its `AdapterCapabilities`
- Error mapping (`errors.ts`) with the same codes: `PROVIDER_UNREACHABLE`, auth failures,
  timeouts, `RUN_CANCELLED` and the rest
- `injectedTools` for the families that need it
- `ProviderRegistry` with per-provider caching, invalidation hooks and an account port stub
- `ProviderRepository` and `ModelRepository` (the tables exist from the baseline)
- Cancellation: every driver call takes a `CancellationToken` (`tokio-util`), and streams end
  with `RUN_CANCELLED` when it fires

**Out of scope** (do not do these, even if tempting)
- Account mode (TASK_065–TASK_067)
- `ProviderService`, health checks and the `providers.*` channels (TASK_064)
- New adapter families or vendors

## Steps

1. Ports and capability descriptors, mirrored one for one from the TS files.
2. `HttpClient`. Verify on Windows that the system proxy (registry settings) is honoured by
   the pinned `reqwest`. If it is not, read it explicitly and record how in the handoff.
3. SSE parser and its tests.
4. `ApiTransport`: secret resolution at the point of use and never stored on the struct,
   timeouts, cancellation, header assembly.
5. Adapters, one module each, ported with their tests against a local mock server (`wiremock`
   or `axum` in dev-dependencies, pinned). No live vendor in CI. Port the `liveProvider` test
   as `#[ignore]`d.
6. Error mapping, ported with its table-driven tests.
7. `injectedTools`, ported with `toolCalling.test.ts`.
8. Repositories for `provider` and `model`.
9. `ProviderRegistry`, with tests that the cache invalidates on provider, secret and account
   change and that no plaintext is held after a call returns.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: not visible in the UI yet. A `#[ignore]`d test streams a completion from a
running local Ollama when `ZVS_LIVE_OLLAMA=http://127.0.0.1:11434` is set. Run it once and
note the result in the handoff.

## Definition of done

- [x] Driver ports, capabilities, transports, SSE, three API-mode adapters and error mapping
      are ported with their tests
- [x] One shared HTTP client with native roots and the system proxy, verified on Windows
- [x] The registry caches drivers, never plaintexts, and invalidates correctly
- [x] Cancellation is honoured by every request and stream
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Dependencies: `async-trait =0.1.92` and `axum =0.7.9` (dev, `tokio` + `http1` only), both
  already in the lock. `zvs-host` now takes `reqwest` (workspace pin plus `system-proxy`) and
  `tokio-util`. `serde_json` gains `preserve_order` workspace-wide (see below).
- `zvs-contract`:
  - `enums`: the `WireEnum` trait and a `wire_enum!` macro (`ALL`, `as_str`, `parse`,
    `Display`, serde names, `no_refinements!`).
  - `ai`: `AdapterFamily`, `AuthMode`, `AccountFamily`, `TunableParameter`, `ToolMode`,
    `AccountStatus`, `LOCAL_BASE_URL`, `resolve_tool_mode`, `model_tool_support`.
  - `providers`: `ProviderKind`, `ProviderCapability`, `ProviderStatus`, `ModelCapability`.
    These are enums only. The provider DTOs and channels are TASK_064.
- `zvs-host::drivers::ai`:
  - `ports`: `TextGenerationDriver` (`generate`, `stream` returning a `DeltaStream`,
    `list_models`), `EmbeddingDriver`, `ImageDriver`. They are `async_trait`s and object-safe.
    `AiDriver` holds `Option<Arc<dyn …>>` slots.
  - `capabilities`: `AdapterCapabilities` (`Copy`, `&'static` auth modes), `Honours`,
    `supports_auth_mode`, `unsupported_parameters` (in wire order).
  - `http::HttpClient`: one `reqwest::Client` built with rustls and the native roots, the
    system proxy, no cookie store, a 30 s connect timeout and the UA `ZVSAIStudio/<version>`.
    `HttpClientOptions` takes `ProxyMode::{System, Explicit, Off}` and `resolve` overrides,
    which the tests use.
  - `transport`: `Transport` (`base_url`, `request`, `stream -> ByteStream`), `RequestSpec`,
    `TransportResponse`, `join_url`. `transport::api::ApiTransport` carries `ApiKey` (a secret
    id plus a `SecretResolver`), the 60 s default timeout, header precedence (accept → extra →
    spec → authorization → content-type) and HTTP-failure mapping.
  - `sse`: a streaming UTF-8 decoder, `SseDecoder` (CRLF, comments, multi-line `data:`, the
    earlier boundary wins, stops reading at `[DONE]`), `NdjsonDecoder`, and the `frames`,
    `sse_data` and `ndjson_lines` stream combinators.
  - `errors`: `http_failure` (401/403 auth or session-expired in account mode, 429 with
    `Retry-After` in seconds or as an HTTP date, 408/504 timeout, 5xx unreachable, the rest
    `UNKNOWN`, `vendorMessage` from the body), `network_failure` (Node-style `syscall` from
    `io::ErrorKind` and DNS errors, timeouts, connect errors), `cancelled`, `timed_out`,
    `session_expired`, `is_*`, `raw_error_text`, `raw_cause_text`.
  - `adapters`: `openai_compatible` (chat, SSE, `/models` then `/tags` on 404, the OpenRouter
    catalog join, embeddings, `ToolCallAssembler`), `ollama` (NDJSON `/chat`, native tools,
    `/tags`), `local` (`FileSystemModelStore`, the `LocalEmbeddingEngine` port,
    `LocalEmbeddingAdapter`), and the family table `adapter_entry` / `adapter_entry_named`.
  - `injected_tools`: the prompt, the result fencing, the parser, `could_be_injected_call`,
    `InjectedCallBuffer` (64 Ki UTF-16 units), `injected_request`, `with_injected_tools`.
  - `stream_to_events`: the `streamToEvents` port over `StreamHandle`.
  - `registry::ProviderRegistry` with `ProviderSource` (`DatabaseProviders`), the
    `AccountPort` stub `UnlinkedAccounts`, a per-provider cache keyed by the TS fingerprint,
    `invalidate`, `invalidate_by_secret`, `invalidate_by_account`, `clear`,
    `attach_local_engine`, `capabilities`, `text`, `embedding` and `image`.
- Data: `schema::{ProviderRow, ProviderSettings, ModelRow}`, `schema::columns::{EnumColumn,
  json_column, json_text, qualified}`, and `ProviderRepository` (list, find, create, update,
  remove, `set_status`, `set_default_model`, secret usage kept in step) and `ModelRepository`
  (`list_by_provider` with `selectedModelIds`, `find_by_id`, `replace_for_provider` keeping
  ids). Both are on `Repositories`.
- Tests: 114 new in `zvs-host` (228 in total, 4 ignored, up from 114 and 1) and 3 in `zvs-contract`. Ported:
  every API-mode case from `aiDrivers.test.ts`, `localAdapter.test.ts`, `providers.test.ts`
  and the driver half of `toolCalling.test.ts`, with fixtures read from
  `test/fixtures/providers`. Adapters run over `HttpClient` → `ApiTransport` against
  `mock::MockServer`, a local axum server that records requests and can delay, chunk, hold a
  stream open and count dropped streams. `liveProvider.test.ts` is ported as
  `live::live_list_models_streaming_abort_and_a_wrong_key` (`#[ignore]`), next to
  `live::live_ollama_streams_a_completion`.
- Docs: `crates/zvs-host/README.md` (AI drivers section), `src/data/README.md` (the
  multi-row repository rule) and `crates/zvs-contract/README.md`.

**Deviated from plan:**

- No source comments, at the user's request.
- **Secrets are resolved per request, not when the transport is built.** The TS registry
  awaited `resolve` in `driver()` and kept `Bearer …` on `ApiTransport`. Here `driver()` is
  synchronous and resolves nothing. A missing or undecryptable secret therefore surfaces on
  the first call instead of on `driver()`, and a changed secret value is picked up even by a
  cached driver. `invalidate_by_secret` still exists and is tested.
- **The system proxy needed a feature.** The workspace pin sets `default-features = false`,
  which drops reqwest's `system-proxy`. Without it only `HTTP(S)_PROXY` applies and the
  Windows registry is ignored. `zvs-host` re-enables the feature, and hyper-util's
  `client-proxy-system` then reads
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` (`ProxyEnable`,
  `ProxyServer`, `ProxyOverride`). This is verified by reading that code and by a test that
  runs the test binary in a child process with a proxy in its environment, through the same
  `Matcher::from_system` path. The registry branch itself was not exercised: `ProxyEnable` is
  `0` on this machine, and I did not change the user's Internet Settings. Known limits of
  hyper-util's reader: a per-scheme `ProxyServer` (`http=a:1;https=b:2`) is copied verbatim
  and will not parse, and `<local>` in `ProxyOverride` gets no special handling.
- `serde_json/preserve_order` is on for the whole workspace. Tool-call arguments,
  `injected_call_text` and request bodies need `JSON.stringify` key order: the Ollama fixture
  expects `{"query":…,"k":3}`, and `zvs_call_tool` must come before `arguments`. Every
  existing workspace test passes with it on.
- Account families: `qwen-web` and `deepseek-web` have their capability descriptors but
  `implemented: false`, and `build` answers `adapter … is not implemented yet` until
  TASK_066. The registry reaches the account port first, so an account-mode row gets
  `PROVIDER_SESSION_EXPIRED` with `{ providerId[, accountId], reason: "account-not-linked" }`.
  The TS assertion "every family is implemented" is not ported as written.
- Repositories open no transaction, per `src/data/README.md`. The auth-mode integrity check
  runs **before** the insert or update rather than after it, so a refused row writes nothing
  even without a transaction. `replace_for_provider` and the multi-row writes need the caller's
  `database.transaction`, and the rollback test wraps them in one. `update`,
  `set_default_model` and `replace_for_provider` take `now` instead of calling `Date.now()`.
- `ProviderRow.adapter` and `auth_mode` are `String`, so an unknown value such as `made-up`
  is read and then refused by the registry with the TS message, not by the row mapper.
- `Transport::stream` is `async` and returns `Result<ByteStream>`, so an HTTP failure arrives
  when the stream opens. The adapter streams stay lazy, so a consumer still sees it on the
  first poll, as with the TS async iterator.
- `raw_error_text` is split in two. `AppError` has no `cause`, so
  `raw_error_text(&AppError)` gives the vendor message or the message, and
  `raw_cause_text(&dyn Error)` walks a source chain.
- `FileSystemModelStore` sorts case-insensitively with a byte tie-break. This approximates
  `localeCompare(…, "ru")`. Injected call ids are `call_<uuid v7>` instead of v4.
- The transports live in `drivers/ai/transport/` (`mod.rs`, `api.rs`), as PASSPORT §12 lays
  out.
- Not ported: the Qwen case from `toolCalling.test.ts` (account mode, TASK_066), and the
  `chatTools`/`vector.list` cases, which test services and kernel nodes rather than drivers.

**Next session should know:**

- Wiring for TASK_064:
  `ProviderRegistry::new(ProviderRegistryOptions::new(Arc::new(DatabaseProviders::new(db)), secrets, HttpClient::new()?))`.
  `secrets` must be an `Arc<dyn SecretResolver>`, and `SecretService` implements the trait.
  `Services.secrets` is held by value today, so either make it an `Arc<SecretService>` or
  share one `Arc` between `Services` and the registry. Nothing in `compose` builds the
  registry yet.
- Wrap `ModelRepository::replace_for_provider` and every provider write in
  `database.transaction(|tx| …)`.
- Use `with_injected_tools(driver, None)` when `resolve_tool_mode(capabilities.tools, …)` is
  `Injected`, as `ChatService` does.
- For the account half (TASK_067), implement `AccountPort` (`snapshot` feeds the
  fingerprint; `transport` returns the `AccountTransport`) and pass it in
  `ProviderRegistryOptions::accounts`. Flip the qwen and deepseek entries to
  `implemented: true` once `build` constructs them.
- Live runs on 2026-10-14:
  - `ZVS_LIVE_OLLAMA=http://127.0.0.1:11434` against Ollama started for the check.
    `/api/tags` listed `glm-4.6:cloud` and `embeddinggemma:latest` through the real stack.
    Only those two models are pulled. `glm-4.6:cloud` forwards prompts to ollama.com under the
    user's account, so it was not used. Streaming `embeddinggemma` returned Ollama's 400 ("does
    not support chat"), which mapped to `UNKNOWN` «Провайдер вернул неизвестную ошибку», the
    TS mapping for a 400.
  - A successful streamed completion against a local chat model is still to be seen. Run
    `ollama pull <small chat model>`, then
    `ZVS_LIVE_OLLAMA=http://127.0.0.1:11434 cargo test -p zvs-host -- --ignored live_ollama --nocapture`.
  - The remote live test (`LIVE_BASE_URL`, `LIVE_API_KEY`) was not run.
- Gates run on Windows on 2026-10-14: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` all
  green. `pnpm typecheck`, `pnpm lint`, `pnpm format:check` and `pnpm build` all exit 0.
  The `preserve_order` feature change forces one full release rebuild of the native crates the
  first time `pnpm typecheck` runs.
