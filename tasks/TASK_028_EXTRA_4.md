---
task: TASK_028_EXTRA_4
title: Indexing ETA and documents that appear as they land
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_028_EXTRA_1]
unblocks: []
estimate: 1.5 h
---

# TASK_028_EXTRA_4 — Indexing ETA and documents that appear as they land

## Context

A real corpus — 359 052 chunks, a folder of court papers — showed two gaps in the indexing panel
that a small test corpus never does.

There was no estimate. «16 из 359 052 фрагментов встроено» tells you the run has barely started and
nothing about whether that means ten minutes or ten hours, which is the only question worth asking
at that point.

And the Documents tab stayed on «Документы не проиндексированы» for the whole run, because
`refreshAfterIndex` only loads the list when the run **ends**. On a corpus this size that is the
wrong answer for hours: files were finishing the whole time, and none of them showed.

## Why

A progress bar with no estimate is a spinner with extra steps, and a list that stays empty while it
fills looks like a broken run.

## Scope

**In scope**

- Smoothed throughput and an ETA on the indexing panel
- Each document emitted as its vectors land, and shown immediately
- The empty state saying the right thing while a run is in flight

**Out of scope**

- A better GPU utilisation reading. See the note below — the meter under-reads on bursty work, but
  fixing that means averaging inside the sampler and is its own change.

## Steps

1. Move `rateSmoothing.ts` to `features/progressRate.ts` and make it unit-neutral, so the same
   smoothing serves bytes for a download and chunks for an index.
2. `IndexContext.onDocument`, emitted from `ingest` right after `recordIndexed`, forwarded by the
   `vector.index` node as a `step` with `domain: "document"`.
3. `VectorStoreStore`: keep a `RateState` for the run, expose `indexChunksPerSecond` and
   `indexEtaMs`, and merge each document into `documents` as it arrives.
4. Panel: a line under the bar with the estimate, the rate and the running document count.

## Verify

```bash
pnpm vitest run
pnpm typecheck && pnpm lint && pnpm lint:tailwind && pnpm build
```

Manual check: index a folder of a few hundred files. Documents should appear one by one, the count
should climb, and the estimate should settle within the first minute.

## Definition of done

- [x] The panel shows time remaining and chunks per second
- [x] Documents appear during the run, newest first, never duplicated
- [x] A re-indexed file moves in the list rather than being listed twice
- [x] Tests and gates pass

## Handoff

**Built:**

*Rate helper* — `features/downloads/rateSmoothing.ts` became `features/progressRate.ts`, with
`bytes`/`bytesPerSecond` renamed to `value`/`perSecond`. It was already unit-neutral arithmetic
sitting under a name that claimed otherwise; downloads and indexing now share it.

*Host* — `IndexContext.onDocument` fires from inside `withExclusiveStore`, right after
`recordIndexed`, which already returned the written row. The `vector.index` node forwards it as a
`step` event with `domain: "document"`, alongside the `resources` steps from TASK_028_EXTRA_1 — so
this rides the run's own stream and needs no second subscription.

*Renderer* — `VectorStoreStore` keeps a `RateState` fed by each progress event and derives
`indexChunksPerSecond` and `indexEtaMs` from it. `receiveDocument` validates the loose step payload
against `VectorDocumentDto` and merges it newest-first, replacing any row with the same id.
Starting a run clears `documents` first, so a re-index rebuilds the list from what this run reports
rather than leaving rows for files that have since been deleted. `formatDuration` and
`formatThroughput` in `vectorPresentation.ts`; the empty state now distinguishes "no run" from
"running, nothing finished yet".

**Deviated from plan:**

- **The ETA is computed in the renderer, not the host.** The host already sends `done`/`total` on
  every progress event, and the smoothing is presentation — how jumpy a number the user should be
  shown. Putting it on the host would mean a third field on the progress event and the same
  arithmetic anyway.
- **`documents` is cleared when a run starts.** The alternative — merging into whatever was already
  listed — leaves stale rows for files removed from disk, which the run then deletes from the
  database without the list noticing.

**Next session should know:**

- The ETA is only as good as `total`, and `total` is an estimate that converges as files are
  chunked (`bytes / 6 / (chunkSize - overlap)` per pending file, swapped for the truth as each file
  finishes). Expect it to be wrong early and tighten quickly. `formatDuration` coarsens as the
  number grows for exactly this reason.
- **The GPU meter under-reads on indexing.** `nvidia-smi` reports instantaneous utilisation and the
  sampler caches it for a second; embedding runs in bursts of 32 chunks with concurrency 2, so a
  sample often lands in a gap and shows 0 % while VRAM clearly holds the model. The honest fix is
  averaging several reads inside `nvidiaSmiSampler` per tick — worth doing if anyone reads the
  meter as "the GPU is idle" and goes looking for a bug that is not there.
- Documents arrive only for files that were actually indexed. Unchanged and skipped files report
  nothing, which is why the list can climb more slowly than the file counter in the message line.
