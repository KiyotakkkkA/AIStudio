---
task: TASK_028_EXTRA_5
title: Reset a vector store, and Windows long-path support
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_028, TASK_028_EXTRA_2]
unblocks: []
estimate: 1.5 h
---

# TASK_028_EXTRA_5 — Reset a vector store, and Windows long-path support

## Context

Two things, both surfaced by indexing a real folder of court papers.

There was no way to empty a store. «Удалить» destroys the store, its sources and its settings;
re-indexing after a bad configuration meant rebuilding all of it by hand.

And the paths in that corpus are long. `C:\Users\kirza\Desktop\суд. разбирательства\взыск.
ден.ср\(ИП Гусаров)\…` with descriptive Russian file names goes past Windows' 260-character limit
without trying. The walker's error handling made that invisible: every failure became «Папка
недоступна», the same note a permissions problem gets, and the run finished "successfully" having
silently skipped a subtree.

## Why

A corpus you cannot re-index from scratch is a corpus you cannot fix, and an indexer that skips
files without saying why is worse than one that fails.

## Scope

**In scope**

- `vectorStores.clear`: empty the table and the document rows, keep everything else
- A `\\?\` prefix at every filesystem call that takes a user-chosen path
- Walk notes that name the actual cause

**Out of scope**

- A `longPathAware` manifest for the packaged build. See the note below — it belongs with
  TASK_056 (packaging), and the prefix makes the app work without it either way.

## Steps

1. `VectorStoreService.clear`: drop and recreate the table, delete the document rows, reset the
   counters to `pending`.
2. `platform/longPath.ts` with `toExtendedPath` / `fromExtendedPath`.
3. Apply at the boundaries: `walk.ts` (stat, readdir, stat), `IndexingService.defaultRead`, and
   the path handed to the sidecar for `job.document.extract`.
4. `reason()` in the walker, so a note says what actually went wrong.
5. «Сброс» in the store header, next to «Удалить».

## Verify

```bash
pnpm vitest run
pnpm typecheck && pnpm lint && pnpm lint:tailwind && pnpm build
```

Manual check: index a folder, press «Сброс», confirm the stat strip zeroes and the sources
survive, then index again. Point a store at a deeply nested tree and confirm nothing is skipped.

## Definition of done

- [x] A store can be emptied without losing its sources or settings
- [x] A file past 260 characters is walked, read and indexed
- [x] A path that genuinely cannot be read says why
- [x] Tests and gates pass

## Handoff

**Built:**

*Reset* — `VectorStoreService.clear` runs under the same `exclusive` lock as everything else:
`removeVectorIndex` then `createVectorIndex` with the store's own dimension and metric, then
`vectorDocuments.removeByStore`, then the counters back to zero and the status to `pending`.
Recreating beats deleting row by row — a full corpus is millions of vectors, and a fresh table also
reclaims the disk that a delete would only mark free. A failure between the two leaves the store
marked `broken` rather than reporting a successful reset. `vectorStores.clear` is a new channel;
the renderer clears its document list and any search result alongside. The button is disabled while
an index is running and when there is nothing to clear, and its confirmation states the counts and
says plainly that sources and settings stay.

*Long paths* — `platform/longPath.ts`. `toExtendedPath` prefixes an absolute Windows path with
`\\?\` (and a UNC path with `\\?\UNC\`), which opts that one call out of the 260-character limit.
It is applied at the filesystem boundary only: the prefix disables path normalisation, so anything
carrying it must already be absolute and clean, and it must never reach a database row, a DTO or
the screen. `fromExtendedPath` strips it on the way in, so a pasted `\\?\` source path still
normalises. The sidecar gets the prefixed path too, because Rust opens the file itself and Windows
enforces the same limit on it.

*Walk notes* — `reason()` maps `ENOENT`, `EPERM`/`EACCES` and `ENAMETOOLONG` to their own wording
and falls back to the message. Previously every failure read «Папка недоступна» with no cause.

*Tests* — `longPath.test.ts` builds a real tree past 300 characters out of Russian folder names and
asserts the walker finds the file and reads it back; plus two host cases for the reset, including
that a re-index after one finds the file changed rather than unchanged.

**Deviated from plan:**

- **The prefix is applied to every absolute Windows path, not only long ones.** A threshold means
  the code works at 255 characters and breaks at 261, which is the worst kind of bug to carry. All
  the call sites already `resolve()` first, which is the precondition the prefix needs.

**Next session should know:**

- **The failure could not be reproduced on this machine, and the test does not pretend otherwise.**
  Windows 10 1607+ lifts the limit when `LongPathsEnabled` is set in the registry *and* the running
  binary declares itself long-path aware; the development `node` binary here does both, so an
  unprefixed long path works in development. That is exactly why this is easy to miss locally and
  easy to hit on a user's machine — and why the test asserts the prefix works rather than that the
  alternative fails. The packaged Electron binary is the case to watch: **TASK_056 should add
  `longPathAware` to the application manifest**, which the prefix makes a belt-and-braces measure
  rather than a dependency.
- The 260-character limit applies to the *whole path*, so a deep tree can break on a short file
  name. Nothing in the app shortens paths; the prefix is the only thing standing between a corpus
  and a silently short index.
- `clear` does not touch the extractor cache or the scratch directory for page images — those are
  per-run and cleaned up by the extractor itself.
