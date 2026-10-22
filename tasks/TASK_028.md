---
task: TASK_028
title: Indexing job — chunk, embed, upsert, with progress
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_027]
unblocks: []
estimate: 4 h
---

# TASK_028 — Indexing job — chunk, embed, upsert, with progress

## Context

Vector stores can be created and searched, but nothing fills them. This task builds ingestion: point a store at files or a folder, and the app chunks, embeds and upserts them with real progress and real cancellation.

The pipeline crosses both Rust integration paths, which makes it a good test of decision D1. Chunking happens in `zvs-core` (TASK_017). Embedding goes through the AI driver ports on the host (TASK_014), because that is where credentials live and Rust must never see them. Upserting goes through the addon into LanceDB (TASK_019). Metadata — the `vector_document` rows — is written by the host through a repository, because Rust never touches SQLite (D2).

Incremental re-indexing is the feature that makes this usable rather than a party trick: hash each source file, skip anything unchanged, and delete the vectors of anything removed. TASK_020 already added `contentHash` for exactly this.

## Why

Retrieval is only as good as what is in the store. Without ingestion, chat's citations are limited to whatever was seeded by hand.

## Depends on

- `TASK_027` — zvs-jobd sidecar — protocol, lifecycle, health, cancel.

## Read first

- `/design/VectorStores.dc.html` — the Documents tab and the re-index affordance
- `/design/Tasks.dc.html` — how an indexing run is presented, including the unit counts
- `/docs/PASSPORT.md` §10, §2 D2

## Scope

**In scope**
- Source ingestion: files, folders (recursive, with include/exclude patterns)
- Text extraction for plain text, markdown, code and CSV
- Chunk → embed (batched) → upsert pipeline as a kernel job
- Incremental re-index by content hash; deletion of vectors for removed sources
- Progress reporting granular enough to be useful
- Documents tab on the Vector Stores page
- Tests with a fake embedding driver

**Out of scope** (do not do these, even if tempting)
- PDF and office extraction — that depends on TASK_042/TASK_043; wire the hook, leave the formats unsupported for now
- OCR (later)
- Scheduled re-indexing

## Steps

1. Ingestion source model: a store has sources (a file, a folder with patterns). Persist them so re-index knows what to walk.
2. Walk sources, hash each file with `zvs-core`, and diff against `vector_document` rows: new, changed, unchanged, removed. Only new and changed go through the pipeline; removed have their vectors deleted by source id.
3. Extraction per file type, behind a small registry so TASK_043 can add PDF and office formats by registering rather than editing this file.
4. Chunk via `zvs-core` using the store's chunk size and overlap. Embed in batches sized to the provider's limits, with concurrency capped so a large corpus does not trip rate limits — and handle `RATE_LIMITED` by backing off rather than failing the run.
5. Upsert into LanceDB through the addon, then write `vector_document` rows through the repository, in that order, so a crash leaves orphaned vectors (recoverable by reconcile) rather than metadata claiming vectors that do not exist.
6. Progress: report files done / total and chunks embedded / estimated, matching the mockup's `19 402 of 31 402 chunks embedded`. Emit at a sensible rate, not per chunk.
7. Cancellation at every batch boundary; a cancelled index leaves the store consistent and reports how far it got.
8. On completion, run `reconcile` (TASK_020) so the stat strip is accurate.
9. Documents tab: list indexed documents with path, chunk count, size and indexed-at, with a remove action that deletes its vectors.
10. Tests with a fake embedding driver: unchanged files are skipped on re-index; a changed file replaces exactly its own vectors; a removed file's vectors are deleted; cancellation leaves consistent state; rate-limit backoff is exercised.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Index a folder of a few hundred text files against a real embedding provider. Watch progress on the Tasks page. Cancel halfway, confirm consistency. Re-run and confirm it skips everything unchanged. Edit one file, re-run, confirm only that file is re-embedded. Then search the store from the Vector Stores page and confirm the new content is findable.

## Definition of done

- [x] Folder and file ingestion working end to end
- [x] Incremental re-index skips unchanged and cleans up removed
- [x] Progress is accurate and appears on the Tasks page
- [x] Cancellation leaves the store consistent
- [x] Rate limiting handled by backoff, not failure
- [x] Documents tab lists what was indexed
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `vector_source` table (migration `0012_vector_source`) with `VectorSourceRepository`;
a store keeps file and folder sources with include/exclude patterns and a recursive flag, unique per
store path and cascading with the store. `host/indexing/` holds the pipeline: `patterns.ts`
(segment-aware glob matcher; a bare name matches at any depth and, in an exclude, its whole
subtree), `walk.ts` (recursive walk with directory pruning, per-file size cap, dedup across
overlapping sources and non-fatal notes for unreadable paths), `extraction.ts` (an
`ExtractorRegistry` keyed by extension — plain text, markdown, code and a CSV/TSV extractor that
turns rows into `column: value` lines; PDF and office extensions are *deferred*, so they are
skipped with a named reason instead of an unknown-format one), and `IndexingService.ts` (the
chunk → embed → upsert → record pipeline). `vector.index` is a kernel node, so an index is a real
run: `vectorStores.index` starts a one-node `indexing`-kind run, progress events carry chunks
embedded against an estimated total, and cancelling the run cancels the pipeline. Seven new IPC
channels cover sources (list/add/remove/pick), documents (list/remove) and `index`. Renderer: a
Documents tab on the Vector Stores page with source chips, an Electron folder/file picker, index
and full-reindex buttons, a live progress bar and per-document remove; the Tasks page learned the
`indexing` run kind and renders «N из M фрагментов встроено». Tests: `apps/studio/tests/indexing.test.ts`
(18 cases) against a fake embedding driver and a `FakeCore` that now upserts and deletes for real.

**Deviated from plan:**

- **Host-orchestrated, not in the sidecar.** §10 lists corpus indexing as sidecar work, but the
  task's own steps put chunking in the addon, embedding on the host (credentials never reach Rust)
  and metadata in repositories (D2), which leaves the sidecar nothing but a relay. The pipeline is
  therefore a `vector.index` kernel node on the host; the sidecar keeps `job.run` for work that is
  genuinely Rust-side. Moving it later means moving one file, not the protocol.
- **`indexing` is a new `RunKind`,** matching the mockup's Kind filter. `RunService.admit` exempts
  `vector.index` inside an `indexing` run the same way it exempts chat's own nodes — the user
  pressed the button, so a permission prompt would be a dead end on a page with no approval UI.
- **Progress totals are an estimate that converges.** The real chunk count is only known after a
  file is chunked, so the total starts as `bytes / 6 / (chunkSize - overlap)` per pending file and
  each finished file swaps its estimate for the truth; the last report always lands on the exact
  count. Reports are rate-limited to 250 ms plus one forced report per file.
- **Exclusivity is held per file, not per run.** `VectorStoreService.withExclusiveStore` wraps only
  the native write and the metadata row, so searching a store during a long index still works.
  `IndexingService` keeps its own per-store lock so two index runs cannot overlap.
- **A file that fails is counted, not fatal.** Extraction, embedding (after the retry budget) or
  upsert failures add a note to the report and the run continues; only cancellation stops the walk.
- **No pause,** as in TASK_027 — cancel is the only control.

**Next session should know:**

- Order is deliberate: vectors are written before the `vector_document` row, so a crash leaves
  orphaned vectors that `reconcile` can see rather than metadata promising vectors that are not
  there. A changed file deletes by `documentId` before upserting, so a shrinking file leaves no
  stale chunks. Vector ids are `${documentId}:${chunkIndex}`.
- Document identity is the **absolute source path**, and `vector_document` is unique on
  (store, path). Moving a file reads as a delete plus an add.
- TASK_042/TASK_043 add PDF and office support by calling `registry.register([".pdf"], extractor)`
  on the registry passed to `IndexingService` — `extraction.ts` needs no edit, and the deferred
  list there is what makes today's message say «формат .pdf пока не поддерживается».
- Rate limits back off at `backoffMs * 2^(attempt-1)` for `maxAttempts` (500 ms, 5) and only on
  `RATE_LIMITED`; everything else fails the file immediately. `sleep` is injectable for tests.
- Embedding batches are 32 with concurrency 2 by default; both are options, and the provider's real
  limits are still guesswork until a provider declares them.
- `pickSources` is wired from `main.ts` through the handler factory as an optional callback, so the
  IPC tests construct the handlers without Electron and get `CONFLICT` from `sources.pick`.
- Gates run on Windows: `pnpm --filter studio test` (402 passed, 2 skipped), `pnpm test`
  (450 passed, 2 skipped), `pnpm typecheck`, `pnpm lint`, `pnpm lint:tailwind`, `pnpm build`.
  `pnpm format:check` still reports the drizzle-generated `meta/*.json`, as it did before.
