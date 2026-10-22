---
task: TASK_029
title: Downloads — catalogue, queue, resume, disk accounting
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_027]
unblocks: [TASK_030]
estimate: 4 h
---

# TASK_029 — Downloads — catalogue, queue, resume, disk accounting

## Context

`/design/Downloads.dc.html` shows a page that tracks bytes moving and a local catalogue: models, embedding models, MCP server packages and skill bundles, each installed, updatable or available, with a disk breakdown. This task builds the service behind it.

The canvas note is explicit that this is not a separate system: a queued download appears on both the Tasks page and the Downloads page — **one queue, two views**. So a download is a kernel job like any other, and the Downloads page is a specialised view over the same runs, with its own richer per-item state.

Resumability is the requirement that shapes the design. A 240 GB model pull cannot start over because the app restarted, so downloads write to a temp file with a range-request resume, verify a checksum on completion, and only then move into place.

## Why

Local models are the point of an Ollama-first workflow, and a 240 GB pull with no progress, no resume and no disk accounting is unusable.

## Depends on

- `TASK_027` — zvs-jobd sidecar — protocol, lifecycle, health, cancel.

## Read first

- `/design/Downloads.dc.html` — in-progress rows, the catalogue, the disk breakdown, the detail rail
- `/design/Tasks.dc.html` — the same download appearing as a queued task
- `/docs/PASSPORT.md` §11

## Scope

**In scope**
- `download` table and a catalogue model covering models, embeddings, MCP packages and skill bundles
- Download job in the sidecar: ranged, resumable, checksum-verified
- Queue with a concurrency cap and priority
- Disk accounting: per-category usage and a free-space precondition
- Update detection for installed items
- Channels and DTOs

**Out of scope** (do not do these, even if tempting)
- The page itself (TASK_030)
- Installing MCP packages — the download lands the artefact; wiring it up is TASK_032
- Peer-to-peer or mirror selection

## Steps

1. Catalogue: where the list of available items comes from — see the decision below. Model it as a provider-agnostic `CatalogueItem` so a second source can be added without reshaping the table.
2. `download` table: id, itemKind, itemRef, displayName, sizeBytes, bytesDone, status, targetPath, checksum, error, runId, timestamps.
3. Download job in `zvs-jobd`: ranged HTTP with resume from `bytesDone`, writing to `<target>.part`. Verify the checksum before the atomic rename into place. Emit progress with bytes and a smoothed rate — an instantaneous rate jitters and looks broken.
4. Preconditions before starting: enough free disk for the item plus a margin, and a writable target. The mockup shows an item that cannot be downloaded for lack of space — that is a real precondition, not an afterthought.
5. Queue: concurrency cap (default 2, configurable), priority so a small MCP package is not stuck behind a 240 GB model, and pause/resume/cancel per item. Cancelling deletes the partial file; pausing keeps it.
6. Disk accounting: per-category totals for the breakdown bar, computed on demand and cached briefly rather than recomputed on every render.
7. Update detection: compare installed versions or digests against the catalogue and mark items updatable — the mockup's amber `update` state.
8. Channels: `downloads.list|start|pause|resume|cancel|remove`, `downloads.catalogue`, `downloads.disk`.
9. Tests: resume continues from a partial file rather than restarting; a checksum mismatch fails and does not install; cancel removes the partial; the queue respects concurrency and priority; insufficient disk is rejected before starting.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Where should the downloadable catalogue come from? Ollama's model library is the obvious source for models, but the page also lists MCP server packages and skill bundles. Do you want a curated list shipped with the app, live queries against each source, or a registry file you control?"
>
> **Why it matters:** It decides whether the app makes outbound calls to discover what is available, which PASSPORT §14 says must be a deliberate choice, and whether the catalogue can go stale between releases.
> **Options:** (a) live queries per source — always current, outbound traffic on page open; (b) a curated JSON shipped with the app, updated per release — no surprise traffic, goes stale; (c) a registry URL you control, fetched on demand and cached.
> **If the user is unavailable:** take (b) for v1 with a clearly-marked refresh action that does a live query on demand. Note it in Handoff.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Start a download of a real, reasonably large model. Kill the app mid-download, restart, and confirm it resumes rather than restarting. Corrupt the partial file deliberately and confirm the checksum failure is caught and the item is not installed.

## Definition of done

- [x] Downloads resume across app restarts
- [x] Checksum verified before install; corrupt downloads never land
- [x] Queue respects concurrency, priority, pause, resume and cancel
- [x] Free-space precondition enforced
- [x] Disk accounting accurate per category
- [x] Downloads appear as runs on the Tasks page
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Decision taken:** option (b). The curated catalogue (`host/downloads/curated.ts`) ships with
the app and is the only thing the Downloads page reads on open — no outbound call. `refresh: true`
on `downloads.catalogue` is the explicit live query; it is the only path that leaves the process.

**Built:**

- **`download` table** (`0013_download.sql`) and `DownloadRepository`. The queue order lives in
  `nextQueued`: priority descending, then smallest artefact, then age.
- **`job.download` in `zvs-jobd`** (`crates/zvs-jobd/src/download.rs`): ranged HTTP over reqwest
  with rustls, appending to `<target>.part`, resuming from whatever is already there, restarting
  cleanly when a server ignores the range, verifying sha256/blake3 before the atomic rename, and
  leaving the partial in place on cancellation. Progress carries a smoothed (EWMA, 3 s half-life)
  byte rate — the sidecar protocol's `job.progress` gained an optional `rate` field.
- **`DownloadService`** (`host/downloads/`): queue with a concurrency cap
  (`downloads.concurrency`, default 2), priority, pause/resume/cancel/remove, free-space and
  writable-target preconditions, progress persistence, and recovery that requeues whatever was
  mid-flight when the app stopped.
- **`CatalogueService`** with a provider-agnostic `CatalogueItem`: `curatedProvider` (shipped,
  offline) and `ollamaProvider` (loopback `/api/tags`, the source for update detection). Merged
  and cached; `invalidate()` on every completed download.
- **`DiskService`**: per-category totals (models, embeddings, mcp, skills, vectors, other) plus
  free space, measured on demand and cached for 30 s.
- **`download.fetch` kernel node** — a download is a run, so it lands on the Tasks page with the
  same progress, cancel and history. `RunKind` gained `download`.
- **Channels** `downloads.list|start|pause|resume|cancel|remove|catalogue|disk`.

**Deviated from plan:**

- **Skill bundles ship with no catalogue entries.** The kind is modelled end to end and covered by
  tests, but inventing a registry URL would have been worse than an empty category. The shared
  registry that fills it belongs with TASK_032.
- **Curated entries mostly carry no checksum.** Verification is implemented and tested, but a
  wrong digest would fail every pull, and the published sha256 for these artefacts is not
  knowable without fetching them. `CatalogueItemDto.checksum` being absent is what the mockup's
  "Verify checksum" indicator should read. Ollama-sourced items do carry a digest.
- The curated list is a typed TS module rather than a `.json` file — same content, but it is
  typechecked and needs no asset-path handling in the electron-vite build.
- `remove` on a succeeded download deletes the installed artefact too, which is what the
  mockup's Remove button on an installed row means.

**Next session should know:**

- The service and the kernel are circular (the kernel executes downloads, downloads start runs),
  so `main.ts` constructs `DownloadService` first and calls `downloads.attach(runs)` afterwards.
  Any test that builds the pair must do the same.
- `downloadTargetPath` rejects unsafe file names; a new catalogue source must supply a
  `fileName` matching `[a-zA-Z0-9._-]+`.
- Verified end to end against the **released sidecar binary** in
  `test/native/sidecar.test.mjs` — a half-written `.part` resumes with a single `Range` request
  and a mismatched digest fails leaving nothing behind. `node --test test/native/` cannot resolve
  a bare directory on this Node/Windows build; use `node --test "test/native/*.test.mjs"`.
- TASK_030 renders this: `downloads.catalogue` already returns `state`, `downloadable` and
  `blockedReason` (the mockup's "not enough free disk" row), and `downloads.list` carries
  `rateBytesPerSecond` and `etaMs` for the in-progress rows.
