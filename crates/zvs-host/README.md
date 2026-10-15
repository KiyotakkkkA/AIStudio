# zvs-host

The Rust host. Every service, repository, driver and the execution kernel will live here
(PASSPORT §5, §8). The crate has **no Tauri dependency**, so all of it is unit-testable
without a window. Shell capabilities reach it through port traits that `zvs-studio`
implements, and `apps/studio/src-tauri/src/lib.rs` is the composition root that builds it.

| Module                   | Contents                                                                                                          |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| `ipc`                    | `IpcServer`, `register`, and one file of thin handlers per domain                                                 |
| `services`               | `Services`, the handlers' only dependency, and the business logic                                                 |
| `data`                   | `Database`, migrations, row structs and repositories (see `src/data/README.md`)                                   |
| `platform::paths`        | `PathEnvironment` → `StudioPaths`, plus the validated path builders                                               |
| `platform::logger`       | `Logger` trait, `FileLogger` (JSON lines, rotation, redaction), `silent_logger`                                   |
| `platform::ids`          | UUID v7 ids and the injectable `IdGenerator`                                                                      |
| `platform::clock`        | `Clock`, `SystemClock`, `FakeClock`                                                                               |
| `platform::events`       | `EventBus`, `StreamHandle`, the `EventSink` and `EventRecorder` ports                                             |
| `platform::recorder`     | `FileEventRecorder` and the `ZVS_RECORD_EVENTS` flag                                                              |
| `platform::theme`        | `ThemePort`, `ThemeSource`, `apply_theme_source`, `THEME_SETTING_KEY`                                             |
| `platform::window_state` | `read_window_state`, `capture_window_state`, `WINDOW_STATE_KEY`                                                   |
| `platform::shutdown`     | `ShutdownCoordinator`, `ShutdownStage`, `ExitDecision`                                                            |
| `platform::navigation`   | `WorkspaceRoute`, the `WorkspaceNavigator` port (implemented by `EventBus`)                                       |
| `platform::device`       | `DeviceProbe`, `DeviceReadings`, `os_readings`, `device_tier`                                                     |
| `platform::gpu`          | The `GpuProbe` port, `GpuAdapter`, `select_gpu`, `NoGpu`                                                          |
| `platform::metrics`      | `ResourceMonitor`, `ResourceCounters` (`SysinfoCounters`), the `GpuSampler` port, `NvidiaSmi`                     |
| `platform::disk`         | `free_bytes`                                                                                                      |
| `services::crypto`       | `CryptoBackend`, `CryptoService`, `Plaintext`, `UnavailableBackend`                                               |
| `services::secrets`      | `SecretService`, the summary and DTO mapping, `secret_policy`, `secret_fields`                                    |
| `services::providers`    | `ProviderService`, `DriverSource`, `probe_outcome`, `provider_status`                                             |
| `services::health_check` | `HealthCheckService`, the `providers.healthcheck.*` settings keys                                                 |
| `services::accounts`     | `AccountService`, the `AccountBrowser` port, `mask_email`                                                         |
| `drivers::ai`            | AI ports, `HttpClient`, transports, adapters, `ProviderRegistry` (see below)                                      |
| `drivers::core`          | `TextCore`, `VectorCore`, `GraphCore`, `InProcessCore`, `GraphRegistry`                                           |
| `drivers::sidecar`       | `SidecarDriver`, `SidecarJobs`, `JobRunOptions`, the NDJSON protocol                                              |
| `testing`                | `temporary_directory`, `temporary_database`, `test_services`, `FakeCrypto`, `FakeDriver`, `FakeSessionGateway`, … |
| `browser::policy`        | `is_web_url`, `navigation_url`, `profile_dir`, `BROWSER_PARTITION`, the heights, `TAB_LIMIT`                      |
| `browser::lifecycle`     | `BrowserLifecycle`, `BrowserLifecycleEvent`, `LifecycleSubscription`                                              |
| `kernel`                 | Empty until its port task                                                                                         |

`testing` is compiled for this crate's own tests and behind the `testing` feature for other
crates' tests. `debug-panic` adds `InProcessCore::debug_panic`, `vector_debug_panic` and
`graph_debug_panic` for the panic-recovery test; release builds never enable it.

## Core and sidecar

`zvs-core` is linked in-process and reached only through the `drivers::core` ports (PASSPORT
§10). `InProcessCore` implements all three. Synchronous work (chunking, hashing, every graph
call, removing a vector store) runs on `spawn_blocking`; vector calls are LanceDB futures. Each
call is wrapped in `catch_unwind`, so a panic answers `NATIVE_ERROR` («Rust operation
panicked») and the next call works. Core errors keep their code (`core_error`), as the addon's
JSON envelope did. `ChunkConfig` is validated before the core is reached
(`VALIDATION_FAILED`, «Некорректный размер или перекрытие фрагментов»), and graph ids, nodes,
edges and queries are trimmed and checked against `MAX_HOPS` and `MAX_NODE_BUDGET`
(«Некорректный запрос к графу»). `upsert` takes the caller's `CancellationToken` and passes it
into `VectorIndex::upsert`; a token that is already cancelled answers `RUN_CANCELLED` without
touching the store. The graph registry keyed by `graphId` lives in `InProcessCore`, so the
composition root's single instance is the process-wide registry.

`SidecarDriver` is `SidecarDriver.ts` over `tokio::process`: the child is spawned on demand
with `CREATE_NO_WINDOW`, requests and answers are newline-delimited JSON (a line over 8 MiB
kills the child), stderr is logged as `Sidecar diagnostics`, and a ping every 5 s (2 s timeout)
kills a silent child. `run(job, params, JobRunOptions { cancel, on_progress })` sends
`job.cancel` when the token fires and kills the process if the job is still running after the
2 s grace. An exit fails every pending job with `SIDECAR_UNAVAILABLE` and schedules the next
spawn after `250 ms × 2^restarts` (capped at 30 s); a ping answer resets the count. `dispose`
kills the child and waits for it, after which `run` answers `CONFLICT`.
`StudioPaths::sidecar_path` is `zvs-jobd(.exe)` next to the main executable: `target/<profile>`
in development, the `externalBin` copy when packaged. The driver's tests live in
`crates/zvs-jobd/tests/driver.rs`, so they run against the real binary Cargo built for them.

## System

`SystemService` answers `system.nativePing` (an in-process chunk count with the default
config), `system.device` and `system.resources`. `DeviceProbe` measures once, caches, and
measures again on `refresh`; a failing GPU or disk probe costs only its own field. The tier
rule is the TypeScript one: ≥ 30 GiB and ≥ 12 cores is `high`, ≥ 14 GiB and ≥ 6 cores is
`medium`, and a discrete GPU lifts the tier one step. `select_gpu` drops software adapters,
picks the adapter with the most dedicated memory, and calls it discrete when it is NVIDIA, has
at least 2 GiB, or sits beside a weaker adapter. `ResourceMonitor` reads CPU, memory and the
process's resident memory through `sysinfo`, and GPU counters through `nvidia-smi`, which is
tried once and then left alone on a machine without it; a missing reading is `null`, never 0.

## IPC server

`IpcServer::handle::<I, O, _, _>(channel, handler)` registers one contract channel. A handler
is `async fn(Arc<Services>, I) -> Result<O, AppError>`. `dispatch(channel, payload)` returns
the wire envelope as a `serde_json::Value`, exactly as `@zvs/ipc`'s `createIpcServer` does:

| Failure                                | Envelope                                                       | Log                                                  |
| -------------------------------------- | -------------------------------------------------------------- | ---------------------------------------------------- |
| Payload fails serde or `Validate`      | `VALIDATION_FAILED`, `Некорректные данные запроса`, no details | `warn` `Rejected invalid input`                      |
| Handler returns `AppError`             | The error as is, details included                              | `error` `Handler failed`                             |
| Handler panics                         | `UNKNOWN`, `Внутренняя ошибка`                                 | `error` `Handler failed` with the panic message      |
| Output fails `Validate` (debug builds) | `UNKNOWN`, `Внутренняя ошибка`                                 | `error` `Handler output does not match the contract` |
| Channel has no handler                 | `NOT_IMPLEMENTED`, naming the channel                          | `debug` `Called a channel that is not implemented`   |

A panic is caught with `FutureExt::catch_unwind`, so it costs one call and leaves the server
usable. Registering a name outside `zvs_contract::CHANNELS`, or the same name twice, panics at
startup. `finish()` writes one log line listing the channels still answering
`NOT_IMPLEMENTED`. `ipc::tests::every_contract_channel_is_served` is `#[ignore]`d until
TASK_076, when it becomes strict.

## Logger

`logs/studio.jsonl`, rotated to `studio.1.jsonl` … `studio.3.jsonl` at 5 MiB. A line has the
TypeScript host's shape and key order: the caller's fields first, then `timestamp`, `level`,
`scope`, `message`, and `raw` (always present at `warn` and `error`). Strings are redacted
with the same key shapes as `logger.ts`. Fields are passed as a slice of `(&str, Value)` so
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
follow-ups. Field validation against a type schema (`services::secret_fields`) reproduces
`buildFieldsSchema`: strict keys, defaults, and `z.url()` through the WHATWG `url` crate.

## AI drivers

`drivers::ai` is the API-mode half of `host/drivers/ai` (PASSPORT §12), plus the
`SessionGateway` port account mode is built on. `AccountTransport`, `qwen-web`,
`deepseek-web` and the identity probes land in TASK_067.

| Module             | Contents                                                                                                          |
| ------------------ | ----------------------------------------------------------------------------------------------------------------- |
| `ports`            | `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`, `AiDriver`, the request/result types                    |
| `capabilities`     | `AdapterCapabilities`, `Honours`, `supports_auth_mode`, `unsupported_parameters`                                  |
| `http`             | `HttpClient`: one `reqwest::Client`, rustls with the OS roots, the system proxy, no cookies                       |
| `transport`        | `Transport`, `RequestSpec`, `TransportResponse`, `join_url`, `ApiTransport`, `AccountTransport`, `SessionGateway` |
| `sse`              | Streaming UTF-8 decoding, `sse_data` (stops at `[DONE]`) and `ndjson_lines`                                       |
| `errors`           | `http_failure`, `network_failure`, `cancelled`, `timed_out`, `session_expired`, raw text                          |
| `adapters`         | `openai-compatible`, `ollama`, `local`, `qwen-web`, `deepseek-web`, the family table                              |
| `identity`         | Identity probes per account family, `ProbeAccountCredentials`, `AccountCredentialStore`                           |
| `injected_tools`   | The `zvs_call_tool` prompt protocol and `with_injected_tools`                                                     |
| `stream_to_events` | Deltas re-emitted as `token` events, ended with the run outcome                                                   |
| `registry`         | `ProviderRegistry`, `ProviderSource`, `AccountPort`, `DatabaseAccounts`, `UnlinkedAccounts`                       |

Every driver call takes a `tokio_util::sync::CancellationToken`. A fired token ends a request
or a stream with `RUN_CANCELLED` and drops the `reqwest` response, which aborts the vendor
request. The provider timeout (default 60 s) is one deadline from the send, covering the body
and every stream chunk, as `AbortSignal.timeout` did.

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

`transport::session::SessionGateway` is `SessionGateway.ts`: `partition()`, `user_agent()`,
`account_token(url)` and `fetch(url, SessionRequest, &CancellationToken)`. The response is a
`SessionResponse` with the status, lower-cased headers and the body as a `ByteStream`, which
`bytes()` and `text()` collect. `AccountToken` and `SessionRequest` print no secret values.
The host never holds a cookie: `zvs-studio`'s `AccountGateway` runs every request as `fetch()`
inside the browser profile (see `apps/studio/src-tauri/src/browser/gateway/README.md`).
`testing::FakeSessionGateway` answers from per-URL queues (the last reply repeats) or a
responder closure, records requests and the cancel scope of each, can stall or hold a body
open, and holds tokens per origin.

### Account transport

`transport::account::AccountTransport` is `AccountTransport.ts` over a `SessionGateway`. Each
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
(`GET https://chat.qwen.ai/api/v1/auths/`) and `DeepSeekIdentityProbe`
(`GET https://chat.deepseek.com/api/v0/users/current`). Both send the captured bearer when the
gateway has one, give up after 20 s, map a redirect or HTML to `PROVIDER_SESSION_EXPIRED`, and
validate the payload field by field as the Zod schemas did (a malformed profile is
`VALIDATION_FAILED`). A non-zero DeepSeek `code` or `biz_code` is a signed-out session.

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
else `abilities.thinking`), and generates by creating a vendor chat (`/chats/new`) and
streaming `/chat/completions?chat_id=…`. History is flattened into one role-labelled prompt.
`thinking_summary` frames become reasoning deltas (only the new suffix of each summary line),
`answer` frames text deltas; a stream that ends without `status: finished` is
`PROVIDER_UNREACHABLE` «Поток вендора оборвался». Vendor frames with `401`/`403` or an
auth-like code are `PROVIDER_SESSION_EXPIRED`, other error frames `UNKNOWN` «Вендор прервал
генерацию». `deepseek-web` serves a curated two-model list and refuses generation with
`account-mode generation is not implemented yet (TASK_014_EXTRA_3)`.

The registry builds account-mode transports through `DatabaseAccounts`: the provider's
account must exist and be `linked` (else `PROVIDER_SESSION_EXPIRED`, with
`reason: account-not-linked` or the status), the session comes from the `SessionFactory`
(without one, `VALIDATION_FAILED` «Сессия браузера недоступна»), and the credentials are
`ProbeAccountCredentials`. The `#[ignore]`d
`live_qwen_identity_models_and_thinking_and_fast_generation` talks to chat.qwen.ai with
`LIVE_QWEN_TOKEN`.

## Providers

`ProviderService` is `ProviderService.ts`. It reads and writes `provider` and `model` through
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

`AccountService` is `AccountService.ts`. `link(family)` joins a running link for the same
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
exit goes through and `RunEvent::Exit` logs `Shutdown`. Today it holds `health-check.stop`
(`Stop`) and `providers`, `health-check`, `accounts`, `account-gateway` and `sidecar` (`Dispose`). Later ports register their own tasks
in the stage `main.ts` used for them.

## Browser

`browser::policy` holds the rules the browser workspace in `zvs-studio` enforces. Only
`http:`/`https:` URLs without credentials are web URLs, and `navigation_url` turns typed text
into one (`example.com` and `localhost:8123` get `https://`). The work profile is the WebView2
user-data directory `<app data>/browser/work`. `BrowserLifecycle` delivers `link-finished`,
`link-tab-closed`, `workspace-closed` and `cookies-cleared` to subscribers; dropping the
`LifecycleSubscription` unsubscribes.
