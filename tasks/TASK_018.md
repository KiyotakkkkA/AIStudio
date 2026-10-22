---
task: TASK_018
title: napi-rs addon — build, load, panic guard, first async call
phase: 3 — Rust core and vector search
status: done
depends_on: [TASK_017]
unblocks: [TASK_019]
estimate: 4 h
---

# TASK_018 — napi-rs addon — build, load, panic guard, first async call

## Context

`zvs-core` exists but nothing in the app can call it. This task builds `crates/zvs-napi` — the napi-rs addon that the Electron host loads — and gets one real function crossing the boundary.

Three rules from PASSPORT §10 govern the addon and all of them matter. **Every exported function is async**, so napi-rs runs it on a tokio pool and the host's event loop never blocks — a synchronous vector search would freeze the UI. **Panics never cross the boundary**: each entry point wraps its work so a Rust panic becomes an ordinary error rather than a hard crash of the whole application. **Payloads are plain JSON** across the boundary, with buffers reserved for genuinely binary data.

The unglamorous part of this task is the build and load path, and it is where the time goes. The addon compiles per target triple, the artefact has to end up somewhere the host can find it in both dev and packaged layouts, and `paths.ts` is the only place allowed to know where that is.

## Why

Every fast Rust capability — vector search above all — reaches the app through this addon. If the panic guard and the async discipline are not established with the first function, they will not be applied to the fiftieth.

## Depends on

- `TASK_017` — Cargo workspace, zvs-core skeleton, Rust CI.

## Read first

- `/docs/PASSPORT.md` §10 (addon rules), §15 (paths resolved in one place), §16 (packaging native artefacts)

## Scope

**In scope**
- `crates/zvs-napi` wrapping `zvs-core`
- Async exports on the tokio pool, with `catch_unwind` at every entry point
- Error conversion: crate error → JS error carrying the stable code
- Build script producing the `.node` artefact; load path resolved through `paths.ts`
- A typed TypeScript wrapper in `host/drivers/rust/` so services never touch the raw addon
- `chunk` and `hash` exposed end to end, with tests

**Out of scope** (do not do these, even if tempting)
- LanceDB (TASK_019) — this task proves the pipe, not the payload
- The sidecar (TASK_027)
- Packaging for other platforms (TASK_056) — build for the dev machine's triple

## Steps

1. Create `crates/zvs-napi` with `napi` and `napi-derive`. Add it to the workspace members.
2. Export `chunk_text` and `hash_bytes` as `#[napi]` async functions delegating to `zvs-core`.
3. Panic guard: a small wrapper used by every export that catches unwinds and converts them to a napi error with code `NATIVE_ERROR`. Test it with a function that panics deliberately, kept behind a debug feature flag.
4. Error conversion: implement `From<CoreError> for napi::Error`, preserving the stable code string so the host can map it onto `AppErrorCode`.
5. Build: `napi build --platform --release`, output into `apps/studio/resources/native/<triple>/`. Add the artefact path to `.gitignore` — it is built, not committed.
6. Loading: `host/drivers/rust/addon.ts` resolves the `.node` path via `paths.resourcesDir()` and loads it once. If loading fails, throw a clear diagnostic naming the expected path and the build command — this is the error a future contributor will hit first, so make it a good one.
7. Typed wrapper `host/drivers/rust/RustCore.ts`: our own interface over the addon, so services depend on our types. Mirrors the driver-port pattern from TASK_014.
8. Wire a temporary `system.nativePing` channel that chunks a string through Rust and returns the count, proving the whole path from renderer to Rust and back.
9. Tests: wrapper maps native errors to `AppErrorCode`; a deliberate panic surfaces as an error and the process survives; loading failure produces the diagnostic message.

## Verify

```bash
pnpm build:rust
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Call `system.nativePing` from DevTools and confirm a real chunk count comes back. Trigger the deliberate panic path and confirm the app stays alive with an error logged.

## Definition of done

- [x] Addon builds and loads in dev
- [x] All exports async; host thread never blocks
- [x] Panic in Rust becomes an error, not a crash — demonstrated
- [x] Load path resolved only through `paths.ts`, with a helpful failure message
- [x] Typed wrapper in place; no service imports the raw addon
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `crates/zvs-napi` with async chunkText/hashBytes, owned JSON chunk results, Buffer hashing, catch_unwind guards, stable error mapping, and a feature-gated debugPanic export. The napi build script produces the current host triple under resources/native, with a stable zvs-core.node filename and a separate debug artifact. Central paths resolve development and packaged resource layouts; the cached lazy loader reports the expected path and build command on failure. RustCorePort/RustCore and SystemService keep services off the raw addon. The validated system.nativePing channel is wired through preload and Electron. Native CI checks cover both variants; crate README documents build, error transport, layout and debugging. No code comments added.

**Deviated from plan:** Rust's orphan rule requires a local CoreFailure adapter rather than implementing From<zvs_core::Error> directly for napi::Error. The napi async API accepts fixed Status codes, so native rejection messages carry a structured JSON code/message envelope decoded into AppError by RustCore. napi-generated registration glue needs unsafe_code = deny in the addon; zvs-core retains forbid, and no handwritten unsafe code was added. Build uses the official NapiCli API with platform/release/target/locked options instead of shelling through the CLI. Required studio tests exposed three stale migration expectations and an existing checkbox/card double-toggle bug; migration expectations now include 0005, and checkbox click propagation is stopped. Packaging remains TASK_056; the required resource-copy layout is documented.

**Next session should know:** Verified on x86_64-pc-windows-msvc: pnpm build:rust, pnpm test:rust (3 native tests), cargo fmt --check, cargo clippy --locked --all-targets --all-features -- -D warnings, cargo test --locked --all-features (12 tests), pnpm --filter studio test (254 passed, 2 existing skips), pnpm test (300 passed, 2 existing skips), pnpm typecheck, pnpm lint, pnpm build, and the new Playwright nativePing smoke test. A separate live Electron check triggered debugPanic in the main process, observed NATIVE_ERROR and the panic hook on stderr, then successfully called nativePing from the renderer. For DevTools: await window.zvs.call("system.nativePing", { text: "word ".repeat(600) }) returns { ok: true, data: { count: 3 } }. Run pnpm build:rust:debug for the debug artifact; normal builds do not export the panic function. Other platforms and installer packaging were not verified; hosted CI awaits the next push. Task files remain locally maintained under the existing ignore rule.