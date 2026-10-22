---
task: TASK_021_EXTRA_6
title: Hybrid search, neighbour expansion and graph-augmented chat retrieval
phase: 3 — Rust core and vector search (extension)
status: done
depends_on: [TASK_028_EXTRA_6]
unblocks: [TASK_034_EXTRA_2]
estimate: 4 h
---

# TASK_021_EXTRA_6 — Hybrid search, neighbour expansion and graph-augmented chat retrieval

## Context

After `TASK_028_EXTRA_6` every chunk row has a real `text` column, a `section` and a `page`. Search
is still vector-only: `VectorIndex::search` in `crates/zvs-core/src/index.rs` runs `nearest_to`
with `bypass_vector_index()` — an exact scan of every row — and returns the top `k`.

Vector-only search has a known blind spot: exact identifiers. Article numbers, error codes,
product names, abbreviations and surnames embed poorly and are exactly what users type. LanceDB
ships a full-text (BM25) index that can be built over the `text` column and queried alongside the
vector search; the two result lists are then fused by rank.

Two more retrieval gaps remain on the context side:

- A hit is one chunk. The sentence that answers the question often sits in the chunk next to it.
  Fetching the matched chunk's neighbours (±1) for the model — "small-to-big" — gives it the
  surrounding context without making the indexed chunks bigger.
- `TASK_021_EXTRA_4` proved graph expansion on Test search but deliberately kept it out of chat,
  and graph neighbours are documents, not chunks, so they had no text to show.

Finally, the exact scan is fine for tens of thousands of vectors and becomes the bottleneck past
roughly a hundred thousand.

## Why

Hybrid search fixes the queries vector search gets most visibly wrong. Neighbour expansion and
graph-in-chat turn the relations and structure the index already holds into context the model can
actually use. Together with reranking this completes the retrieval pipeline that the agentic tool
in `TASK_034_EXTRA_2` will call.

## Depends on

- `TASK_028_EXTRA_6` — the `text`/`section`/`page` columns and schema v2.

## Read first

- `crates/zvs-core/src/index.rs` — `search`, `stats`, index type reporting
- `apps/studio/src/renderer/features/vector-stores/graphExpansion.ts` — the merge rules to reuse
- `apps/studio/src/host/services/chatHistory.ts` — where expanded context is budgeted
- LanceDB docs for the full-text index and hybrid query in the crate version pinned in `Cargo.lock`

## Scope

**In scope**
- `zvs-core`: build and maintain a full-text index on `text` (created with the table, refreshed
  after upserts/deletes); `keyword_search(query, k, filter)`.
- Hybrid search: vector and keyword lists fused by Reciprocal Rank Fusion (k = 60), then the rerank
  stage from `TASK_021_EXTRA_5` when enabled. Each hit carries its provenance
  (`vector` / `keyword` / `both`). A per-store `searchMode` (`hybrid` default for v2, `vector`) for
  comparison.
- `chunks(documentId, indices)` in `zvs-core` + napi; chat assembly adds ±1 neighbours of each
  injected hit when the budget allows, merged into one passage per contiguous run, citation kept
  on the matched chunk.
- Graph in chat: an «Использовать связи» option per conversation (off by default); for each
  document reached one hop from a top hit, its best chunk is found by a vector search filtered to
  that document (`only_if document_id = …`) and added below ranked sources, marked as related.
- ANN index: when a store passes a row threshold (default 100 000) build an IVF-based vector index
  and stop bypassing it; `indexType` on the store reflects it; below the threshold keep the exact
  scan.
- Test search: mode toggle (hybrid / vector), provenance chip per row, neighbour preview.
- Evaluation from `TASK_021_EXTRA_5` gains a mode column: vector, hybrid, hybrid + rerank.

**Out of scope** (do not do these, even if tempting)
- bge-m3 sparse/multi-vector outputs — revisit only if BM25 hybrid underperforms on the eval set
- Multi-hop graph expansion
- Query-side spelling correction or synonym expansion

## Steps

1. Full-text index lifecycle in `zvs-core` + tests (create, update after upsert/delete, rebuild).
2. Keyword search + RRF fusion (pure function, unit-tested on known rank lists).
3. Pipeline: fetch both → fuse → rerank → floor/merge in chat; `searchMode` setting.
4. `chunks(documentId, indices)` + neighbour expansion in chat assembly, budget-aware.
5. Graph-in-chat option, document-filtered best-chunk lookup, related-sources marking.
6. ANN threshold and index build; stats/indexType reporting.
7. Test search mode toggle and provenance; eval modes; record numbers in the handoff.

## Verify

```bash
cargo test -p zvs-core
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: search a store for an exact code or article number that appears in one document —
vector mode misses or ranks it low, hybrid ranks it first. Ask chat a question whose answer spans
two adjacent chunks and confirm the neighbour arrives in the context.

## Definition of done

- [x] Hybrid search with RRF, provenance visible, mode switchable per store
- [x] Neighbour expansion in chat within the budget
- [x] Graph expansion available in chat, off by default, clearly marked
- [x] ANN index above the threshold; exact scan below it
- [x] Evaluation numbers for vector / hybrid / hybrid + rerank in the handoff
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- Full-text index: a BM25 index on `text` (Russian stemming and stop words, lower-casing, ASCII folding) is created with every v2 table. `VectorIndex::maintain` creates it if missing and folds new rows into it; rows added since are still searched, and deleted rows never match. `keyword_search(query, k, filter)`.
- Hybrid: `hybrid_search` fetches k from each side and fuses with `reciprocal_rank_fusion` (k = 60), a pure function unit-tested on hand-computed lists in `crates/zvs-core/tests/retrieval.rs`. Hits carry `provenance` (`vector`/`keyword`/`both`), `keywordScore` (raw BM25) and `fusedScore`. `score` stays the vector similarity, computed for keyword-only hits too, so floors keep their meaning, and keyword hits are exempt from the floor. Pipeline in `searchTimed`: fetch both → fuse → rerank (when enabled, `max(k, candidates)` from each side) → `minScore`/`k`. Per-store `searchMode` (`hybrid` default, `vector`), overridable per call. v1 stores always run vector-only (`effectiveMode`). Results report the `mode` used.
- `chunks(documentId, indices)` in `zvs-core`, napi `chunks`, `VectorStoreService.chunks`, IPC `vectorStores.chunks`.
- Neighbour expansion in chat: `chat.prepare` fetches ±1 chunks of every ranked hit (one call per document) through the kernel services. `windowChatHistory` adds them only from budget the ranked sources left over and merges each contiguous run into one passage (overlap trimmed by byte offsets, `joinChunks`). The citation stays on the best-ranked matched chunk of the run.
- Graph in chat: «Использовать связи» per conversation (`conversation.use_relations`, off by default, toggle in the sources dialog, IPC `chat.conversations.updateRelations`). For the documents one hop from the ranked hits (merge rule `mergeGraphNeighbors`, moved from the renderer to `@zvs/shared` and reused by both), the best chunk is found by a vector search filtered to that document (`relatedChunks`, `documentIds` → `document_id IN (...)`). Up to 3 go below the ranked sources under their own heading, marked «связь «…»» in the prompt, and `ChatCitationDto.related` carries the relation for the sources panel. A run log line records sources / neighbours / related per turn.
- ANN: `maintain(threshold)` trains an `IVF_HNSW_SQ` index once a table holds ≥ 100 000 rows (`DEFAULT_ANN_THRESHOLD`). Search stops bypassing it (`nprobes` 32, `refine_factor` 4), and `indexType` on the store reflects it. Below the threshold it keeps the exact scan. Indexing calls `maintain` after any run that changed something and after a document is removed.
- Test search: mode select (disabled on v1), mode chip, provenance chip and bm25 score per row, page/section chip, «Соседние фрагменты» preview loaded on demand.
- Evaluation modes: `vector`, `hybrid`, `hybrid-rerank` on v2 stores; `vector`, `rerank` on v1 (old saved runs keep reading).

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

Manual check, exact identifiers: on v1 vector search the case-number questions ranked `7 2 4 1 10 -`. With hybrid all six rank 1. The native test `vectorStoreNative.test.ts` pins the mechanism: a query only BM25 can match outranks the vector top hit.

**Deviated from plan:**

- Fusion runs in Rust inside one `search` call (`mode: "hybrid"`) instead of two calls fused in TypeScript. It saves a round trip, and keyword-only hits get their vector score from the same table read.
- RRF ties prefer the entry with a keyword rank. With plain RRF, a keyword-only rank-1 hit ties exactly with the vector rank-1 hit and loses or wins on id alone. Preferring the lexical match is what makes «hybrid ranks the exact code first» hold.
- `IVF_HNSW_SQ` rather than IVF_PQ: PQ needs the dimension divisible into sub-vectors and trades recall, while SQ works for any dimension. The threshold is a host constant, not a per-store setting.
- The full-text index is refreshed at the end of an indexing run, not after every upsert; between refreshes new rows are still searched unindexed, which keeps a long run from rebuilding the index per file.
- Step 3's «floor/merge in chat» belongs to TASK_024_EXTRA_1, which is still `todo`. Chat still takes k = 5 per store in store order with byte-based budgeting; the neighbour and graph budget logic sits on top of that.
- Done in the same session as TASK_028_EXTRA_6 (the dependency was not done).

**Next session should know:**

- On this corpus hybrid alone (MRR 0.861) beat hybrid + rerank (0.799). The reranker demoted two topical answers from rank 1–2 to 2–4 while fixing one. Worth re-checking with a bigger, user-written eval set before turning reranking on by default for hybrid stores. `candidates` now applies per side.
- Related documents cost one embedding of the query plus one filtered k = 1 search each. The cap is `CHAT_RELATED_MAX` = 3.
- Neighbour chunks are fetched even when the budget then drops them. That is cheap (one indexed read per document), but a very large k would want a cap.
- The e2e and format-check failures noted in TASK_028_EXTRA_6 predate both tasks.
