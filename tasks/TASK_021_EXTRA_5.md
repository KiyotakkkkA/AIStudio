---
task: TASK_021_EXTRA_5
title: Retrieval evaluation set and the rerank stage
phase: 3 — Rust core and vector search (extension)
status: done
depends_on: [TASK_024_EXTRA_1, TASK_028_EXTRA_1]
unblocks: [TASK_028_EXTRA_6, TASK_021_EXTRA_6]
estimate: 4 h
---

# TASK_021_EXTRA_5 — Retrieval evaluation set and the rerank stage

## Context

A vector store already carries a rerank configuration — `rerank: { enabled, modelRef, candidates }`
on `VectorStoreDto`, a form section in `VectorStoreAdvanced.tsx`, auto-fill that picks the curated
`curated:model:bge-reranker-v2-m3-f16` — and the Downloads catalogue ships that model. **Nothing
uses it.** `VectorStoreService.searchTimed` embeds the query and returns LanceDB's nearest
neighbours; the only reference to `rerank` in the host is `requireAdvanced`, which validates that
an enabled stage names a model. A user who turns reranking on gets exactly the results they had
with it off.

The local engine (`apps/studio/src/host/runtimes/RuntimeService.ts`) runs llama.cpp's
`llama-server` with one process per runtime and role — embedding and vision are already separate
roles because `--embeddings` changes how the server behaves. `llama-server` also has a
`--reranking` mode serving a rerank endpoint over a cross-encoder GGUF such as bge-reranker-v2-m3.

There is also no way to tell whether a retrieval change helps. Test search shows one query at a
time; every tuning decision so far — chunk size, floor, k — has been made by eye. Before reranking,
hybrid search and new chunking land, the store needs a small set of questions with known answers
and a number that moves when retrieval gets better or worse.

## Why

Reranking is the single largest relevance gain available without re-indexing: a cross-encoder reads
query and passage together instead of comparing two independent vectors, and the model is already
on the user's disk. The evaluation set is what makes this task, and every retrieval task after it,
provable rather than anecdotal.

## Depends on

- `TASK_024_EXTRA_1` — merged, floored retrieval in chat; reranking slots in between fetch and merge.
- `TASK_028_EXTRA_1` — the local runtime, its roles and GPU plan.

## Read first

- `apps/studio/src/host/runtimes/RuntimeService.ts` — roles, server lifecycle, `embed`
- `apps/studio/src/host/services/VectorStoreService.ts` — `searchTimed`
- `apps/studio/src/renderer/features/vector-stores/` — Test search tab and `searchRows`
- `/design/VectorStores.dc.html` — Test search layout to extend, not replace

## Scope

**In scope**

*Evaluation*
- Table `vector_eval_case` (store id, question, expected document ids, created at) + repository +
  `vectorStores.eval.*` channels (list, add, remove, run).
- A «Проверка качества» section on the Test search tab: add a question, pick the expected
  document(s) from the store's document list, run all cases.
- Run reports per case and in aggregate: hit@1, hit@5, recall@k, MRR — computed by a pure,
  unit-tested function. When the store has reranking configured, the run reports **both**
  "vector only" and "with rerank" side by side, so the gain is visible.
- The last aggregate result is kept with the store so the page shows it without re-running.

*Rerank*
- A `rerank` runtime role in `RuntimeService`: `llama-server --reranking` over the store's rerank
  model file, started lazily, stopped with the others, same GPU plan rules as embedding.
- `RerankPort` (`rerank(query, passages[], signal) → scores[]`) with the local implementation; the
  port is the seam a remote reranker would use later.
- Search pipeline when `rerank.enabled`: fetch `rerank.candidates` by vector, rerank, sort by
  rerank score, return `k`. Hits carry `vectorScore` and `rerankScore` (normalised to [0, 1]);
  `score` is the rerank score so floors and merges keep working.
- Failure is never fatal: reranker missing, crashing or timing out returns vector order and logs a
  warning; chat answers anyway.
- Test search shows both scores and the rerank time next to the existing embedding/search timings.
- Chat uses the same pipeline automatically for stores with reranking on.

**Out of scope** (do not do these, even if tempting)
- Remote rerank APIs (Cohere, Jina) — the port allows it; the implementation is not this task
- LLM-as-judge or generated evaluation questions — cases are written by the user
- Hybrid search — `TASK_021_EXTRA_6`

## Steps

1. Migration, repository and channels for evaluation cases; pure metrics function + tests.
2. Evaluation section UI; run, show per-case rank of the expected document and the aggregate.
3. `rerank` role in `RuntimeService`; verify the endpoint path and response shape against the
   llama.cpp build the app downloads, and pin it in a fixture test.
4. `RerankPort` + local implementation + timeout/fallback behaviour + tests with a fake server.
5. Search pipeline integration; `VectorSearchHitDto` gains optional `vectorScore`/`rerankScore`.
6. Test search rows and timings; eval run compares both modes.
7. Chat path check: a store with reranking on produces reranked citations.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: on a real store, write 10 questions whose answer you know. Run the evaluation with
reranking off, then on. Record both MRR values in the handoff. Kill the rerank server mid-search and
confirm the search still returns (vector order) with a warning in the log.

## Definition of done

- [x] Evaluation cases can be written, run and compared; metrics are unit-tested
- [x] Reranking actually changes results when enabled, and is used by chat
- [x] Reranker failure degrades to vector order, never to an error
- [x] Before/after MRR on a real store recorded in the handoff
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass — TS typecheck (root + studio), lint (studio, shared, tailwind) and the studio build pass; `//#build:rust` is still blocked by a running Electron instance holding `zvs-core.node` (EBUSY), as in TASK_021_EXTRA_4
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `vector_eval_case` table (migration `0018_vector_eval`, which also adds `vector_store.last_eval`), `VectorEvalCaseRepository`, and `VectorEvalService` behind `vectorStores.eval.list/add/remove/run`. A case must name documents that exist in the store; duplicates collapse. A run reports per-case rank of the first expected document plus hit@1, hit@5, recall@k and MRR, and the last run is stored on the store row so the page shows it without re-running.
- `retrievalMetrics.ts`: pure `rankedDocuments` (chunk hits → documents, first-seen order), `scoreCase` and `retrievalMetrics`, unit-tested in `tests/retrievalMetrics.test.ts`.
- Rerank role: `LlamaServer` role `rerank` starts `llama-server --reranking` (full micro-batch like embedding) and calls `/v1/rerank`. Verified against build b10978 (the one installed here): results come back **sorted by score**, keyed by `index`, with **raw logits** in `relevance_score`. That exact response is pinned as a fixture in `tests/llamaRerank.test.ts`. `RuntimeService.rerank(modelRef, …)` resolves the catalogue file and runs it on the same engine/GPU plan as embedding, one process per role.
- `RerankPort` + `localRerankers(runtimes)` in `services/rerank.ts`, attached to `VectorStoreService` in `main.ts` via `attachRerankers` (same pattern as `attachLocalEngine`).
- Pipeline in `searchTimed`: when `rerank.enabled`, fetch `max(k, candidates)` with no floor, rerank, sigmoid-normalise to [0, 1], sort, then apply `minScore` and `k`. Hits carry `vectorScore`/`rerankScore`, and `score` is the rerank score. Results carry `rerankMs` and `rerank: "applied" | "fallback"`. When reranking is off the shape is the same as before. Missing reranker, thrown error, malformed answer and the 20 s timeout all fall back to vector order with a `warn` log. Caller cancellation still cancels. Chat goes through `vector.search` → `search()` and gets the same pipeline (test in `chat.test.ts`).
- UI: «Проверка качества» accordion on Test search (question, expected-document picker, depth, run, metric table with Δ column, per-case ranks per mode, fallback notice). Search rows show `rr`/`vec` scores under the bar, and the timing chips gain `rerank N мс` plus a «без переранжирования» chip on fallback. Store logic lives in `VectorEvalStore` (owned by `VectorStoreStore` as `evaluation`) and `evalRows.ts`, tested in `tests/renderer/vectorEval.test.ts`.

**Manual check (real store, real engine):** run headless against a *copy* of «Юридическая БД» (1338 documents, 3220 vectors, bge-m3 + bge-reranker-v2-m3 on llama.cpp CUDA b10978), 10 hand-written questions over the Supreme Court documents and receipts, k = 10:

| | hit@1 | hit@5 | recall@k | MRR |
| --- | --- | --- | --- | --- |
| vector only | 0.50 | 0.70 | 0.75 | **0.581** |
| with rerank | 0.70 | 0.80 | 0.75 | **0.720** |

Per-case ranks went from `2 1 9 5 1 1 1 - - 1` to `1 1 1 5 1 1 1 - - 1`. Both misses are the scanned receipts. On the 9-document store «Начало» both modes score MRR 0.90, a ceiling effect. Killing the rerank `llama-server` in the middle of a rerank request returned vector order and logged `Reranking failed, using vector order … "fetch failed"`. The next search restarted the server and reranked again.

**Deviated from plan:**

- The eval run is synchronous over IPC rather than a kernel run, since ten cases take seconds. A large set would want a run with progress.
- `VectorSearchResultDto.rerank` is optional, not defaulted, so every existing caller and test keeps its shape when reranking is off.
- `LlamaServer` changes beyond the role, both found by the kill test: a rerank request no longer binds the server *start* to the caller's signal, so a cold-start timeout falls back while the model keeps loading. Any network failure or child exit now drops the server so the next call restarts it. Before this, a crashed server was reused until the idle timer fired, for every role.

**Next session should know:**

- Passages are capped at 2000 characters and the query at 1000. llama-server splits `--ctx-size` 8192 across its default parallel slots (≈2048 tokens each), so longer chunks from the TASK_028_EXTRA_6 chunker will need either a bigger rerank context or a smaller cap.
- Rerank on 80 candidates costs ≈2–3 s on CUDA. `candidates` is the main knob.
- The eval set compares modes on one store at one point in time. After a re-index, expected document ids survive only if the document row survives; removed ones show as «удалён» and count as misses.
