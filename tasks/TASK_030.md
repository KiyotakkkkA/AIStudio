---
task: TASK_030
title: Downloads page
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_029]
unblocks: []
estimate: 3 h
---

# TASK_030 — Downloads page

## Context

The download service works; this task gives it the screen from `/design/Downloads.dc.html` and closes Phase 5.

Layout: a 216 px left rail with catalogue filters (Everything, Models, Embedding models, MCP servers, Skill bundles), status filters, and the disk breakdown card at the bottom; a centre column with **In progress** at the top (rows carrying a progress bar, bytes done over total, rate and ETA) then the queue, then the catalogue with installed / update / available states; and a 314 px detail rail for the selected item showing its metadata, download stats, destination path, checksum and post-download options.

## Why

Pulling a model is one of the first things a user does; without this page it is invisible and uncontrollable.

## Depends on

- `TASK_029` — Downloads — catalogue, queue, resume, disk accounting.

## Read first

- `/design/Downloads.dc.html` — the whole page
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/downloads/` store and organisms
- In-progress rows with live progress, rate and ETA
- Catalogue list with per-state actions
- Filters and the disk breakdown
- Detail rail including post-download toggles (verify checksum, set as default model)

**Out of scope** (do not do these, even if tempting)
- Catalogue sourcing changes — TASK_029's decision settles that
- Bandwidth limiting (note it for TASK_052 if wanted)

## Steps

1. `DownloadStore` subscribing to download run events; no polling. Smooth the displayed rate over a few seconds so the number is readable.
2. In-progress rows exactly as drawn: icon tile, name, kind chips, bytes line, rate and ETA, progress bar, percentage, pause and cancel.
3. Queue rows with priority controls and cancel; catalogue rows with Download / Update / Remove per state, and a disabled state with the reason when a precondition fails.
4. Left rail filters with live counts; disk breakdown as the stacked bar with its legend.
5. Detail rail: description, capability chips, download stats, destination path, checksum, and the post-download toggles.
6. States: nothing downloading, catalogue empty or unreachable, insufficient disk, checksum failure with a retry action.
7. Tests: rate smoothing is a pure function and covered; store reconciles progress events including gaps.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Queue two downloads, confirm the concurrency cap holds and the second waits. Pause one, restart the app, confirm state is preserved and resume works from the page.

## Definition of done

- [x] Page matches the mockup
- [x] Live progress, rate and ETA from real events
- [x] All catalogue states and their actions work
- [x] Disk breakdown accurate
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
`features/downloads/` — `DownloadStore` (event-driven, no polling), `rateSmoothing.ts` (pure EMA
over the real gap between progress events, plus `estimateEta`), `downloadPresentation.ts`
(Russian labels, byte/rate/ETA formatting), and the organisms: `DownloadFilterRail`,
`DiskBreakdown`, `DownloadProgressRow`, `DownloadQueueRow`, `DownloadFailureRow`,
`CatalogueRow`, `DownloadDetailRail`, `DownloadsWorkspace`. `DownloadsPage` is now composition
only. The store maps `streamId → downloadId` from `runs.list({kinds:["download"], live:true})`
and applies progress events straight to the row; an event on an unmapped stream, a sequence gap
or a run `end` schedules a single reconciling `downloads.list`. Post-download toggles persist
under the `downloads.options` setting. The whole interface is Russian.

**Deviated from plan:**
- The curated catalogue was cut to three entries at the user's request: Qwen2.5-VL 7B Instruct
  (local OCR), BGE-M3 (embeddings) and BGE Reranker v2 M3. Sizes and sha256 checksums are the
  real published values from the Hugging Face artefacts.
- Added `downloads.prioritise` (contract, handler, `DownloadService.prioritise`) — step 3 asks
  for queue priority controls and no channel existed for them. Only a `queued` row can move.
- `TaskFilterButton` moved to `ui/molecules/FilterRailButton` since both rails need it
  (PASSPORT §13: a feature component useful to another domain moves down into `ui/`).
- "Open folder" from the mockup is not built: no IPC channel opens a path, and adding one is
  outside this task. The header carries "Обновить каталог" instead.

**Next session should know:**
- Qwen2.5-VL needs its `mmproj` projector alongside the weights, and a catalogue entry is one
  URL — the entry pulls the weights only. Wiring multi-file artefacts is a catalogue-shape
  change, deliberately left to whoever needs it.
- The "Сделать моделью по умолчанию" toggle records the intent in settings; nothing consumes it
  yet — the provider wiring belongs with whoever lands default-model selection.
- Bandwidth limiting is still unbuilt; note it for TASK_052.

---

**Phase 5 is complete when this task is done.** Long-running work is real: the sidecar runs it, the Tasks page shows it, and it survives restarts.
