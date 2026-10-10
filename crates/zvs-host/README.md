# zvs-host

The Rust host. Every service, repository, driver and the execution kernel will live here
(PASSPORT §5, §8). The crate has **no Tauri dependency**, so all of it is unit-testable
without a window. Shell capabilities reach it through port traits that `zvs-studio`
implements, and `apps/studio/src-tauri/src/lib.rs` is the composition root that builds it.

| Module                                 | Contents                                                                        |
| -------------------------------------- | ------------------------------------------------------------------------------- |
| `ipc`                                  | `IpcServer`, `register`, and one file of thin handlers per domain               |
| `services`                             | `Services`, the handlers' only dependency, and the business logic               |
| `platform::paths`                      | `PathEnvironment` → `StudioPaths`, plus the validated path builders             |
| `platform::logger`                     | `Logger` trait, `FileLogger` (JSON lines, rotation, redaction), `silent_logger` |
| `platform::ids`                        | UUID v7 ids and the injectable `IdGenerator`                                    |
| `platform::clock`                      | `Clock`, `SystemClock`, `FakeClock`                                             |
| `platform::events`                     | `EventBus`, `StreamHandle`, the `EventSink` and `EventRecorder` ports           |
| `platform::recorder`                   | `FileEventRecorder` and the `ZVS_RECORD_EVENTS` flag                            |
| `testing`                              | `temporary_directory`, `MemoryLogger`, `MemorySink`, `FakeClock`                |
| `kernel`, `data`, `drivers`, `browser` | Empty until their port tasks (TASK_061 onwards)                                 |

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
