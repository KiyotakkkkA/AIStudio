---
task: TASK_068
title: zvs-core in-process, sidecar driver in Rust, device and resource probes, system.* channels
phase: M — Migration to Tauri
status: done
depends_on: [TASK_067]
unblocks: [TASK_069]
estimate: 4 h
---

# TASK_068 — zvs-core in-process, sidecar driver in Rust, device and resource probes, system.* channels

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Old D1 split native work between a **napi-rs addon** (`crates/zvs-napi`, loaded by the TS host
as `zvs-core.node`, wrapped by `host/drivers/rust/RustCore.ts`, all JSON across the boundary)
and the **`zvs-jobd` sidecar** (a separate process speaking newline-delimited JSON-RPC, driven
by `host/drivers/sidecar/SidecarDriver.ts`, which spawns, health-checks, restarts with backoff
and hard-kills on cancel). The split stays. The napi layer disappears, because the host is
Rust: `zvs-host` depends on `zvs-core` directly. What the addon gave us still has to hold:
- **No blocking of the async host.** Synchronous core calls run on `spawn_blocking`.
- **A panic costs one call, not the app.** Each port method is wrapped in `catch_unwind` and
  maps a panic to `NATIVE_ERROR`, as the addon's `guarded` did.
- **Cancellation.** The upsert-token dance (`vectorBeginUpsert`/`vectorCancel`/
  `vectorRelease`) becomes a `CancellationToken` passed straight into `zvs-core`.

This task also ports the platform probes that fed `system.device` and `system.resources`:
- `platform/device.ts` holds `DeviceProbe` and the tier heuristic.
- `platform/gpu.ts` asked Electron's `app.getGPUInfo`. Under Tauri it becomes **DXGI adapter
  enumeration** (`IDXGIFactory1::EnumAdapters1`), which also gives real dedicated VRAM, so
  `vramBytes` stops being `null`.
- `platform/metrics.ts` holds `ResourceMonitor`, CPU totals and the `nvidia-smi` sampler.
- `downloads/disk.ts` provides `freeBytes`, which the device probe needs.

The `system.*` channels this task covers are `nativePing` (now an in-process chunk count, same
response shape), `device` and `resources`. `system.demoJob` waits for the kernel (TASK_070).

## Why

Vector search, graph traversal, indexing, OCR, downloads and the device-aware runtime plan all
sit on these three things: the core port, the sidecar driver and the device probe.

## Depends on

- `TASK_067` — the AI layer is complete. Embeddings are needed by vector stores next.

## Read first

- `/docs/PASSPORT.md` §10 (Rust boundary, now in-process plus sidecar)
- `apps/studio/src/host/drivers/rust/` (`RustCore.ts`, `ports.ts`, `vectorTypes.ts`,
  `graphTypes.ts`, `addon.ts`) and `crates/zvs-napi/README.md`: every semantic to keep
- `apps/studio/src/host/drivers/sidecar/SidecarDriver.ts`, `protocol.ts`,
  `crates/zvs-jobd/README.md`
- `apps/studio/src/host/platform/device.ts`, `gpu.ts`, `metrics.ts`,
  `services/SystemService.ts`, `ipc/system.ts`
- `apps/studio/tests/rust.test.ts`, `sidecar.test.ts`, `metrics.test.ts`

## Scope

**In scope**
- `zvs-host::drivers::core`: the `TextCore` (chunk, chunk segments, hash), `VectorCore` and
  `GraphCore` traits, with the same operations and semantics as `RustCorePort`,
  `VectorCorePort` and `GraphCorePort`. `InProcessCore` implements them over `zvs-core` with
  `spawn_blocking` and `catch_unwind`. The process-wide graph registry keyed by `graphId`
  moves here from `zvs-napi`.
- `zvs-host::drivers::sidecar::SidecarDriver`: a `tokio::process` child with a hidden window
  (`CREATE_NO_WINDOW`), NDJSON framing, `run(job, params, { signal, onProgress })`, `ping`,
  health interval, restart with backoff, hard kill on cancel, stderr to the log, and
  `dispose`. Behaviour identical to the TS driver.
- Sidecar location: in development `target/<profile>/zvs-jobd(.exe)`. Packaged, it is a Tauri
  `bundle.externalBin` placed next to the main executable. `StudioPaths::sidecar_path`
  resolves both cases.
- `platform::device` and the tier heuristic, `platform::gpu` with DXGI on Windows
  (`zvs-studio/src/platform/windows/dxgi.rs`, scoped `unsafe`, exposed to `zvs-host` as a
  `GpuProbe` port) and a `None` stub elsewhere, `platform::metrics` (CPU totals via
  `sysinfo`, pinned, plus the `nvidia-smi` sampler), and `free_bytes`
- `SystemService`, and `system.nativePing`, `system.device` and `system.resources`
- `ShutdownCoordinator` extended with `sidecar.dispose()`

**Out of scope** (do not do these, even if tempting)
- Deleting `crates/zvs-napi` or `scripts/build-rust.mjs`'s addon half. The Electron host still
  needs them until TASK_076.
- Moving work between the in-process core and the sidecar. The D1 table is unchanged.
- Vector store and graph services (TASK_069)

## Steps

1. Core traits and `InProcessCore`, ported with the native tests from `rust.test.ts` and
   `test/native/`. Panic recovery is proved with a `debug-panic` feature on `zvs-host`
   mirroring the addon's, never compiled into release.
2. Cancellation semantics for upsert, matching the core README.
3. `SidecarDriver`, with `sidecar.test.ts` ported against a real `zvs-jobd` built in the test
   profile: spawn, progress, cancel kills, crash and restart, health.
4. DXGI probe and port. Test the heuristic on synthetic adapter lists, and run the real probe
   on Windows only.
5. Metrics and device probes, ported with their tests.
6. `SystemService` and the channels. Wire everything in the composition root.

## Verify

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm build:tauri
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, `window.zvs.call("system.nativePing", { text: "word ".repeat(600) })`
returns `{ count: 3 }`. Vector Stores' machine metrics show the CPU, the GPU with its real
VRAM, and the device tier. Killing `zvs-jobd.exe` in Task Manager is followed by a logged
restart.

## Definition of done

- [x] `zvs-core` is called in-process behind ports, with no blocking, panic isolation and token
      cancellation
- [x] `SidecarDriver` is at parity: spawn, health, backoff, hard kill and disposal
- [x] The GPU probe uses DXGI, and `vramBytes` is real on Windows
- [x] `system.nativePing`, `system.device` and `system.resources` work under Tauri
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-host::drivers::core`: `TextCore`, `VectorCore` and `GraphCore` with the operations of
  `RustCorePort`, `VectorCorePort` and `GraphCorePort`, re-exporting `zvs-core`'s row, hit,
  stats, node, edge and traversal types. `InProcessCore` runs sync work on `spawn_blocking`,
  wraps every call in `catch_unwind` (`NATIVE_ERROR`, «Rust operation panicked»), maps core
  errors by code, validates `ChunkConfig` and graph input as `RustCore.ts` did, and owns the
  `GraphRegistry` keyed by `graphId`. `upsert` takes a `CancellationToken` and passes it into
  `VectorIndex::upsert`. A `debug-panic` feature adds `debug_panic`, `vector_debug_panic` and
  `graph_debug_panic`. Tests port `test/native/addon.test.mjs` (Unicode chunking, hashing,
  400 vectors, filters, scoped search, replace, delete, cancellation, every graph traversal,
  hydration, a panic under the registry lock, host responsiveness) and `rust.test.ts`.
- `zvs-host::drivers::sidecar`: `SidecarDriver` over `tokio::process` with
  `CREATE_NO_WINDOW`, the NDJSON protocol, `run(job, params, JobRunOptions { cancel,
  on_progress })` behind a `SidecarJobs` port, `ping`, the 5 s health check, backoff
  restarts, the cancel grace and hard kill, stderr to the log, the 8 MiB line cap and
  `dispose`. Its tests are in `crates/zvs-jobd/tests/driver.rs` against the real binary
  (`CARGO_BIN_EXE_zvs-jobd`): concurrency and progress, job errors, cancel, a job that ignores
  cancel, a crash, an external kill, a deaf sidecar, ping and disposal.
- `zvs-jobd`: debug-only `debug.crash` and `debug.hang` jobs and a `--deaf` flag, the
  stand-ins for `test/helpers/fakeSidecar.mjs`. `serve_with` and `ServeOptions` carry the flag.
- `platform::gpu` (`GpuProbe` port, `GpuAdapter`, `select_gpu`, `NoGpu`),
  `platform::device` (`DeviceProbe`, `os_readings` via `sysinfo`, `device_tier`),
  `platform::metrics` (`ResourceMonitor`, `SysinfoCounters`, `NvidiaSmi`, `watch`) and
  `platform::disk::free_bytes`, each with tests ported from `platform.test.ts` and
  `metrics.test.ts`.
- `zvs-studio/src/platform/windows/dxgi.rs`: `DxgiProbe` (`CreateDXGIFactory1`,
  `EnumAdapters1`, `GetDesc1`), so `vramBytes` is the adapter's dedicated memory. Non-Windows
  builds use `NoGpu`.
- `SystemService` gains the core, device and resources (defaults plus `with_*`), and
  `system.nativePing`, `system.device` and `system.resources` are served. `compose` builds the
  sidecar, the DXGI-backed device probe (free disk under `downloads`), the `nvidia-smi`
  monitor, registers `sidecar` in the `Dispose` stage, and pings the sidecar once at startup.
- `StudioPaths::sidecar_path` (`PathEnvironment::executable_dir`), `scripts/tauri.mjs` behind
  `dev:tauri` and `build:tauri`.

**Deviated from plan:**

- `bundle.externalBin` is not in `tauri.conf.json`. tauri-build fails when the external binary
  is missing and copies it over `target/<profile>/zvs-jobd.exe`, so every `cargo` gate would
  depend on a pre-staged `binaries/zvs-jobd-<triple>.exe`. Instead the sidecar is built by
  Cargo next to the main executable (`scripts/tauri.mjs` builds `zvs-jobd` in the matching
  profile first), and `sidecar_path` is "next to the executable" in both layouts. TASK_056
  adds `externalBin` with its staging step when bundling is turned on.
- `sysinfo` is pinned to 0.38.4, not the latest 0.39.6: 0.39 needs rustc 1.95 and the
  toolchain is 1.93.1.
- `sysinfo` has no raw CPU tick totals, so `ResourceCounters::cpu_percent` returns its
  between-refresh load and the monitor only rounds and clamps it. The TS delta-of-totals test
  became a clamp and rounding test.
- `select_gpu` picks the adapter with the most dedicated memory and calls it discrete when it
  is NVIDIA, has at least 2 GiB, or sits next to a weaker adapter. The Electron probe called
  every AMD adapter discrete; an AMD APU with a small UMA frame is now integrated.
- A cancelled `nvidia-smi` probe no longer disables the sampler; only a failed one does.
- `dispose` rejects jobs still pending with `SIDECAR_UNAVAILABLE`; the TS driver left them
  pending forever.
- The sidecar's missing-binary message points to `cargo build -p zvs-jobd`, not
  `pnpm build:rust`.
- `compose` pings the sidecar once at startup so the health check runs (and a missing binary is
  logged) before TASK_070 gives it jobs.

**Next session should know:**

- `zvs-studio` now links `zvs-core`, so `pnpm dev:tauri` and `pnpm build:tauri` need
  `protoc`. `scripts/tauri.mjs` sets `PROTOC` from `target/tools/protoc` like `build-rust.mjs`;
  direct `cargo` commands still need `$env:PROTOC` set.
- TASK_069: take `SharedVectorCore` and `SharedGraphCore` from one `InProcessCore` built in
  `compose` (the graph registry lives in that instance). Pass the run's token into `upsert`.
- TASK_070: the `SidecarDriver` built in `compose` is the `SidecarJobs` for `job.run` and
  `system.demoJob`; move it into `Services` or the kernel there.
- TASK_071 and TASK_072: `services.system.device()` and `services.system.resources()` return
  the shared `DeviceProbe` and `ResourceMonitor` for the runtime plan and indexing.
- Smoke run of the release build on 2026-10-16: `Started the sidecar` logged, killing
  `zvs-jobd.exe` logged `The sidecar exited; it restarts on the next job`, and `sidecar` ran
  in the `Dispose` stage. Not checked by hand: the DevTools `system.nativePing` call and the
  Vector Stores machine metrics. The IPC test returns `{ count: 3 }` for `"word ".repeat(600)`,
  and the DXGI test enumerated this machine's adapters.
- Gates run on Windows on 2026-10-16, all exit 0: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` (also `zvs-host --features
  debug-panic` and `zvs-jobd --release`), `cargo test --workspace`, `pnpm typecheck`,
  `pnpm lint`, `pnpm format:check`, `pnpm build`, `pnpm build:tauri`, `npx vitest run`
  (1232 passed, 2 skipped) and `node scripts/contract-channels.mjs --check`.
