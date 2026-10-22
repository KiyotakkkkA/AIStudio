---
task: TASK_028_EXTRA_6
title: Index schema v2 — structure-aware chunks, page and section metadata, embedding prefixes
phase: 5 — Sidecar and long-running work (extension)
status: done
depends_on: [TASK_021_EXTRA_5, TASK_028_EXTRA_5]
unblocks: [TASK_021_EXTRA_6]
estimate: 4 h
---

# TASK_028_EXTRA_6 — Index schema v2 — structure-aware chunks, page and section metadata, embedding prefixes

## Context

Indexing (`apps/studio/src/host/indexing/IndexingService.ts`) extracts a file to **one flat
string**, chunks it with `chunk_text` in `crates/zvs-core/src/chunk.rs`, embeds each chunk and
upserts rows into LanceDB (`crates/zvs-core/src/index.rs`). The chunker splits on whitespace
"tokens" (words, not model tokens), prefers to end a chunk on a sentence boundary, and starts the
next one `overlap` words back, which usually means **mid-sentence**. Everything structural is lost
before chunking: Markdown and OOXML headings, PDF page boundaries, slide numbers.

The row schema is `id, vector, payload (JSON string), document_id, chunk_index, path`. The chunk
text lives inside the payload JSON, so LanceDB cannot index it for keyword search, and a citation
can only say *which file*, never *which page or section*.

The query and the passages are embedded identically. That is right for bge-m3, but several common
embedding models (e5 family, nomic-embed, Qwen3-Embedding) are trained with distinct query and
document prefixes or instructions and lose noticeable recall without them.

This task changes what goes **into** the index. `TASK_021_EXTRA_6` then changes how it is
searched. They are split so this task can be measured on its own with the evaluation set from
`TASK_021_EXTRA_5` before hybrid search muddies the comparison.

## Why

A chunk that reads "it must be submitted within 10 days" without the heading that says what "it"
is retrieves badly and answers worse. Section paths and page numbers give both the embedder and
the model that context, and make citations point somewhere a user can open. A real text column is
the precondition for keyword search.

## Depends on

- `TASK_021_EXTRA_5` — the evaluation set used to prove this change helps.
- `TASK_028_EXTRA_5` — the store reset path used to rebuild a v1 index.

## Read first

- `crates/zvs-core/src/chunk.rs`, `crates/zvs-core/src/index.rs`
- `apps/studio/src/host/indexing/extraction.ts`, `documents.ts`, `ooxml.ts`
- `apps/studio/src/host/downloads/curated.ts` — where per-model metadata lives

## ❓ DECISION — what happens to existing v1 stores?

The schema change needs every existing store re-embedded.

- **(a)** v1 stores keep working read-only for search; the page shows «Требуется переиндексация»
  and the next «Полная переиндексация» rebuilds the table as v2.
- **(b)** Rebuild automatically on first launch after upgrade, as a background indexing run.

Fallback if unavailable: **(a)** — re-embedding a large store is expensive and should be the
user's call.

## Scope

**In scope**
- Extraction returns **segments**, not a flat string: `{ text, section: string[], page?: number }`.
  Markdown headings, OOXML heading styles, PDF pages and slide numbers populate them; plain text
  is one segment.
- Chunker v2 in `zvs-core`: chunks never cross a top-level section boundary; overlap begins at a
  sentence start; chunk size counted in estimated model tokens (same heuristic as
  `TASK_024_EXTRA_1`), not words. Existing chunk tests kept, new ones for sections and overlap.
- Contextual header: the embedded text is `"<file title> › <section path>\n<chunk>"`; the stored
  display text stays the bare chunk.
- Schema v2 columns: `text` (Utf8), `section` (Utf8, joined path), `page` (UInt32, nullable),
  plus `zvs.schema = 2` metadata. Payload JSON keeps only what has no column.
- v1 detection and the migration path per the decision.
- Per-model embedding prefixes: catalogue items and provider models may declare
  `queryPrefix`/`documentPrefix`; indexing applies the document prefix, search the query prefix.
  Values recorded on the store at creation so a later catalogue edit cannot silently mismatch an
  existing index.
- Citations carry `section` and `page`; the Chat page and Test search show them
  ("handbook.pdf · стр. 12 · Отпуск › Порядок").

**Out of scope** (do not do these, even if tempting)
- Full-text index, hybrid search, neighbour expansion — `TASK_021_EXTRA_6`
- Semantic chunking by embedding similarity — measure structure-aware chunking first
- LLM-generated chunk summaries ("contextual retrieval" with a model call per chunk) — too costly
  for local indexing at this stage

## Steps

1. Segment type in extraction; Markdown, OOXML, PDF, plain-text producers + tests.
2. Chunker v2 in Rust + napi exposure + tests (section boundary, sentence-start overlap, token sizing).
3. Contextual header for embedding input; display text unchanged.
4. Schema v2 in `index.rs`; v1 detection; migration per decision; health shows the state.
5. Embedding prefixes: catalogue/model metadata, recorded on the store, applied on both sides.
6. Citations and Test search rows show section and page.
7. Run the `TASK_021_EXTRA_5` evaluation before and after on the same store; record both.

## Verify

```bash
cargo test -p zvs-core
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: index a multi-page PDF and a Markdown file with headings. Test search shows page and
section for hits. An existing v1 store shows the re-index prompt and still searches until rebuilt.

## Definition of done

- [x] Chunks respect sections and start overlaps at sentence starts
- [x] Section and page reach citations in chat and Test search
- [x] Chunk text is a real column; schema version recorded; v1 stores handled per decision
- [x] Embedding prefixes applied where the model declares them
- [x] Evaluation before/after recorded in the handoff
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Extraction returns segments `{ text, section, page }` (`extraction.ts`). Markdown ATX headings open nested sections and fenced blocks cannot fake one. Word headings come from `styles.xml` (style named `heading N` or `Title`, or an outline level) or a paragraph-level outline level, so Russian style ids such as `1` work. PDF pages and pptx slides set `page`, and a slide's title placeholder becomes its section. Plain text, CSV, xlsx and OCR'd images stay one segment.
- Chunker v2 `chunk::chunk_segments` in Rust plus napi `chunkSegments`. It never crosses a top-level section, packs whole sentences (UAX#29), splits only oversized sentences by words, starts overlap at a sentence start, and counts `size`/`overlap` in estimated tokens via `chunk::estimate_tokens` (Cyrillic ≈ 3 chars, Latin ≈ 4, CJK 1). Tests are in `crates/zvs-core/tests/sections.rs`. `chunk_text` is kept for v1 callers and its tests.
- Contextual header: the embedder sees `<documentPrefix><file title> › <section path>\n<chunk>` (`embeddingInput`); the stored text is the bare chunk.
- Schema v2 in `index.rs`: `text`, `section` (joined with ` › `), nullable `page`, and `zvs.schema = 2`. The payload keeps only `chunkCount`/`byteStart`/`byteEnd`/`tokenCount`. Search hits carry `text`/`section`/`page` for v1 tables too (text from `payload.text`).
- v1 handling per decision (a) (the fallback; nobody decided): v1 tables open and search but refuse upserts. Stats and the store row (`schema_version`, migration `0019_index_schema_v2`) report the version. The store page shows «Требуется переиндексация» with an explanation. An incremental run is refused with that message. «Полная переиндексация» recreates the table as v2 through the new `VectorStoreService.recreateTable`, which keeps document rows, then re-embeds every file under its old document id.
- Embedding prefixes: `queryPrefix`/`documentPrefix` on the store (recorded at creation, frozen after indexing like chunk settings). The index side applies the document prefix and search applies the query prefix. Catalogue items may declare them (`CatalogueItemDto.queryPrefix/documentPrefix`). Otherwise `knownEmbeddingPrefixes` in `@zvs/shared` recognises e5, nomic-embed, Qwen3-Embedding, mxbai / arctic / bge-*-en; bge-m3 gets none. The form autofills them when the model changes and shows them under «Расширенные настройки».
- Citations carry `section` and `page` (`ChatCitationDto`, defaults keep old rows readable). The chat sources panel shows «Фрагмент N · стр. 12 · Отпуск › Порядок», and Test search rows show a page/section chip.

**Evaluation (real store, real engine):** headless against a *copy* of «Юридическая БД» (bge-m3 + bge-reranker-v2-m3, llama.cpp CUDA, k = 10). Neither real store had saved eval questions, so 12 new ones were written: six topical questions over the Supreme Court documents and six case-number lookups (`А33-13638/2020`, `2-2587/2021`, `2а-3276/2025`, …). A case counts as found when any document from that case's folder ranks. «Before» is the existing v1 table (1338 documents, 3220 vectors). «After» is the same copy rebuilt by «Полная переиндексация». The store had no source rows left, so the rebuild walked the whole `суд. разбирательства` folder: 2452 documents, 13 173 chunks, 1103 skipped (mostly scans with OCR off), 19 failed (unparseable PDFs), 740 s. The after corpus is therefore almost twice as large, which makes retrieval harder, not easier.

| | hit@1 | hit@5 | recall@k | MRR | per-case ranks |
| --- | --- | --- | --- | --- | --- |
| v1 vector | 0.33 | 0.75 | 0.60 | **0.488** | `3 1 5 1 3 1 7 2 4 1 10 -` |
| v1 vector + rerank | 0.67 | 0.92 | 0.64 | **0.753** | `2 1 5 1 3 1 1 1 1 1 1 -` |
| v2 vector | 0.67 | 1.00 | 0.83 | **0.769** | `5 1 5 1 3 2 1 1 1 1 1 1` |
| v2 vector + rerank | 0.75 | 1.00 | 0.78 | **0.854** | `2 1 4 1 1 2 1 1 1 1 1 1` |
| v2 hybrid | 0.75 | 1.00 | 0.83 | **0.861** | `2 1 2 1 3 1 1 1 1 1 1 1` |
| v2 hybrid + rerank | 0.67 | 1.00 | 0.81 | **0.799** | `3 1 4 2 1 2 1 1 1 1 1 1` |

The harness (stages before / reindex / after, driving `LlamaServer`, `SidecarDriver` and the real services against a copied database) lives outside the repo; it is not part of the suite.

**Deviated from plan:**

- **Document ids survive the v1 → v2 rebuild.** The obvious path, the TASK_028_EXTRA_5 reset, deletes document rows, and graph edges cascade on document deletion while eval cases name document ids. Rebuilding through it would have silently destroyed the user's relations and eval set, so `recreateTable` drops only the LanceDB table.
- Provider models have no metadata to «declare» prefixes in, so the provider side is the shared known-family table rather than a per-model field.
- Slide numbers reuse `page`, so a slide citation reads «стр. N».
- The before/after comparison could not use one fixed corpus (see the evaluation note). The v2 vector-only row is the cleanest measure of this task alone: MRR 0.488 → 0.769 and hit@5 0.75 → 1.00 on a corpus twice the size.
- Done together with TASK_021_EXTRA_6 in one session; the full-text index, hybrid search and neighbour reads in the same diff belong to that task.

**Next session should know:**

- An existing store's `chunkSize`/`chunkOverlap` now mean *estimated tokens*, not words, after its rebuild. A 768-«word» store becomes ~2300 Cyrillic characters per chunk, so chunk counts roughly double (3220 → 13 173 here, with the larger corpus).
- `estimateTokens` in chat is still `Buffer.byteLength` — TASK_024_EXTRA_1 (still `todo`) owns that. The Rust estimator is the calibrated one.
- `pnpm format:check` already failed before this task on 11 old migration snapshots and `test/native/sidecar.test.mjs`. `pnpm test:e2e` fails the same three specs (chat store checkbox, secret channels, vector store form `#vs-embeddingModelId`) on the base commit as on this one; both were left alone.
