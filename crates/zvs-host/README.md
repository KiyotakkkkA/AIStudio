# zvs-host

The Rust host. Every service, repository, driver and the execution kernel will live here
(PASSPORT §5, §8). The crate has **no Tauri dependency**, so all of it is unit-testable
without a window. Shell capabilities reach it through port traits that `zvs-studio`
implements, and `apps/studio/src-tauri/src/lib.rs` is the composition root that builds it.

| Module                   | Contents                                                                        |
| ------------------------ | ------------------------------------------------------------------------------- |
| `ipc`                    | `IpcServer`, `register`, and one file of thin handlers per domain               |
| `services`               | `Services`, the handlers' only dependency, and the business logic               |
| `data`                   | `Database`, migrations, row structs and repositories (see `src/data/README.md`) |
| `platform::paths`        | `PathEnvironment` → `StudioPaths`, plus the validated path builders             |
| `platform::logger`       | `Logger` trait, `FileLogger` (JSON lines, rotation, redaction), `silent_logger` |
| `platform::ids`          | UUID v7 ids and the injectable `IdGenerator`                                    |
| `platform::clock`        | `Clock`, `SystemClock`, `FakeClock`                                             |
| `platform::events`       | `EventBus`, `StreamHandle`, the `EventSink` and `EventRecorder` ports           |
| `platform::recorder`     | `FileEventRecorder` and the `ZVS_RECORD_EVENTS` flag                            |
| `platform::theme`        | `ThemePort`, `ThemeSource`, `apply_theme_source`, `THEME_SETTING_KEY`           |
| `platform::window_state` | `read_window_state`, `capture_window_state`, `WINDOW_STATE_KEY`                 |
| `services::crypto`       | `CryptoBackend`, `CryptoService`, `Plaintext`, `UnavailableBackend`             |
| `services::secrets`      | `SecretService`, the summary and DTO mapping, `secret_policy`, `secret_fields`  |
| `drivers::ai`            | AI ports, `HttpClient`, transports, adapters, `ProviderRegistry` (see below)    |
| `testing`                | `temporary_directory`, `temporary_database`, `test_services`, `FakeCrypto`, …   |
| `kernel`, `browser`      | Empty until their port tasks                                                    |

`testing` is compiled for this crate's own tests and behind the `testing` feature for other
crates' tests.

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

`drivers::ai` is the API-mode half of `host/drivers/ai` (PASSPORT §12). Account mode
(`AccountTransport`, `qwen-web`, `deepseek-web`, identity probes) lands in TASK_065 – TASK_067.

| Module             | Contents                                                                                       |
| ------------------ | ---------------------------------------------------------------------------------------------- |
| `ports`            | `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`, `AiDriver`, the request/result types |
| `capabilities`     | `AdapterCapabilities`, `Honours`, `supports_auth_mode`, `unsupported_parameters`               |
| `http`             | `HttpClient`: one `reqwest::Client`, rustls with the OS roots, the system proxy, no cookies    |
| `transport`        | `Transport`, `RequestSpec`, `TransportResponse`, `join_url`; `transport::api::ApiTransport`    |
| `sse`              | Streaming UTF-8 decoding, `sse_data` (stops at `[DONE]`) and `ndjson_lines`                    |
| `errors`           | `http_failure`, `network_failure`, `cancelled`, `timed_out`, `session_expired`, raw text       |
| `adapters`         | `openai-compatible`, `ollama`, `local`, and the family table (`adapter_entry`)                 |
| `injected_tools`   | The `zvs_call_tool` prompt protocol and `with_injected_tools`                                  |
| `stream_to_events` | Deltas re-emitted as `token` events, ended with the run outcome                                |
| `registry`         | `ProviderRegistry`, `ProviderSource`, `AccountPort`, `UnlinkedAccounts`                        |

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
