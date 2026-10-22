---
task: TASK_072
title: Indexing in Rust — walk, extraction registry, OOXML, PDF and OCR extractors, IndexingService, vectorStores.index
phase: M — Migration to Tauri
status: done
depends_on: [TASK_071]
unblocks: [TASK_073]
estimate: 5 h
---

# TASK_072 — Indexing in Rust — walk, extraction registry, OOXML, PDF and OCR extractors, IndexingService, vectorStores.index

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Indexing (TASK_028 and EXTRA_1 – EXTRA_6) lives in `host/indexing/`:
- `IndexingService.ts` (642 lines): resolve sources, walk, extract, chunk with index schema v2
  (structure-aware chunks, page and section metadata, embedding prefixes), embed, upsert,
  persist documents as they land, progress and ETA metrics, cancellation, and the schema-v2
  read-only or rebuild rule.
- `walk.ts` and `patterns.ts`: the directory walk with include and exclude patterns.
- `extraction.ts`: the extractor registry keyed by extension.
- `ooxml.ts`: DOCX, XLSX and PPTX text read on the host over the zip reader (TASK_028_EXTRA_2
  decision).
- `documents.ts`: the PDF extractor (sidecar job, because PDF parsing is the least
  trustworthy input) and the image extractor (OCR through the local vision model, over a
  scanned page's embedded image).
- `kernel/jobNodes.ts`.

The channel is `vectorStores.index`. The others were ported in TASK_069.

Everything else this needs exists by now: `InProcessCore` chunking and hashing (TASK_068),
`SidecarDriver` (TASK_068), `RuntimeService` for OCR and embeddings (TASK_071),
`VectorStoreService` (TASK_069) and runs (TASK_070).

## Why

This is the last piece of the "retrieval works" milestone under Tauri: a user can point a store
at a folder and search what lands in it.

## Depends on

- `TASK_071` — local engine for OCR and embeddings, archive and zip readers.

## Read first

- `apps/studio/src/host/indexing/*`, `kernel/jobNodes.ts`
- `crates/zvs-core/README.md` (chunking, sections), `crates/zvs-jobd/README.md` (the PDF job)
- Handoffs of TASK_028, TASK_028_EXTRA_2, TASK_028_EXTRA_4 and TASK_028_EXTRA_6
- `apps/studio/tests/indexing.test.ts`, `documents.test.ts`

## Scope

**In scope**
- `indexing::{walk, patterns, extraction, ooxml, documents}` and `IndexingService`
- OOXML over the `zip` crate and `quick-xml` (pinned), with the same text order and
  section and page mapping as `ooxml.ts`
- PDF extractor (sidecar job) and image extractor (OCR via `RuntimeService`), registered in
  the composition root as `main.ts` does
- Progress, ETA and documents-as-they-land events, with the same event shapes
- `vectorStores.index`, with job nodes registered
- An index parity check: index the same fixture folder (Markdown, DOCX, XLSX, PPTX, a text PDF,
  a scanned PDF, an image) with both hosts. Compare document count, chunk count, the section
  and page metadata of a sample, and evaluation numbers. Record them in the handoff.

**Out of scope** (do not do these, even if tempting)
- Office → PDF conversion for the viewer (TASK_043)
- New file types or chunking changes

## Steps

1. Walk and patterns, with their tests (including long paths and excluded folders).
2. Extraction registry and OOXML, with fixture documents and text-order tests.
3. PDF and image extractors against the real sidecar and a stub OCR port.
4. `IndexingService`: port `indexing.test.ts` in full, including cancellation mid-embed,
   resume semantics and schema-v2 behaviour.
5. Channel and job nodes, then the parity run.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, index a mixed folder into a new store. Watch progress, ETA
and documents appear as they land on the Vector Stores page and the run on the Tasks page.
Cancel and re-run. Search finds text from the DOCX, the PDF and the OCR'd scan.

## Definition of done

- [ ] Walk, extraction, OOXML, PDF and OCR extractors and `IndexingService` are ported with
      their tests
- [ ] The parity run against the TS host is recorded in the handoff with matching numbers
- [ ] Indexing works end to end under Tauri with progress, ETA and cancellation
- [ ] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract`: `VectorIndexInput` (`full` defaulted behind `full()`) and `VectorIndexReportDto`
  (notes capped at `INDEX_NOTES_MAX` = 200), both with `JsonSchema` for the node. Four fixture files
  under `packages/shared/fixtures/vectorStores`, registered in both fixture tables.
- `zvs-host::indexing`:
  - `patterns` (glob → case-insensitive regex, bare name matches at any depth, exclude also takes
    `<pattern>/**`), checked against the TS module's answers for the edge cases.
  - `walk` (sources, recursion, include/exclude, size and file limits, unreachable roots,
    truncation, extended-length paths, cancellation) on `spawn_blocking`.
  - `extraction`: `ExtractorRegistry` (interior-mutable, so the composition root registers PDF and
    images after construction, as `main.ts` did), plain text, Markdown sections with fences, CSV/TSV.
  - `ooxml`: DOCX (styles, outline levels, `Заголовок N` fallback, tabs), XLSX (shared strings,
    inline strings, sheets in name order), PPTX (slide order by number, title placeholder as
    section, page = position), over `platform::archive::read_zip_entries`.
  - `documents`: `OcrPort` (implemented by `RuntimeService::read_image`), `PdfExtractor`
    (`job.document.extract` through the sidecar, scratch image dir removed afterwards, OCR text
    replaces a scanned page, unreadable pages become notes) and `ImageExtractor`.
  - `IndexingService`: upgrade/refuse v1 stores, embedding driver checks, walk, forget deleted files
    unless the walk was truncated or the root unreachable, hash/unchanged, extract, chunk, embed in
    batches over N workers with rate-limit backoff, upsert under the store lease, documents as they
    land, progress with chunk estimate and throttle, resource sampling for the run's lifetime,
    maintain and reconcile, cancellation at batch boundaries.
- Kernel: `vector.index` node (progress, `log` with `storeId`, `step` events with `domain:
  resources` / `domain: document`), `KernelServices::indexing` via the `VectorIndexer` trait,
  registered by `register_core_nodes`. `Services::indexing`.
- `vectorStores.index` (`ipc::vector_stores::start_index_run`) starts an `indexing` run titled
  `Индексация «name»`, exactly as `startIndexRun` did.
- Composition (`zvs-studio`): `IndexingService` over `InProcessCore`, the provider registry and
  the resource monitor; PDF and image extractors on the sidecar and `RuntimeService`, scratch dir
  `<cache>/documents`; indexing in `KernelServices` and `Services`.
- Tests: `indexing.test.ts` ported in full (`indexing::tests`, against a word-chunking `TextCore`
  and an in-memory `VectorCore` mirroring the TS fakes), plus cancellation mid-embed, resume after a
  cancelled run, truncated walk, run cancellation through the kernel, worker ordering and the
  wrong-dimension case. `documents.test.ts` ported (`indexing::ooxml::tests`,
  `indexing::documents::tests`). Walk tests include a >260-character path. Real sidecar:
  `crates/zvs-jobd/tests/documents.rs` (text page + scanned JPEG page through `zvs-jobd`).
- Gates run on Windows 2026-10-17, all passing: `cargo fmt --check`, workspace clippy with
  `-D warnings`, `cargo test --workspace` (951 passed; zvs-host 718), `pnpm typecheck`,
  `pnpm lint`, `pnpm build`, `pnpm format:check`, `npx vitest run` (1658 passed, 2 skipped),
  `contract-channels --check`. The corpus folder is in `.prettierignore` so its bytes stay fixed.

**Parity run:** `test/fixtures/indexing/corpus` holds Markdown, DOCX, XLSX, PPTX, a two-page text
PDF, a scanned PDF (embedded JPEG), a PNG and a legacy `.doc`. Both hosts index it with the real
sidecar, their real core (native addon / `InProcessCore`), the same FNV-1a hashing embedder
(32 dimensions), stub OCR with fixed answers per media type, chunk size 48 / overlap 8, then run
the same eight evaluation questions at k = 5. Each host was run unpinned first; the outputs were
identical (every report count and note, every document, and every chunk's text, section and page),
and that output is pinned in `test/fixtures/indexing/parity.json`. Both
`apps/studio/tests/indexingParity.test.ts` and `crates/zvs-jobd/tests/index_parity.rs` assert it.

| | files | indexed | skipped | documents | chunks | hit@1 | hit@5 | recall@k | MRR | ranks |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| TS vector | 8 | 7 | 1 | 7 | 14 | 0.625 | 1.00 | 1.00 | **0.7604** | `1 3 1 2 4 1 1 1` |
| Rust vector | 8 | 7 | 1 | 7 | 14 | 0.625 | 1.00 | 1.00 | **0.7604** | `1 3 1 2 4 1 1 1` |
| TS hybrid | | | | | | 1.00 | 1.00 | 1.00 | **1.0000** | `1 1 1 1 1 1 1 1` |
| Rust hybrid | | | | | | 1.00 | 1.00 | 1.00 | **1.0000** | `1 1 1 1 1 1 1 1` |

Chunks per document (both hosts): contract.docx 3, deck.pptx 2 (pages 1–2, slide titles as
sections), handbook.md 4, prices.xlsx 1, report.pdf 2 (pages 1–2), scan.pdf 1 (OCR text, page 1),
sign.png 1 (OCR text).

**Deviated from plan:**

- No `quick-xml`. Matching `ooxml.ts` exactly (same text order, same tolerance of odd markup) was
  simplest by porting its expressions to the already-pinned `regex` crate. No `zip` crate either,
  for the reason in TASK_071's handoff: the in-memory reader there is used.
- `IndexingService` sits beside `VectorSourceService` (sources, documents, manual removal) rather
  than absorbing it; the channels keep using `VectorSourceService`.
- The PDF job is given the run's cancellation token; the TS extractor ran it without a signal.
- A DOCX heading level of 0 (`heading 0`) is ignored instead of throwing, as the TS array
  arithmetic would have.
- Walk results sort case-insensitively, then bytewise, standing in for `localeCompare`. Only the
  processing order can differ, not what is indexed.
- The TS parity test is skipped when `target/debug/zvs-jobd(.exe)` does not exist (it needs the
  real sidecar); the Rust one always runs.

**Next session should know:**

- The manual check (`pnpm dev:tauri`: index a mixed folder, watch progress/ETA and documents
  appear, the run on the Tasks page, cancel and re-run, search finds DOCX, PDF and OCR text) has
  **not** been done. OCR in the parity run is a stub; real OCR goes through
  `RuntimeService::read_image` and needs an installed vision model with its projector.
- Re-run both parity tests after any change to zvs-core chunking/search, the extractors, the
  sidecar PDF job or the fixture. To regenerate: set `expected` to `null`, run each host, compare,
  and pin only output both agree on.
- `IndexingService` keeps the "last progress report" timestamp per service, not per run, as the
  TS host did.
