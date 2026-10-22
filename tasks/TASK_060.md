---
task: TASK_060
title: zvs-host skeleton — paths, logger, ids, clock, event bus, Rust IPC server, composition root
phase: M — Migration to Tauri
status: done
depends_on: [TASK_059]
unblocks: [TASK_061]
estimate: 4 h
---

# TASK_060 — zvs-host skeleton — paths, logger, ids, clock, event bus, Rust IPC server, composition root

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

After TASK_058 and TASK_059 we have a Tauri window running the real renderer, a bridge that
answers `NOT_IMPLEMENTED`, and a serde mirror of the contract. Nothing yet serves channels the
way `@zvs/ipc`'s `createIpcServer` does. Its job is to parse the input, call one handler, map
failures to the envelope, check exhaustiveness and validate output in development. There is
also no event bus like `host/platform/events.ts`, which opens streams, stamps `seq` and `ts`,
logs and drops events emitted after the stream ends, and feeds the recorder.

This task creates `crates/zvs-host`. That crate is where every service, repository, driver and
the kernel will live. **It has no Tauri dependency**, so all of it is unit-testable without a
window, which is the same rule the TS host followed with Electron (§8). Tauri-specific
capabilities reach it through ports that `zvs-studio` implements. This task also builds the
platform foundation: paths, the JSON-lines logger, UUID v7 ids and an injected clock. It moves
`system.ping` and `system.demoStream` onto the real server, and turns `zvs-studio`'s `lib.rs`
into the composition root.

## Why

Every port task after this one only writes services and handlers and registers them. If the
server, the envelope, the stream semantics or the logger format differed from the TS host, the
renderer would break in ways no single domain port could explain.

## Depends on

- `TASK_059` — `zvs-contract`: the envelope, `HostEvent` and `CHANNELS`.

## Read first

- `/docs/PASSPORT.md` §7, §8, §15
- `packages/ipc/src/server.ts` and `packages/ipc/README.md`: envelope mapping, exhaustiveness,
  `validateOutput`
- `apps/studio/src/host/platform/events.ts`, `eventRecorder.ts`, `logger.ts`, `paths.ts`,
  `ids.ts`
- `apps/studio/README.md`, "The event channel" and "Recording and replay"
- `apps/studio/tests/events.test.ts`, `platform.test.ts`, `packages/ipc/tests/server.test.ts`:
  the behaviours to port

## Scope

**In scope**
- `crates/zvs-host` with the module tree from PASSPORT §5: `ipc/`, `services/`, `kernel/`,
  `data/`, `drivers/`, `platform/`, `browser/` (empty modules are fine)
- `platform::paths`: `StudioPaths` built from an injected `PathEnvironment { data_dir,
  resource_dir, packaged }` with the same derived folders as `paths.ts`
  (`studio.sqlite`, `logs/`, `logs/streams/`, `backups/`, `cache/`, `vectors/`, `downloads/`
  and its kind subfolders) and the same validation helpers (`vector_store_path`,
  `runtime_install_path`, `download_target_path`). There are no preload or renderer paths any more.
- `platform::logger`: JSON lines in `logs/studio.jsonl`, rotating at 5 MiB and keeping three
  backups. The line format is the TS one (`ts`, `level`, `scope`, `message`, fields). The
  directory and clock are injected. Use `tracing` with a custom layer, or a hand-written
  writer, whichever keeps the format exact.
- `platform::ids` (UUID v7) and `platform::clock` (`Clock` trait, `SystemClock`, `FakeClock`
  for tests)
- `platform::events`: `EventBus`, `open_stream()`, `StreamHandle::emit(draft)` and
  `end(outcome)`. Semantics match the TS bus: `seq` starts at 0 per stream, an emit after
  `end` is logged at `error` and dropped, and nothing panics. Delivery goes through an
  `EventSink` trait. With no sink attached, events are logged at `debug` and dropped. When
  `ZVS_RECORD_EVENTS=1` is set, `logs/streams/<date>.jsonl` records every event before
  delivery.
- `ipc::IpcServer`: typed registration (`server.handle::<Input, Output, _>(channel, handler)`).
  The input is deserialised and validated, the handler is async, and any error becomes the
  envelope: `AppError` keeps its code, a serde error becomes `VALIDATION_FAILED`, a panic or
  anything else becomes `UNKNOWN`, and the detail is logged host-side only. Output is
  validated in debug builds. `IpcServer::finish()` compares registered channels to
  `zvs_contract::CHANNELS`. A missing channel is answered with `NOT_IMPLEMENTED`, listed in one
  startup log line, and fails a test that becomes strict at TASK_076.
- `zvs-studio`: the composition root resolves Tauri's `app_data_dir`, `resource_dir` and
  packaged flag into `PathEnvironment`, builds the logger and the bus, and registers handlers.
  `ipc_call` delegates to `IpcServer::dispatch`. The stored `Channel` from `events_subscribe`
  becomes the `EventSink`.
- `system.ping` and `system.demoStream`, ported with their tests
- `test-support` helpers under `crates/zvs-host/src/testing.rs` (`#[cfg(test)]` or a
  `testing` feature): `temporary_directory()` with an assertion that the path is under the OS
  temp dir, and `FakeClock`

**Out of scope** (do not do these, even if tempting)
- SQLite (TASK_061). Settings-backed window state and theme also wait for TASK_061.
- Any domain service
- Changing the event or envelope shapes. They are fixed by `packages/shared`.

## Steps

1. Create the crate and its module tree. Add dependencies at the workspace level and pin them
   (`tokio`, `serde`, `serde_json`, `uuid` with `v7`, `tracing` if used, `thiserror`).
2. Port `paths.ts` and its tests. All path joining lives here: no other module builds a path
   from strings (§15).
3. Port the logger with rotation tests on a temporary directory and a fake clock.
4. Port the event bus and recorder. Port every case in `events.test.ts`.
5. Build `IpcServer`. Handlers receive `(&Services, Input)` and return
   `Result<Output, AppError>`. Wrap each handler in `catch_unwind` (`FutureExt::catch_unwind`
   via `futures`) so a panicking handler costs one call, not the host. Port the
   `server.test.ts` cases that still apply.
6. Composition root in `zvs-studio/src/lib.rs`. `ipc_call` checks the webview label, then
   dispatches. Register `system.ping` and `system.demoStream`.
7. Remove the inline ping from TASK_058.

## Verify

```bash
cargo test -p zvs-host -p zvs-contract
cargo clippy --workspace --all-targets -- -D warnings
pnpm build:tauri
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, the placeholder demo-stream control shows ten progress
events and an `end`. `logs/studio.jsonl` appears under the Tauri app data directory with the
same line shape as the Electron log. With `ZVS_RECORD_EVENTS=1`, a stream file appears, and
`zvsReplay.fromText` replays it in the renderer.

## Definition of done

- [x] `zvs-host` exists with no Tauri dependency and the PASSPORT §5 module tree
- [x] Paths, logger, ids, clock, event bus and recorder are ported with their tests
- [x] `IpcServer` reproduces envelope mapping, panic isolation and the exhaustiveness report
- [x] `system.ping` and `system.demoStream` run end to end through Tauri
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- `crates/zvs-host` (no Tauri dependency) with the §5 tree: `ipc/`, `services/`, `platform/`,
  and empty `kernel/`, `data/`, `drivers/`, `browser/`. README covers the module map and the
  envelope table.
- `platform::paths` (`PathEnvironment` → `StudioPaths`, `DownloadKind`, `vector_store_path`,
  `runtime_install_path`, `download_target_path`, `log_file_path`, `stream_file_path` with a
  dependency-free UTC date), `platform::logger` (`Logger` trait, `FileLogger`, redaction with
  the same key shapes via `regex`), `platform::ids` (UUID v7, `IdGenerator`),
  `platform::clock` (`Clock`, `SystemClock`, `FakeClock`), `platform::events` (`EventBus`,
  `StreamHandle`, `EventSink`, `EventRecorder`) and `platform::recorder`
  (`FileEventRecorder`, `ZVS_RECORD_EVENTS`).
- `ipc::IpcServer`: typed `handle`, `dispatch` returning the envelope, `catch_unwind` panic
  isolation, debug-only output validation (`with_output_validation` overrides it),
  `coverage()` and `finish()`.
- `services::SystemService` (`ping`, `demo_stream`) with thin handlers in `ipc/system.rs`.
- `zvs-studio`: `lib.rs` composes paths, logger, recorder, bus, services and server;
  `ipc_call` checks the label and dispatches; `events_subscribe` attaches the `Channel` as the
  `EventSink`. The inline ping and its tests are gone.
- `testing` module (`cfg(test)` or feature `testing`): `temporary_directory`,
  `assert_temporary_path`, `MemoryLogger`, `MemorySink`, `FakeClock`.

**Deviated from plan:**
- The log line keeps the TS key `timestamp`, not `ts` as the task text says: `logger.ts` and
  its test use `timestamp`, and the TS host is the specification.
- Handlers take `Arc<Services>` instead of `&Services`, so the boxed future can be `'static`
  and `Send` without lifetime gymnastics.
- A validation failure returns no `details`, matching `createIpcServer`; the reason is logged
  host-side only.
- `()` gained a `Validate` impl in `zvs-contract` so `z.void()` inputs and outputs register.
- An unregistered or unknown channel is answered with `NOT_IMPLEMENTED` and a `warn` line.
  Registering a non-contract or duplicate channel panics at startup.
- No sidecar or native-addon paths: the addon is gone, and the sidecar path arrives with its
  driver port.

**Next session should know:**
- New domains: add the service to `Services`, a `ipc/<domain>.rs` with `register`, and call
  it from `ipc::register`. The strict coverage test is `ipc::tests::every_contract_channel_is_served`
  (ignored until TASK_076).
- Log fields are `&[(&str, Value)]` so key order matches the TS lines.
- `cargo fmt --check` already fails on `crates/zvs-core/src/pdf.rs` on `main`; it was left
  alone. Local `cargo` needs `PROTOC=target/tools/protoc/bin/protoc.exe` for LanceDB.
- Window state and theme from settings still wait for TASK_061.
