# zvs-host

The Rust host. Every service, repository, driver and the execution kernel live here
(PASSPORT §5, §8). The crate has **no Tauri dependency**, so all of it is unit-testable
without a window. Shell capabilities reach it through port traits that `zvs-studio`
implements, and `apps/studio/src-tauri/src/lib.rs` is the composition root that builds it.

| Module                     | Contents                                                                                                          |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `ipc`                      | `IpcServer`, `register`, and one file of thin handlers per domain                                                 |
| `services`                 | `Services`, the handlers' only dependency, and the business logic                                                 |
| `data`                     | `Database`, migrations, row structs and repositories (see `src/data/README.md`)                                   |
| `platform::paths`          | `PathEnvironment` → `StudioPaths`, plus the validated path builders                                               |
| `platform::logger`         | `Logger` trait, `FileLogger` (JSON lines, rotation, redaction), `silent_logger`                                   |
| `platform::ids`            | UUID v7 ids and the injectable `IdGenerator`                                                                      |
| `platform::clock`          | `Clock`, `SystemClock`, `FakeClock`                                                                               |
| `platform::events`         | `EventBus`, `StreamHandle`, the `EventSink` and `EventRecorder` ports                                             |
| `platform::recorder`       | `FileEventRecorder` and the `ZVS_RECORD_EVENTS` flag                                                              |
| `platform::theme`          | `ThemePort`, `ThemeSource`, `apply_theme_source`, `THEME_SETTING_KEY`                                             |
| `platform::window_state`   | `read_window_state`, `capture_window_state`, `WINDOW_STATE_KEY`                                                   |
| `platform::shutdown`       | `ShutdownCoordinator`, `ShutdownStage`, `ExitDecision`                                                            |
| `platform::navigation`     | `WorkspaceRoute`, the `WorkspaceNavigator` port (implemented by `EventBus`)                                       |
| `platform::device`         | `DeviceProbe`, `DeviceReadings`, `os_readings`, `device_tier`                                                     |
| `platform::gpu`            | The `GpuProbe` port, `GpuAdapter`, `select_gpu`, `NoGpu`                                                          |
| `platform::metrics`        | `ResourceMonitor`, `ResourceCounters` (`SysinfoCounters`), the `GpuSampler` port, `NvidiaSmi`                     |
| `platform::disk`           | `free_bytes`                                                                                                      |
| `platform::archive`        | `extract_archive` (zip over `flate2`, tar.gz over `tar`), `safe_member_path`, `read_zip_entries`                  |
| `platform::long_path`      | `to_extended_path`, `from_extended_path`, `extended` (the `\\?\` prefix)                                          |
| `platform::picker`         | The `PickerPort` behind `vectorStores.sources.pick` (`tauri-plugin-dialog` in `zvs-studio`)                       |
| `services::crypto`         | `CryptoBackend`, `CryptoService`, `Plaintext`, `UnavailableBackend`                                               |
| `services::secrets`        | `SecretService`, the summary and DTO mapping, `secret_policy`, `secret_fields`                                    |
| `services::providers`      | `ProviderService`, `DriverSource`, `probe_outcome`, `provider_status`                                             |
| `services::health_check`   | `HealthCheckService`, the `providers.healthcheck.*` settings keys                                                 |
| `services::vector_stores`  | `VectorStoreService`, `VectorSearchOptions`, `effective_mode`, `StoreLease`, `Stopwatch`                          |
| `services::vector_sources` | `VectorSourceService`: sources, documents, `resolve_path`, `from_extended_path`                                   |
| `services::vector_eval`    | `VectorEvalService`, the `EvalSearch` port, `eval_modes`, the parity run (`parity.rs`, tests only)                |
| `services::graph`          | `GraphService`: relation catalogue, edges, hydrate-on-first-use, reconcile, traversals                            |
| `services::rerank`         | The `Reranker` port, `RerankerSource`, `local_rerankers`, `normalise_rerank_score`, `apply_rerank`                |
| `services::retrieval_*`    | `merge_retrieval` (floor, ±1 collapse, per-document cap, top N), `retrieval_metrics`, `retrieval_query`           |
| `services::vector_health`  | `derive_vector_health`, the metric mapping between the contract and `zvs-core`                                    |
| `services::accounts`       | `AccountService`, the `AccountBrowser` port, `mask_email`                                                         |
| `downloads`                | `DownloadService`, `CatalogueService` (curated, Ollama, engine builds), `DiskService`, the `DownloadRunner` port  |
| `runtimes`                 | `RuntimeService`, `LlamaServer`, `ReleaseResolver`, the engine definitions and `plan_accelerator`                 |
| `drivers::ai`              | AI ports, `HttpClient`, transports, adapters, `ProviderRegistry` (see below)                                      |
| `drivers::core`            | `TextCore`, `VectorCore`, `GraphCore`, `InProcessCore`, `GraphRegistry`                                           |
| `drivers::sidecar`         | `SidecarDriver`, `SidecarJobs`, `JobRunOptions`, the NDJSON protocol                                              |
| `testing`                  | `temporary_directory`, `temporary_database`, `test_services`, `FakeCrypto`, `FakeDriver`, `FakeSessionGateway`, … |
| `browser::policy`          | `is_web_url`, `navigation_url`, `profile_dir`, `BROWSER_PARTITION`, the heights, `TAB_LIMIT`                      |
| `browser::lifecycle`       | `BrowserLifecycle`, `BrowserLifecycleEvent`, `LifecycleSubscription`                                              |
| `browser::nodes`           | `browser.*` nodes over the `AutomationPort`, `Browser`, `register_browser_nodes`                                  |
| `browser::grants`          | `BrowserGrants`: per-origin click and type grants; `capability::resolve_tier` decides every browser tier          |
| `kernel`                   | `NodeDef`, `NodeRegistry`, `Scheduler`, `RunStream`, `StepContext`, the core and job nodes (see below)            |
| `services::runs`           | `RunService`: start, inspect, list, retry, clear, cancel, recover, subscribe, dispose                             |
| `services::permissions`    | `PermissionService`: grants, revocation, `admit` / `admit_subject` with `off > ask > auto`                        |
| `services::approvals`      | `ApprovalService`: persisted pending approvals, timeout, decisions, `always` grants                               |
| `services::retention`      | `RetentionService`: prunes step payloads and events past `runs.retention.days`                                    |
| `tools`                    | `ToolRegistry` (tiers, ceiling, defaults, usage, references), `ToolInvoker`, `register_kernel_tools`              |
| `services::mcp`            | `McpService`: servers, supervision, discovery into the registry, health, autostart, `derive_namespace`            |
| `drivers::mcp`             | The `rmcp` client, `StdioTransport`, `McpHttpClient`, Windows `.cmd` launch, the isolated toolchain env           |
| `skills`                   | `SkillLibrary` (`SKILL.md` folders, index, checks), `SkillService` (trigger match, runs), `skill.*` nodes         |
| `services::chat`           | `ChatService`, the `chat.prepare` / `chat.generate` / `chat.persist` nodes, the tool-calling loop                 |
| `services::chat_history`   | `window_chat_history` (budget, passages, neighbours, related documents, citations), `join_chunks`, `fence_source` |
| `services::chat_tools`     | `offered_tools`, `wire_tool_name`, result budget, capping and fencing, the loop caps                              |
| `services::token_estimate` | `estimate_tokens`, the script-aware conservative estimate                                                         |

`testing` is compiled for this crate's own tests and behind the `testing` feature for other
crates' tests. `debug-panic` adds `InProcessCore::debug_panic`, `vector_debug_panic` and
`graph_debug_panic` for the panic-recovery test; release builds never enable it.

## Core and sidecar

`zvs-core` is linked in-process and reached only through the `drivers::core` ports (PASSPORT
§10). `InProcessCore` implements all three. Synchronous work (chunking, hashing, every graph
call, removing a vector store) runs on `spawn_blocking`; vector calls are LanceDB futures. Each
call is wrapped in `catch_unwind`, so a panic answers `NATIVE_ERROR` («Rust operation
panicked») and the next call works. Core errors keep their code (`core_error`). `ChunkConfig` is validated before the core is reached
(`VALIDATION_FAILED`, «Некорректный размер или перекрытие фрагментов»), and graph ids, nodes,
edges and queries are trimmed and checked against `MAX_HOPS` and `MAX_NODE_BUDGET`
(«Некорректный запрос к графу»). `upsert` takes the caller's `CancellationToken` and passes it
into `VectorIndex::upsert`; a token that is already cancelled answers `RUN_CANCELLED` without
touching the store. The graph registry keyed by `graphId` lives in `InProcessCore`, so the
composition root's single instance is the process-wide registry.

`SidecarDriver` runs `zvs-jobd` over `tokio::process`: the child is spawned on demand
with `CREATE_NO_WINDOW`, requests and answers are newline-delimited JSON (a line over 8 MiB
kills the child), stderr is logged as `Sidecar diagnostics`, and a ping every 5 s (2 s timeout)
kills a silent child. `run(job, params, JobRunOptions { cancel, on_progress })` sends
`job.cancel` when the token fires and kills the process if the job is still running after the
2 s grace. An exit fails every pending job with `SIDECAR_UNAVAILABLE` and schedules the next
spawn after `250 ms × 2^restarts` (capped at 30 s); a ping answer resets the count. `dispose`
kills the child and waits for it, after which `run` answers `CONFLICT`.
`StudioPaths::sidecar_path` is `zvs-jobd(.exe)` next to the main executable: `target/<profile>`
in development, where `scripts/tauri.mjs` builds it, and beside the installed executable when
packaged. The driver's tests live in
`crates/zvs-jobd/tests/driver.rs`, so they run against the real binary Cargo built for them.

## System

`SystemService` answers `system.nativePing` (an in-process chunk count with the default
config), `system.device` and `system.resources`. `DeviceProbe` measures once, caches, and
measures again on `refresh`; a failing GPU or disk probe costs only its own field. The tier
rule: ≥ 30 GiB and ≥ 12 cores is `high`, ≥ 14 GiB and ≥ 6 cores is
`medium`, and a discrete GPU lifts the tier one step. `select_gpu` drops software adapters,
picks the adapter with the most dedicated memory, and calls it discrete when it is NVIDIA, has
at least 2 GiB, or sits beside a weaker adapter. `ResourceMonitor` reads CPU, memory and the
process's resident memory through `sysinfo`, and GPU counters through `nvidia-smi`, which is
tried once and then left alone on a machine without it; a missing reading is `null`, never 0.

## Vector stores and graph

`VectorStoreService` works over `VectorCore`. A store row is written as
`pending` before `create_index`; a failed creation removes the table and the row, and a
failed cleanup marks the row `broken` («Store creation and cleanup failed; metadata retained
for removal»). `list` reports drift without writing (a busy store keeps its stored status),
`get` and `reconcile` repair the counts from `stats`, `clear` drops and recreates the table
and deletes the document rows, and a failed reset marks the store `broken`. Mutations take an
exclusive per-store lease and fail at once while another lease is held (`CONFLICT` «Vector
store is busy»); `lease` hands one to other services. Searches and chunk reads share the store
with each other and wait up to `STORE_READ_WAIT` (30 s) for a running mutation before failing
the same way.
Chunk size, overlap and both prefixes change only before indexing.

`search_timed` embeds the query (prefix included, the provider's `timeoutSeconds` or 30 s)
**before** taking the lease, then re-reads the row and refuses (`CONFLICT` «Vector store
changed during the search») if the provider, model, dimension or query prefix moved. Only the
LanceDB read runs under the lease. `effective_mode` turns `hybrid` into `vector` below
schema v2. With reranking on, the search fetches `max(k, candidates)` without a floor, the
`Reranker` scores `passage_of` each hit (2000 UTF-16 units, query 1000), scores are
sigmoid-normalised, and the floor and `k` apply afterwards. No reranker, an error, a malformed
answer or the 20 s timeout fall back to vector order with a `warn`; a cancelled caller gets
the cancellation. Local rerankers attach through `attach_rerankers` once the runtime service
exists (TASK_071). Durations come from the injectable `Stopwatch`.

`VectorSourceService` is the non-indexing half of the vector store sources: sources (paths
trimmed, the `\\?\` prefix stripped and resolved like `path.resolve`), documents, and
`remove_document` (delete the vectors under the lease, drop the row, `maintain`, reconcile).
`VectorEvalService` runs every case in
each mode `eval_modes` allows and stores the run as `last_eval`; a failed case is reported, not
fatal. In `GraphService` the catalogue comes from migration
`0001_graph_relation_seed`, the engine graph of a store is hydrated from the ledger on first
use (`tokio::sync::OnceCell`, retried after a failure), a failed engine write rolls the row
back, a failed engine removal keeps the row and sets `graph_stale`. Engine weights are `f32`;
they come back with their shortest decimal spelling (`stored::widen`).

Search hit scores are the core's `f32` widened with `f64::from`. The parity test
(`services::vector_eval::parity`) runs the corpus in `test/fixtures/retrieval/eval-corpus.json`
and asserts the metrics pinned in it.

## Kernel

The kernel's behaviour is specified in `src/kernel/README.md`. A node is written once as a typed `NodeDef`
(`Input: DeserializeOwned + JsonSchema + Validate`, `Output: Serialize + JsonSchema +
Validate`, a `PermissionRequirement` and a mandatory `side_effect`) and registered type-erased
over `serde_json::Value`. Registration records the input and output JSON Schema
(`schemars`, draft 2020-12, subschemas inlined, `null` stripped from optional input
properties because they are absent-or-set like Zod's `.optional()`); TASK_073 hands those to
models. `FnNode` wraps a closure for small nodes and tests. `NodeRegistry` keeps insertion
order, which `for_tool` relies on.

`Scheduler` runs one graph. Every node runs in its own task; the scheduler races it against
the run's `CancellationToken` and, after cancellation, waits `grace` (500 ms by default) before
marking the step `abandoned`. The task is detached, not aborted, and a `live` flag drops its
late events, approvals and output. Retries, backoff, the concurrency cap, bindings and the
sibling abort follow `src/kernel/README.md`. Run, step and event writes go through `RunStream::commit`:
the rows and the `run_event` rows of the drafts commit in one SQLite transaction, then the
events are delivered and synchronous observers (`RunService::subscribe`) are called. Delivery
and observers run after the database lock is released, so an observer may write to the
database but must not emit on the run it observes. `StreamHandle::publish_with` is the hook
that assigns sequence numbers inside that transaction.

`RunService` admits graphs (`registry.validate` before any row exists), returns `{ id,
streamId }` before the scheduler task first runs, and drives `runs.*`. Admission composes the
node requirement, the global ceiling, any registered tool row and every matching scope; chat,
indexing and download runs admit their own core tools. An `ask` persists a pending approval,
emits it, blocks scheduling, and waits for `runs.approve` / `runs.deny`, the timeout (60 s) or
cancellation. `recover()` abandons in-flight steps, fails runs with an interrupted approval
(`APPROVAL_DENIED`), resumes side-effect-free graphs on their original stream and marks the
rest `interrupted`; the composition root calls it after the registry is complete. `dispose()`
suspends every run with the same grace and drains them in the `Drain` stage, before services
and SQLite close. `system.demoJob` starts one `job.run` over `job.sleep`; like every
`job.run`, it needs a `job.run` grant or an approval.

## IPC server

`IpcServer::handle::<I, O, _, _>(channel, handler)` registers one contract channel. A handler
is `async fn(Arc<Services>, I) -> Result<O, AppError>`. `dispatch(channel, payload)` returns
the wire envelope as a `serde_json::Value`, which `@zvs/ipc`'s client validates:

| Failure                                | Envelope                                                       | Log                                                  |
| -------------------------------------- | -------------------------------------------------------------- | ---------------------------------------------------- |
| Payload fails serde or `Validate`      | `VALIDATION_FAILED`, `Некорректные данные запроса`, no details | `warn` `Rejected invalid input`                      |
| Handler returns `AppError`             | The error as is, details included                              | `error` `Handler failed`                             |
| Handler panics                         | `UNKNOWN`, `Внутренняя ошибка`                                 | `error` `Handler failed` with the panic message      |
| Output fails `Validate` (debug builds) | `UNKNOWN`, `Внутренняя ошибка`                                 | `error` `Handler output does not match the contract` |
| Channel has no handler                 | `NOT_IMPLEMENTED`, naming the channel                          | `debug` `Called a channel that is not implemented`   |

A panic is caught with `FutureExt::catch_unwind`, so it costs one call and leaves the server
usable. Registering a name outside `zvs_contract::CHANNELS`, or the same name twice, panics at
startup. `finish()` writes one log line: `Every contract channel is served` once every channel
has a handler, otherwise the channels still answering `NOT_IMPLEMENTED`. Since TASK_074 every
contract channel is served, and `ipc::tests::every_contract_channel_is_served` is strict.

## Logger

`logs/studio.jsonl`, rotated to `studio.1.jsonl` … `studio.3.jsonl` at 5 MiB. A line's key
order is the caller's fields first, then `timestamp`, `level`, `scope`, `message`, and `raw`
(always present at `warn` and `error`). Strings under secret-like keys are redacted. Fields are passed as a slice of `(&str, Value)` so
their order is kept.

## Events

`EventBus::open_stream()` returns a `StreamHandle`. `seq` starts at 0 per stream, `ts` comes
from the injected clock, and `end(outcome)` closes the stream. An emit or end after the end is
logged at `error` and dropped. Nothing panics. Delivery goes through the attached
`EventSink`; with none attached, events are logged at `debug` and dropped. The recorder, when
present, sees every event before delivery. In the studio, the `Channel` handed over by
`events_subscribe` is the sink.

## Secrets

Values are write-only (PASSPORT §18.4). `SecretService::resolve` is the one way back to a
plaintext and is for host services only; no handler in `ipc::secrets` calls it. It returns
`Plaintext`, which zeroises on drop, prints `Plaintext([redacted])` under `Debug` and has no
`Serialize`. Inbound values arrive as `zvs_contract::SecretInput`, which behaves the same way.
Row structs never select `cipher` except `SecretCipherRow`, and its `Debug` prints the length.

`CryptoBackend` is the platform port. `zvs-studio` supplies `DpapiBackend` on Windows
(`CryptProtectData` / `CryptUnprotectData`, current user, ciphertext prefixed with version byte
`0x01`) and `UnavailableBackend` elsewhere, which makes `CryptoService` refuse with
`SECRET_DECRYPT_FAILED` rather than store anything. macOS Keychain and Linux Secret Service are
follow-ups. Field validation against a type schema (`services::secret_fields`) applies strict
keys, defaults, and URL fields through the WHATWG `url` crate.

## AI drivers

`drivers::ai` holds the AI drivers (PASSPORT §12): API mode, and account mode over the
`SessionGateway` port with `AccountTransport`, `qwen-web` and the identity probe. Qwen is the only account family; why DeepSeek was removed is
in `docs/adr/0001-account-provider-limitations.md`.

| Module             | Contents                                                                                                          |
| ------------------ | ----------------------------------------------------------------------------------------------------------------- |
| `ports`            | `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`, `AiDriver`, the request/result types                    |
| `capabilities`     | `AdapterCapabilities`, `Honours`, `supports_auth_mode`, `unsupported_parameters`                                  |
| `http`             | `HttpClient`: one `reqwest::Client`, rustls with the OS roots, the system proxy, no cookies                       |
| `transport`        | `Transport`, `RequestSpec`, `TransportResponse`, `join_url`, `ApiTransport`, `AccountTransport`, `SessionGateway` |
| `sse`              | Streaming UTF-8 decoding, `sse_data` (stops at `[DONE]`) and `ndjson_lines`                                       |
| `errors`           | `http_failure`, `network_failure`, `cancelled`, `timed_out`, `session_expired`, raw text                          |
| `adapters`         | `openai-compatible`, `ollama`, `local`, `qwen-web`, the family table                                              |
| `identity`         | Identity probes per account family, `ProbeAccountCredentials`, `AccountCredentialStore`                           |
| `tool_dialect`     | `ToolDialect`, the `zvs_call_tool` prompt protocol, `with_tool_dialect`; web adapters wrap themselves             |
| `stream_to_events` | Deltas re-emitted as `token` events, ended with the run outcome                                                   |
| `registry`         | `ProviderRegistry`, `ProviderSource`, `AccountPort`, `DatabaseAccounts`, `UnlinkedAccounts`                       |

Every driver call takes a `tokio_util::sync::CancellationToken`. A fired token ends a request
or a stream with `RUN_CANCELLED` and drops the `reqwest` response, which aborts the vendor
request. The provider timeout (default 60 s) is one deadline from the send, covering the body
and every stream chunk.

`ApiTransport` holds the secret **id** and a `SecretResolver`, never the key. Each request
resolves it, builds a sensitive `Authorization` header and drops the plaintext before the call
returns. `ProviderRegistry` caches drivers per provider and drops them on a fingerprint change
(adapter, auth mode, base URL, secret id, timeout, `updated_at`, account status), on
`invalidate_by_secret` and on `invalidate_by_account`. Building a driver resolves nothing.

`HttpClient::new()` honours the system proxy: reqwest's `system-proxy` feature reads
`HTTP(S)_PROXY`/`NO_PROXY` and, on Windows, `HKCU\…\Internet Settings` (`ProxyEnable`,
`ProxyServer`, `ProxyOverride`). `the_default_client_honours_the_proxy_the_operating_system_reports`
proves it in a child process.

Adapter tests run against `mock::MockServer`, a local axum server, so the whole stack
(`HttpClient` → `ApiTransport` → adapter) is exercised. Two `#[ignore]`d tests in `live.rs`
talk to real endpoints: `LIVE_BASE_URL` + `LIVE_API_KEY` (+ `LIVE_MODEL`) for any
OpenAI-dialect vendor, and `ZVS_LIVE_OLLAMA=http://127.0.0.1:11434`
(+ `ZVS_LIVE_OLLAMA_MODEL`) for a local Ollama.

### Session gateway

`transport::session::SessionGateway` has `partition()`, `user_agent()`,
`account_token(url)` and `fetch(url, SessionRequest, &CancellationToken)`. The response is a
`SessionResponse` with the status, lower-cased headers and the body as a `ByteStream`, which
`bytes()` and `text()` collect. `AccountToken` and `SessionRequest` print no secret values.
The host never holds a cookie: `zvs-studio`'s `AccountGateway` runs every request as `fetch()`
inside the browser profile (see `apps/studio/src-tauri/src/browser/gateway/README.md`).
`testing::FakeSessionGateway` answers from per-URL queues (the last reply repeats) or a
responder closure, records requests and the cancel scope of each, can stall or hold a body
open, and holds tokens per origin.

### Account transport

`transport::account::AccountTransport` works over a `SessionGateway`. Each
request sends `accept`, the gateway's `user-agent`, the base URL's `origin` and `referer`, the
extra and spec headers, `authorization` from `AccountCredentials::current` when there is a
token, and `content-type` for a JSON body. A `401`/`403` on an authorised request asks
`refresh` once and retries once; no refreshed token is `PROVIDER_SESSION_EXPIRED` with the
status. A `3xx` or an HTML body is a signed-out session (`PROVIDER_SESSION_EXPIRED` with the
partition). Other failures go through `http_failure` in account mode. The timeout (default
60 s) is one deadline per attempt, covering the body and every stream chunk. Each attempt gets
a child cancel scope that is cancelled when the attempt is dropped, so the gateway stops a
request nobody reads.

### Identity

`identity::IdentityProbe` has one implementation per account family: `QwenIdentityProbe`
(`GET https://chat.qwen.ai/api/v1/auths/`). It sends the captured bearer when the gateway has
one, gives up after 20 s, maps a redirect or HTML to `PROVIDER_SESSION_EXPIRED`, and validates
the payload field by field (a malformed profile is `VALIDATION_FAILED`).

- `account_session` wraps the gateway so `account_token` falls back to the stored token for
  the identity origin, which keeps a session usable after a restart.
- `ProbeAccountCredentials` (the registry's credentials) re-probes on every `current` and
  `refresh`, refuses a different identity, and writes nothing.
- `AccountCredentialStore` re-probes when the stored token is within 60 s of expiry, stores a
  new token through `SecretService` (type `account-token`, encrypted, never in a DTO), marks
  the account `needs-relink` on a signed-out probe or a different identity, and on `unlink`
  deletes the token secret.

### Account adapters

`qwen-web` lists models from `/models/` (one entry that fails validation fails the whole
list, inactive and duplicate ids are skipped, `reasoning` comes from `capabilities.thinking`,
else `abilities.thinking`), and generates by streaming `/chat/completions?chat_id=…` into a
vendor chat. Chats are leased per model: a turn takes an idle chat, or creates one with
`/chats/new` when none is idle (so concurrent turns get separate chats), and returns it after a
finished answer; a chat is retired after 200 turns. A reused chat that fails before any delta
with an error that is not a cancellation, a session expiry or `PROVIDER_UNREACHABLE` is
dropped and the turn retried once on a new chat; a session expiry forgets every idle chat.
History is flattened into one role-labelled prompt and each turn is a root message
(`parent_id: null`).
`thinking_summary` frames become reasoning deltas (only the new suffix of each summary line),
`answer` frames text deltas; a stream that ends without `status: finished` is
`PROVIDER_UNREACHABLE` «Поток вендора оборвался». Vendor frames with `401`/`403` or an
auth-like code are `PROVIDER_SESSION_EXPIRED`, other error frames `UNKNOWN` «Вендор прервал
генерацию». Migration `0002_remove_deepseek_web` deletes any `deepseek-web` account, its
token secret and its providers left from earlier builds.

The registry builds account-mode transports through `DatabaseAccounts`: the provider's
account must exist and be `linked` (else `PROVIDER_SESSION_EXPIRED`, with
`reason: account-not-linked` or the status), the session comes from the `SessionFactory`
(without one, `VALIDATION_FAILED` «Сессия браузера недоступна»), and the credentials are
`ProbeAccountCredentials`. The `#[ignore]`d
`live_qwen_identity_models_and_thinking_and_fast_generation` talks to chat.qwen.ai with
`LIVE_QWEN_TOKEN`.

## Providers

`ProviderService` reads and writes `provider` and `model` through
the repositories, always inside `database.transaction`, and reaches drivers only through the
`DriverSource` port (`ephemeral_driver`, `invalidate`), which `ProviderRegistry` implements.
The service never branches on the auth mode; a test scans its source to keep it that way.

A probe lists models through the first port the driver has (text, then embedding, then image)
and races it against the provider timeout (`settings.timeoutSeconds`, else 30 s), the caller's
`CancellationToken` and the service's own shutdown token. The timeout covers building the
driver too. The result is a `ProbeOutcome` (`probe_outcome.rs`): `ok`, `ok-empty`,
`auth-failed`, `account-not-linked`, `session-expired`, `unreachable`, `rate-limited` or
`error`. Vendor messages never reach the outcome; they go to the `warn` line
`Probe did not succeed` as `raw`. `provider_status.rs` turns the outcome and the credential
lifetimes (secret `rotatesAt`, account `token_expires_at` in seconds) into the card status.

A saved probe runs as a spawned task keyed by provider id, so a second caller joins the first
and `update` and `remove` answer `CONFLICT` while it runs. A successful probe replaces the
model list and the status in one transaction; a failed one keeps the models. A draft probe
writes nothing. `dispose` refuses new probes, cancels running ones and waits for them.

An account-mode provider whose account is missing or not `linked` reports `session-expired`
without building a driver.

`HealthCheckService` is off unless `providers.healthcheck.enabled` is `true`. It sweeps every
enabled, idle provider every `providers.healthcheck.minutes` (default 15, at least 1) on a
`tokio` interval, reports `progress` on its own event stream and ends it `ok` or `failed`.
`stop` bumps a generation and cancels the running sweep; `dispose` also waits for it.
`settings.set` on either key restarts it.

## Accounts

In `AccountService`, `link(family)` joins a running link for the same
family, cancels refreshes of that family, opens the vendor login through the `AccountBrowser`
port, and polls the identity probe every 2 s (±10 % jitter) on its own event stream
(`step` `waiting-for-sign-in`, `progress` against the 180 s deadline). A signed-out or
unreachable probe waits and retries; any other failure ends the link as
`PROVIDER_UNREACHABLE`. A found identity is saved in one transaction: other identities of the
family are removed with their token secrets, the matching row keeps its id (and with it the
providers and their default models), the token is created or updated in `SecretService`, and
the masked email is stored. The stream ends `ok` with a `done` step, and the studio navigates
to `/providers`. Cancelling, closing the link tab, closing the workspace, clearing the
vendor's cookies or `dispose` ends the link as `cancelled`; the deadline ends it as
`timeout`. Either way nothing is written, and `link-finished` is emitted.

`refresh(id)` re-probes with the stored token as fallback: a different identity or a
signed-out probe marks the account `needs-relink`, any other failure answers
`PROVIDER_UNREACHABLE` and only touches `last_checked_at`. `start_auto_refresh` checks every
minute and refreshes a linked account after 15 minutes, or after 1 minute when its token
expires within 5 minutes. `unlink` deletes the row and the token secret and detaches the
providers (`failed`, «no account linked»); the browser session stays. `cookies-cleared` for a
vendor host (or a parent domain) cancels its link and refreshes and marks its accounts
`needs-relink` without a network call.

The `accounts.*` channels (`list`, `link`, `cancelLink`, `unlink`, `refresh`) are thin
handlers in `ipc::accounts`. No DTO carries a token, a partition or an unmasked email.

## Shutdown

`ShutdownCoordinator` replaces `before-quit`. Services register named tasks in a stage:
`Stop`, then `Drain`, then `Dispose`. Stages run in that order, the tasks of one stage run
together, and the whole run is capped at `DEFAULT_SHUTDOWN_CAP` (5 s). Every stage logs
`Shutdown stage started` and `Shutdown stage finished` under `host`. `zvs-studio` calls
`request_exit` from `RunEvent::ExitRequested`: the first call prevents the exit and starts the
run, which ends with `AppHandle::exit`; calls during the run keep preventing it; afterwards the
exit goes through and `RunEvent::Exit` logs `Shutdown`. Today it holds `health-check.stop`,
`retention.stop`, `downloads` and `runtimes` (`Stop`), `runs` (`Drain`) and `providers`,
`health-check`, `accounts`, `account-gateway` and `sidecar` (`Dispose`). New services register
their own tasks in the stage that fits them.

## Downloads

In `DownloadService`, a download is a `download` run with one
`download.fetch` node; the node calls `DownloadService::execute`, which runs `job.download` in the
sidecar (resume from `<target>.part`, checksum before the rename). The queue holds at most
`downloads.concurrency` (1–8, default 2) running rows and fills free slots by priority, then
size, then age (`DownloadRepository::next_queued`); pumping is serialised and re-entrant safe.
A start is refused when the catalogue does not know the ref, the item is already active, or the
volume lacks the artefact plus 1 GiB (`Недостаточно места на диске: …`). Pause and cancel record
an intent and cancel the run; a cancellation with no intent is a shutdown and leaves the row
`paused` with its partial. `recover()` requeues rows still `running` after a crash. Byte counts
reach the database at most once per second. An installer (`RuntimeService`) runs after the
transfer and before the row turns `succeeded`.

`CatalogueService` merges its providers, caches the list and the installed snapshot for 60 s,
skips live providers unless `refresh`, keeps the previous list when a refresh reaches nothing,
and answers `find` from `offer`ed engine archives first. `OllamaProvider` reads `/api/tags`
from the first enabled Ollama provider. `DiskService` caches a measurement for 30 s and shares
one measurement between concurrent callers.

`platform::archive` reads zip archives itself (central directory, stored and deflated members,
ZIP64 refused) and tar.gz through `tar` + `flate2`. Every member name goes through
`safe_member_path`: `..` that escapes, drive letters and directory entries are dropped, rooted
names are re-anchored under the target. Tar links and devices are skipped; a zip symlink entry
is written as a plain file. Rust's `std` already adds the `\\?\` prefix to long absolute paths;
`long_path` keeps the explicit helpers for callers that need the spelling.

## Runtimes

In `RuntimeService`, the installed builds live in the `runtimes.installed`
setting. `install` resolves today's llama.cpp release (`ReleaseResolver`, cached 6 h, the cache
kept when GitHub is unreachable, 403 is `RATE_LIMITED`), offers each asset to the catalogue and
queues it; `unpack_download` extracts an engine archive, deletes it and records the tag and the
`llama-server` it found. `embed`, `read_image` (the OCR entry point, projector found by name
overlap) and `rerank` pick the installed build that loads the format (an accelerated one first)
and keep one `LlamaServer` per build and role. The registry gets the service as its
`LocalEmbeddingEngine` and the vector store service gets `local_rerankers` over it.

`LlamaServer` spawns `llama-server` with `CREATE_NO_WINDOW` and `kill_on_drop`, on a free
loopback port with a random API key, polls `/health` every 250 ms up to 180 s, keeps the last
4000 characters of stderr for start failures, stops itself after 5 idle minutes, and resets when
the process exits on its own (logged as `A local engine server exited`). Concurrent callers join
one launch. A rerank caller that gives up does not stop a model that is still loading; an
embedding or vision caller that gives up does. The local engine client has no proxy. Its tests
re-run the test binary as a stub `llama-server` (`runtimes::llama_server::tests::llama_stub`).

## Tools and MCP

In `ToolRegistry`, a tool row's tier is the permission ceiling: registering
writes the `global` grant (`granted_by = registry`), `set_tier` rewrites it, and `RunService`
(given `tools`) refuses a disabled or unavailable tool before composing scopes and counts a use
after every admitted step. `register_kernel_tools` registers every tool-gated kernel node except
`llm.generate`, with its `schemars` input schema.

`McpService` works over the official SDK, `rmcp =3.5.0` (client side only, no
default features). `rmcp` speaks the protocol (handshake, requests, timeouts with
`notifications/cancelled`, sessions); the transports are ours. `StdioTransport` spawns through
`default_spawn` (Windows: `PATH`/`PATHEXT` resolution and `cmd.exe /d /s /c` with quoted,
caret-escaped arguments for `.cmd`/`.bat`, `CREATE_NO_WINDOW`), feeds stderr to the health rail,
logs unreadable lines, reports a message that is not JSON-RPC host-side only, and stops the whole
tree (`taskkill /T /F`) after a 2 s grace. `McpHttpClient` implements `rmcp`'s
`StreamableHttpClient` over the shared `reqwest` client, sends static headers verbatim (secret
values resolved at launch, never stored or logged) and names a 401/403 in the error. The client
asks for `2025-06-18` and accepts three protocol versions. Stdio servers get the
`mcp-toolchain` env under app data, a server's own env wins. A start that fails is never
restarted; a server that dies is restarted with backoff (1 s doubling to 60 s, five attempts).
Discovered tools are registered as `mcp` tools and kernel nodes under `namespace.tool`;
`dispose()` (shutdown stage `Dispose`) stops every server. Tests drive
`test/helpers/fakeMcpServer.mjs` with `node` (stdio, including a `.cmd` shim) and an axum
endpoint (HTTP).

## Chat

`ChatService` registers `chat.prepare`, `chat.generate` (gated on
`llm.generate`, which a chat run admits itself) and `chat.persist` on the shared registry, so the
composition root builds it before `recover()`. `send` checks the budget for the system prompt and
the current turn first, then starts the turn graph with `RunService::start_with`: the user message
and the run row commit in one transaction, and the token observer is attached before the scheduler
task exists. The graph is one `vector.search` per attached store (query from `retrieval_query`),
then prepare, generate and persist. Prepare merges the hits (`merge_retrieval`, per-store floor),
fetches ±1 neighbours when the vector service has chunks, follows the graph for related documents
when the conversation asks, windows the history (`window_chat_history`) and inserts the partial
assistant message. Each token is saved as it arrives, so cancellation, a provider failure and
shutdown keep the partial answer; recovery marks the turn `interrupted` and never repeats it. A
conversation runs one turn at a time, and removal and truncation refuse a busy one. The title is
derived from the first question after the first answer.

The tool loop offers the conversation's picked tools that are enabled, available and not `off`,
under wire-safe names. It runs at most 4 rounds and 8 calls, then asks for an answer with the
budget note. Each call is a kernel step `generate.callN` through `StepContext::invoke_tool`, which
admits it as a model call (`AdmissionOrigin::Model`, the ceiling and the run gate apply, a
destructive tool needs at least `ask`) and reports approvals to the call record. Tool results are
always fenced. Web adapters apply their `ToolDialect` inside the driver and emit native tool-call
deltas, several per reply, so services never branch on `ToolMode::Injected`.
`KernelServices` gains `graph` (`GraphNeighbors`) and `VectorSearch` gains `chunks` and
`related_chunks` behind `has_chunks` / `has_related_chunks`.

## Browser

`browser::policy` holds the rules the browser workspace in `zvs-studio` enforces. Only
`http:`/`https:` URLs without credentials are web URLs, and `navigation_url` turns typed text
into one (`example.com` and `localhost:8123` get `https://`). The work profile is the WebView2
user-data directory `<app data>/browser/work`. `BrowserLifecycle` delivers `link-finished`,
`link-tab-closed`, `workspace-closed` and `cookies-cleared` to subscribers; dropping the
`LifecycleSubscription` unsubscribes.

Agent automation (TASK_046) is in `browser::{capability, grants, credentials, page, automation,
nodes}`. The `browser.*` nodes depend only on `AutomationPort`, which `BrowserWorkspace`
implements over CDP; `page` turns an accessibility tree into the outline and the per-tab handle
table, and `credentials::secret_field` is the field refusal the driver applies. The tiers, the
`browser_grant` table and the approval shape are in PASSPORT §21.
