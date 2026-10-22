---
task: TASK_028_EXTRA_1
title: Local embedding runtime — engine downloads, GPU plan, indexing metrics
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_028, TASK_029, TASK_030]
unblocks: []
estimate: 6 h
---

# TASK_028_EXTRA_1 — Local embedding runtime — engine downloads, GPU plan, indexing metrics

## Context

TASK_028 built the indexing pipeline and TASK_029/030 built downloads, but the local half of
retrieval never closed. `LocalEmbeddingAdapter.embed` threw «Локальный движок эмбеддингов ещё не
подключён»: `FileSystemModelStore` could *see* a `.gguf` on disk and nothing could turn it into a
vector. Indexing therefore worked against Ollama and API providers only, and a user who downloaded
BGE-M3 from the curated catalogue got a file that did nothing.

A weight file is not a program. Running one needs an inference engine, and the engine has to match
the machine — a CPU build, or one of several GPU backends. That is the gap this task fills, plus
the question the user asks the moment a local index starts: is my GPU actually doing anything?

## Why

Without this, «локальные модели» is a download page and nothing more. With it, the whole retrieval
path can run with no vendor, no key and no network.

## Read first

- `/docs/PASSPORT.md` §10 (Rust boundary), §12 (AI layer)
- `TASK_028` Handoff — the indexing pipeline this hangs off
- `TASK_029` Handoff — the download queue the engine rides

## Scope

**In scope**

- A `runtime` download kind and a local-engine catalogue resolved from llama.cpp releases
- Accelerator detection and a recommended build per machine
- Archive unpacking, executable discovery, engine lifecycle
- `LocalEmbeddingAdapter` wired to a real engine
- CPU / RAM / GPU / VRAM sampling during indexing, with meters on the Vector Stores page
- A local-stack panel on the Downloads page that installs the engine and its model together

**Out of scope**

- ONNX execution. `ModelFormat` and `RuntimeEngine` carry `onnx`/`onnxruntime` so the registry has
  a place for it, and an `.onnx` model fails with its own message — but no ONNX engine ships.
- Local text generation. The engine is started `--embeddings`; chat still goes through a provider.
- Reranking through the local engine, though BGE Reranker is already in the curated catalogue.

## ❓ DECISION (answered 2026-09-15)

**Q: bundle an engine, or download one?** Answered: **download**. llama.cpp publishes a build
several times a day across a dozen backends; bundling one pins the app to a stale binary and adds
hundreds of megabytes to the installer for a feature most API users never touch.

**Q: where do the metrics go?** Answered: the **Vector Stores → Documents panel**, beside the
indexing progress bar — not a global status bar.

## Steps

1. `runtime` joins `DownloadItemKind`, `runtimes` joins `DiskCategory`, `github` joins
   `CatalogueSource`; new `packages/shared/src/runtimes/` DTOs and a `ResourceSampleDto`.
2. `RUNTIME_DEFINITIONS` describes each build by the *shape* of its release asset name, so a new
   upstream build needs no code change. `planAccelerator` turns a `DeviceProfileDto` into a
   recommendation with a reason the user can read.
3. `ReleaseResolver` resolves those patterns against the live release list, on demand only.
4. `RuntimeService` queues the archives, unpacks them, finds `llama-server`, and owns the process.
5. `LlamaServer` runs the child on loopback behind a random key and speaks `/v1/embeddings`.
6. `ResourceMonitor` samples CPU, RAM and — where `nvidia-smi` exists — GPU and VRAM.
7. `IndexingService` starts sampling with the run and stops with it; the `vector.index` node emits
   each sample as a `step` on the run's own stream.
8. Downloads page: a local-stack panel. Vector Stores: four meters under the progress bar.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm lint:tailwind && pnpm build
```

Manual check: open Загрузки, press «Установить набор», watch the engine and BGE-M3 land in the
queue. Create a vector store against a `local` provider and index a folder; the meters should move
and GPU should be non-zero on a Vulkan build.

## Definition of done

- [x] A local `.gguf` can be embedded end to end
- [x] The engine is resolved, downloaded, unpacked and started without leaving the app
- [x] The recommended build matches the detected hardware and says why
- [x] CPU/RAM/GPU/VRAM meters appear while an index runs and vanish with it
- [x] A missing counter reads «н/д», never zero
- [x] Tests and gates pass

## Handoff

**Built:**

*Shared* — `runtime` added to `DownloadItemKind`, `runtimes` to `DiskCategory`, `github` to
`CatalogueSource`; `packages/shared/src/runtimes/RuntimeDto.ts` (`ModelFormat`, `Accelerator`,
`RuntimeEngine`, `RuntimeState`, `RuntimeDto`, `RuntimePlanDto`, `RuntimeOverviewDto`);
`ResourceSampleDto` in `system.ts`. Four channels: `runtimes.list`, `runtimes.install`,
`runtimes.stop`, `system.resources`.

*Host* — `platform/archive.ts` unpacks zip (stored + deflate, with a ZIP64 refusal) and tar.gz
(streaming, GNU long names, links dropped), every member path resolved against the target and
refused if it escapes. `platform/metrics.ts` is `ResourceMonitor`: CPU as a delta between
`os.cpus()` readings, RAM from `os`, GPU from `nvidia-smi` probed once and disabled if absent.
`runtimes/definitions.ts` holds the four builds and `planAccelerator`; `runtimes/releases.ts`
resolves them against llama.cpp's release list; `runtimes/LlamaServer.ts` owns one child process;
`runtimes/RuntimeService.ts` ties them together and implements the engine port.
`LocalEmbeddingAdapter` now takes a `LocalEmbeddingEngine` and delegates to it.
`DownloadService` gained an `installer` hook, called after a transfer verifies and *before* the row
is marked succeeded. `IndexingService` gained `resources`/`onSample`; the `vector.index` node emits
samples as `step` events with `domain: "resources"`.

*Renderer* — `LocalStackPanel.tsx` on the Downloads page: detected hardware, the plan's reason, the
engine rows, the paired embedding model, and «Установить набор» which queues both.
`ResourceMeters.tsx` on the Vector Stores Documents tab: four bars, amber past 80 %, red past 95 %,
«н/д» where the counter cannot be read.

*Tests* — `archive.test.ts` (6), `metrics.test.ts` (7), `runtimes.test.ts` (16), plus two sampling
cases in `indexing.test.ts` and one engine case in `localAdapter.test.ts`.

**Deviated from plan:**

- **Vulkan is the GPU default, not CUDA.** 32 MB against 545 MB, no toolkit to install, and it
  accelerates NVIDIA, AMD and Intel alike. CUDA is offered as an explicit second choice for NVIDIA
  owners, and is refused outright unless its `cudart-*` companion archive is in the same release —
  a CUDA build without those libraries starts and then fails to find a device.
- **Unpacking runs on the host, not in the sidecar.** §10 files "model file verification /
  conversion" under the sidecar, but doing it there meant adding `zip`, `tar` and `flate2` to the
  Rust workspace for work that is one `zlib` call away in Node. The long part — the transfer — is
  already sidecar work. `platform/archive.ts` is ~250 lines and unit-tested; moving it later means
  moving one file.
- **The engine catalogue is resolved live, not shipped.** TASK_029 decided the catalogue ships with
  the app and never calls out on page open, and that still holds: `runtimes.list` is local, and
  only pressing Install reaches GitHub. Asset *names* carry the build tag (`llama-b10970-bin-...`),
  so a pinned URL would rot within a week; the definitions pin the naming pattern instead.
- **Metrics ride the run's stream** rather than getting their own subscription. Whoever is watching
  the index is exactly who wants to see what it costs, and `step` events already multiplex (D4).
- **`--api-key` on loopback.** The server binds `127.0.0.1` only, but a random per-start key means
  another local process cannot use our model as a free inference endpoint.

**Next session should know:**

- `RuntimeService.engineFor` prefers an accelerated build over a CPU one when both are installed.
  There is no user override yet; adding one is a settings key and a branch.
- The server stops itself after 5 minutes idle (`LlamaServer.idleMs`) and on `dispose()`. A model
  change stops the old process and starts a new one — one server per runtime, per model.
- `RuntimeService.unpackDownload` handles a multi-archive build by accumulating: the executable may
  arrive in either archive, so "installed with no executable yet" is a legitimate intermediate
  state, not an error.
- **GPU counters are NVIDIA-only.** `nvidia-smi` is the only vendor tool we can call without
  shipping a driver library. AMD and Intel report `null`, which the meters render as «н/д». If this
  matters, the next step is `Get-Counter "\GPU Engine(*)\Utilization Percentage"` on Windows, which
  costs ~500 ms a sample and would need its own cadence.
- **Autofill does not yet recognise a local weight file.** `autofill.sameModel` compares a provider
  model id against the catalogue name before the `:` tag, which matches Ollama's `bge-m3:latest`
  but not the local store's `bge-m3-FP16.gguf`. A local store therefore needs its embedding model
  chosen by hand in the form. Out of scope here; noted rather than fixed.
- Gates run on Windows: `pnpm vitest run` (523 passed, 2 skipped), `tsc -b`, `tsc -p
  tsconfig.test.json`, `eslint`, `lint:tailwind`, `electron-vite build`. `pnpm format:check` still
  reports the drizzle-generated `meta/*.json`, as it did before.
