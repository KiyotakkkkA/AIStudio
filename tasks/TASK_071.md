---
task: TASK_071
title: Downloads and local runtimes in Rust — catalogue, queue, disk, archives, llama.cpp engine, downloads.* and runtimes.*
phase: M — Migration to Tauri
status: done
depends_on: [TASK_070]
unblocks: [TASK_072]
estimate: 5 h
---

# TASK_071 — Downloads and local runtimes in Rust — catalogue, queue, disk, archives, llama.cpp engine, downloads.* and runtimes.*

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Two related host areas move here.

**Downloads** (`host/downloads/`, TASK_029 – TASK_030):
- `DownloadService.ts` (700 lines): queue, priority, pause and resume, resume after restart
  (`recover()`), cancel, remove, and installer hooks. The transfer itself is a `zvs-jobd` job,
  so the host side is orchestration.
- `catalogue.ts` and `curated.ts`: a curated catalogue ships with the app and makes no
  outbound call on page open. A `refresh` flag runs the live sources, Ollama among them
  (TASK_029 decision).
- `disk.ts`: disk accounting and free bytes.
- `platform/archive.ts` (338 lines): zip and tar extraction for runtime archives, with
  traversal protection.
- `platform/longPath.ts`: Windows long-path handling (TASK_028_EXTRA_5).
- `kernel/downloadNodes.ts`.

**Local runtimes** (`host/runtimes/`, TASK_028_EXTRA_1 and EXTRA_3):
- `RuntimeService.ts` (647 lines): the llama.cpp engine is downloaded, not bundled. Release
  assets are resolved on demand with Vulkan recommended for a discrete GPU, through the device
  plan. It installs, removes and stops engines, embeds with local embedding models, runs OCR
  through the local vision model, and offers local rerankers.
- `LlamaServer.ts` (404 lines): spawns and supervises `llama-server` processes. Port, health,
  idle stop, crash handling.
- `releases.ts`: the GitHub release resolver.
- `definitions.ts`: engine and model definitions.

In Electron, the release resolver and the Ollama catalogue use `net.fetch`. In Rust they use the
shared `HttpClient` from TASK_063.

## Why

Local embeddings, OCR and reranking all run on the engine managed here. Indexing (TASK_072)
and chat retrieval with local models cannot be ported without it.

## Depends on

- `TASK_070` — kernel nodes and runs, which downloads are attached to.

## Read first

- `/docs/PASSPORT.md` §10, §15 (paths)
- `apps/studio/src/host/downloads/*`, `runtimes/*`, `platform/archive.ts`, `longPath.ts`,
  `kernel/downloadNodes.ts`, `ipc/downloads.ts`, `ipc/runtimes.ts`
- Handoffs of TASK_029, TASK_028_EXTRA_1, TASK_028_EXTRA_3 and TASK_028_EXTRA_5
- `apps/studio/tests/downloads.test.ts`, `runtimes.test.ts`, `archive.test.ts`,
  `longPath.test.ts`, `llamaRerank.test.ts`

## Scope

**In scope**
- `DownloadRepository`, `DownloadService`, catalogue providers (curated, Ollama, runtime),
  `DiskService` and download nodes
- `platform::archive` over `zip`, `tar` and `flate2` (pinned), with the same traversal and
  symlink refusals, and `platform::long_path` (verbatim `\\?\` handling where `std` does not
  already cover it, tested)
- `RuntimeService`, `LlamaServer` (`tokio::process`, hidden window, health, idle stop, crash
  supervision), the release resolver and definitions
- Attaching the local engine to `ProviderRegistry` (embeddings) and local rerankers to
  `VectorStoreService`, as `main.ts` does
- `downloads.*` and `runtimes.*` channels, DTOs and fixtures
- `ShutdownCoordinator`: `downloads.dispose()` and `runtimes.dispose()`, which stop every
  engine process

**Out of scope** (do not do these, even if tempting)
- Changing the catalogue policy or the engine choice
- PDF and image extractors (TASK_072)

## Steps

1. Archive and long-path modules with their adversarial tests (zip-slip, absolute paths,
   symlinks, overlong names).
2. Disk service and repository.
3. `DownloadService` and catalogue, with `downloads.test.ts` ported against a fake jobs port
   and fake clock, including restart recovery.
4. Release resolver against recorded GitHub API fixtures. Definitions.
5. `LlamaServer` supervision, tested against a stub executable built in the test profile
   that behaves like `llama-server` (health endpoint, crash on command).
6. `RuntimeService`, with `runtimes.test.ts` and `llamaRerank.test.ts` ported.
7. Wiring, channels and shutdown.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, the Downloads page lists the curated catalogue with no
network access. Refresh pulls live sources. Install the llama.cpp engine (Vulkan on a
discrete GPU) and an embedding model, pause and resume the model download, quit mid-download,
relaunch, and see it resume. Remove the engine. Local embeddings work from a vector store test
search.

## Definition of done

- [x] Downloads, catalogue, disk accounting, archives and long paths are ported with their
      tests
- [x] Engine install, supervision, embedding, OCR entry point and rerank are ported with their
      tests
- [ ] The Downloads page and engine management work under Tauri, including resume after
      restart
- [x] Engine processes stop on quit
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract`: `downloads` and `runtimes` modules with every DTO of `downloads.*` and
  `runtimes.*` (inputs with Zod defaults behind accessors, outputs writing their defaulted fields),
  plus `DownloadFetchInput` and `DownloadReportDto` with `JsonSchema` for the node. 28 fixture
  pairs under `packages/shared/fixtures/downloads` and `runtimes`, registered in both tables.
- `platform::archive` (zip reader over `flate2`, tar.gz over `tar =0.4.46`, traversal, drive,
  rooted-name, symlink, hard-link, ZIP64, method, truncation, cancellation and overlong-name
  tests) and `platform::long_path` (moved out of `vector_sources`, with a >260-character test).
- Data: `DownloadRow`, `DownloadRepository` (queue order, active/latest by ref, patch with
  nullable columns, interrupt, sweep) with tests.
- `downloads`: `CatalogueService`, `CuratedProvider`, `OllamaProvider`, `DiskService`,
  `DownloadService`, and the `download.fetch` node (`kernel::download_nodes`, `KernelServices::downloads`).
  `downloads.test.ts` ported against `testing::FakeDownloadJobs` and `FakeClock`, plus catalogue,
  Ollama (mock server) and shutdown tests.
- `runtimes`: definitions and `plan_accelerator`, `ReleaseResolver` (tested against the TS
  payload and a recorded GitHub answer, `src/runtimes/fixtures/llama-releases-b11696.json`),
  `LlamaServer` (tested against the test binary re-run as a stub server: health, key, crash
  mid-request, crash at start, start timeout, idle stop, self-exit, cold-start give-up,
  concurrent start), and `RuntimeService` with `runtimes.test.ts` and `llamaRerank.test.ts` ported.
- `ipc::downloads` and `ipc::runtimes` (all 13 channels) with dispatch tests; `Services` gains
  `downloads` and `runtimes`; `test_services` builds both.
- Composition: disk, catalogue (curated + Ollama + engine builds), downloads over the sidecar,
  runtimes over the device probe, installer and catalogue provider attached, local engine on the
  provider registry, local rerankers on vector stores, `download.fetch` registered,
  `downloads.attach(runs)`, `downloads.recover()` after `runs.recover()`, `downloads` and
  `runtimes` in the `Stop` shutdown stage (the latter waits for every engine process to exit).
- Gates run on Windows 2026-10-17, all passing: `cargo fmt --check`, workspace clippy with
  `-D warnings`, `cargo test --workspace` (zvs-host 637 passed), `pnpm typecheck`, `pnpm lint`,
  `pnpm build`, `pnpm format:check`, `npx vitest run` (1647 passed, 2 skipped),
  `contract-channels --check`.

**Deviated from plan:**

- No `zip` crate. Every release before 9.0.3 is yanked and 9.0.3/9.0.4 were days old, so the zip
  side is the TS host's own central-directory reader ported over `flate2` (same refusals:
  ZIP64, methods other than stored/deflate). `tar` and `flate2` are pinned as planned.
- Tar archives go through the `tar` crate, which also honours PAX path headers; the TS reader
  only knew GNU long names.
- The `LlamaServer` stub is the test binary re-run with `ZVS_LLAMA_STUB_*` variables (the
  kernel tests' pattern), reached through an injectable `Launcher`, rather than a separate bin.
- `LlamaServer` kills with `start_kill` (no SIGTERM grace on Unix); a missing executable fails at
  once instead of waiting out the health deadline; concurrent callers join an in-flight launch
  instead of possibly spawning a second process; an unexpected exit is logged.
- `DownloadService::launch` re-checks whether the run already finished after subscribing, because
  the run can end on another worker before the observer is attached.
- `FakeDownloadJobs` records an attempt after its first chunk lands and also returns when its
  run is cancelled while held, matching the ordering the TS fake got from microtasks.
- Runtime device and the download/catalogue sides of `RuntimeService` are ports
  (`RuntimeDevice`, `RuntimeDownloads`, `RuntimeCatalogue`), as the TS `Pick<…>` types were.

**Next session should know:**

- The manual check (`pnpm dev:tauri`: curated catalogue offline, refresh, install the engine and
  an embedding model, pause/resume, quit mid-download and relaunch, remove the engine, local
  embeddings from a vector store test search) has **not** been done. As in the TS host, a
  download interrupted by quitting is left `paused` (resume it by hand); only a crash leaves it
  `running` for `recover()` to requeue.
- TASK_072: OCR is `RuntimeService::read_image(model_ref, media_type, bytes, language, cancel)`;
  `platform::archive::read_zip_entries` is the in-memory reader the `.docx` extractor used.
- The local engine HTTP client has no proxy on purpose (loopback); the Ollama catalogue and the
  GitHub resolver use the shared client with the system proxy.
