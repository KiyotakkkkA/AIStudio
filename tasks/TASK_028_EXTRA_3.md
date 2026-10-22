---
task: TASK_028_EXTRA_3
title: Smaller OCR model, removable engines, engines in the catalogue
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_028_EXTRA_1, TASK_028_EXTRA_2]
unblocks: []
estimate: 3 h
---

# TASK_028_EXTRA_3 — Smaller OCR model, removable engines, engines in the catalogue

## Context

Three things the previous two tasks got wrong, found by using them.

The OCR pair they shipped — Qwen2.5-VL **7B** plus its projector — is 5.5 GB of weights. Next to a
1.2 GB embedding model, that does not fit an 8 GB card, and on a 16 GB machine it pushes the whole
index into swap. The model was picked for quality without anyone checking it against the machine
the app is meant to run on.

An engine could be installed and never removed. Someone who takes the Vulkan build and later wants
CUDA ends up with both on disk and no way to say so.

And the engines lived in a pinned panel above the catalogue — a second list with its own rules,
its own install button and its own empty state, sitting above a list that already knew how to show
an artefact's size, state and actions. It was also invisible to the catalogue filters.

## Why

The app is supposed to run on an ordinary machine, and a second kind of list is a second thing to
maintain and to learn.

## Scope

**In scope**

- Qwen2.5-VL 3B and its projector in the curated catalogue, as the default OCR pair
- `runtimes.remove`: stop, delete the unpacked build, forget its download rows
- Engine builds as ordinary catalogue rows, inside the existing filters
- A pass over the comments added by the last two tasks

**Out of scope**

- Choosing the OCR model automatically from `DeviceProfileDto.tier`. The descriptions now state
  what each pair needs; the picker is still the user's.

## Steps

1. Curated catalogue: the 3B pair first, the 7B pair kept and re-described as the larger option.
2. `CatalogueItem.state` / `blockedReason`, so a source that knows its own install state can say
   so; `RuntimeService.catalogueProvider()` uses it.
3. `CatalogueService.addProvider`, and `offer()` demoted to lookups only.
4. `RuntimeService.uninstall` and `DownloadService.forgetByRef`.
5. Renderer: delete `LocalStackPanel`, route runtime rows through `CatalogueRow`, move the
   hardware verdict into the detail rail.
6. Comment pass.

## Verify

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo test
pnpm vitest run
pnpm typecheck && pnpm lint && pnpm lint:tailwind && pnpm build
```

Manual check: Downloads → «Локальные движки» filter shows the builds for this machine. Install
Vulkan, then remove it, then install CUDA; the disk breakdown should follow along.

## Definition of done

- [x] The default OCR pair fits 8 GB of VRAM beside the embedding model
- [x] An installed engine can be removed, and its rows do not survive it
- [x] Engines are catalogue rows and are counted by the catalogue filters
- [x] No second list on the Downloads page
- [x] Tests and gates pass

## Handoff

**Built:**

*Catalogue* — `Qwen2.5-VL-3B-Instruct-Q4_K_M` (1.93 GB) and `mmproj-…-3B-…-Q8_0` (845 MB), both
sha256-verified against the Hugging Face API. 2.8 GB for the pair, so it sits on an 8 GB card
beside BGE-M3 with room to work. The 7B pair stays for anyone with 12 GB or more; both
descriptions now name the VRAM they want, and the tags carry it too.

*Engines in the catalogue* — `CatalogueItem` gained an optional `state` and `blockedReason` for a
source that knows better than the download table does, which an engine does: it is a group of
archives unpacked into a directory, so "installed" is a question about the directory, not about
whether some row finished. `RuntimeService.catalogueProvider()` returns one row per build that
exists for this platform — never the archives — and reports installed builds through the
provider's `installed()`. `CatalogueService.addProvider` joins it after construction, because the
engine provider needs the download queue that the catalogue is built into. `offer()` no longer
feeds `list()`: the resolved archives stay findable for `downloads.start` but are not browsable,
so the catalogue shows «llama.cpp · Vulkan», and the archives show up where they belong — as
progress rows in Выполняется.

*Removal* — `runtimes.remove` → `RuntimeService.uninstall`: stops every server of that build,
deletes the unpacked directory, drops the settings entry and calls the new
`DownloadService.forgetByRef` so the finished archive rows do not keep it looking half-present.
The confirm dialog on a runtime row says what removal actually costs.

*Renderer* — `LocalStackPanel` is gone. `CatalogueRow` handles both kinds: `DownloadStore.install`
and `.uninstall` branch on whether the ref names a build. The hardware verdict — the plan's reason
and the detected GPU — moved into the detail rail, shown when a runtime row is selected. The
filter rail needed no change: it iterates `DownloadItemKind.options`, so «Локальные движки» and
its count appeared on their own.

*Comments* — removed about 60 lines of comments that restated their code across the files added by
the last two tasks, and cut `RuntimeDto.ts` from 31 % comments to 10 %. What is left records a
decision or a constraint that is not visible from the code: why Vulkan is the GPU default, why the
local zip header is read instead of the central directory's copy, why a missing GPU counter is
`null` and never zero, why embedding and vision servers cannot share a process.

**Deviated from plan:**

- **The 7B model was kept, not replaced.** It is genuinely better on poor scans, and a 12 GB card
  is not rare. Making the 3B the first row and saying plainly what each needs is better than
  deciding for everyone.
- **`runtimes.list` survives** even though the panel that drove it is gone: the detail rail still
  wants the hardware verdict, and it is the only call that knows it.

**Next session should know:**

- `DownloadStore` lost `installLocalStack`, `recommendedRuntime`, `recommendedModels` and
  `localStackReady` — they existed only for the panel. `RuntimeOverviewDto.recommendedModels` is
  still populated by the host and is now unread by the renderer; it is the obvious hook if a
  "install the whole set" action comes back, this time as a catalogue action.
- A runtime catalogue row's ref is `runtime:<buildId>`; an archive's is
  `runtime:<buildId>:<tag>:<asset>`. `runtimeIdOfCatalogueRef` matches only the former, which is
  what keeps the two apart on both sides of the IPC boundary.
- Removing a build does **not** touch vector stores that were indexed with it. Their vectors stay
  valid; only new embedding calls fail, with the message that points at Downloads.
