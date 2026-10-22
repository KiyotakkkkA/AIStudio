---
task: TASK_069
title: Vector stores and graph in Rust — services, retrieval pipeline, evaluation, vectorStores.* and graphRelations.*
phase: M — Migration to Tauri
status: done
depends_on: [TASK_068]
unblocks: [TASK_070]
estimate: 5 h
---

# TASK_069 — Vector stores and graph in Rust — services, retrieval pipeline, evaluation, vectorStores.* and graphRelations.*

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

The retrieval stack is the most tuned code in the host. It is the product of TASK_019 –
TASK_021_EXTRA_6 and TASK_024_EXTRA_1, and its behaviour is pinned by an evaluation set and by
tests. The services involved:
- `VectorStoreService.ts` (533 lines): create, update, remove, list, get, reset or clear,
  reconcile, documents, chunks, sources, search, and timed search. Hybrid search with rank
  fusion, rerank, neighbour expansion, graph augmentation and the relevance floor are wired
  here.
- `GraphService.ts` (279 lines): the relation catalogue, edges, hydrate-on-first-use,
  reconcile, neighbours, shortest path and k-hop.
- `VectorEvalService.ts`: the evaluation set and runs.
- `rerank.ts`, `retrievalMerge.ts`, `retrievalMetrics.ts`, `retrievalQuery.ts` (the follow-up
  join heuristic, decided in TASK_024_EXTRA_1) and `vectorHealth.ts`.

The repositories are `VectorStore`, `VectorSource`, `VectorDocument`, `VectorEvalCase`,
`GraphEdge` and `GraphRelation`. The channels are `vectorStores.*` (including `documents.*`,
`sources.*` and `eval.*`) and `graphRelations.*`. `vectorStores.sources.pick` opens a native
folder or file picker. Under Tauri that is `tauri-plugin-dialog`, behind a `PickerPort` that
`zvs-studio` implements.

Indexing (`vectorStores.index`) is ported in TASK_072. Until then the channel answers
`NOT_IMPLEMENTED`.

## Why

Search quality is the hardest thing to notice regressing. Porting this stack with its
evaluation set and its numeric tests, and comparing the numbers before and after, is what
proves the Rust version retrieves what the TS version did.

## Depends on

- `TASK_068` — in-process `VectorCore` and `GraphCore`, and the device probe.

## Read first

- `/docs/PASSPORT.md` §10
- `apps/studio/src/host/services/VectorStoreService.ts`, `GraphService.ts`,
  `VectorEvalService.ts`, `rerank.ts`, `retrievalMerge.ts`, `retrievalMetrics.ts`,
  `retrievalQuery.ts`, `vectorHealth.ts`
- `apps/studio/src/host/ipc/vectorStores.ts`, `graphRelations.ts`;
  `packages/shared/src/vectorStores/`, `graph/`
- Handoffs of TASK_021_EXTRA_5, TASK_021_EXTRA_6 and TASK_024_EXTRA_1, which hold the
  recorded evaluation numbers
- `apps/studio/tests/vectorStoreService.test.ts`, `vectorEval.test.ts`, `vectorRerank.test.ts`,
  `graphService.test.ts`, `retrieval.test.ts`, `retrievalMetrics.test.ts`,
  `llamaRerank.test.ts`

## Scope

**In scope**
- The six repositories, `VectorStoreService`, `GraphService` and `VectorEvalService`
- The pure retrieval modules: merge (RRF), metrics, query join and health
- Rerank through a `Reranker` port. Local rerankers attach when the runtime service exists
  (TASK_071). Until then rerank degrades exactly as the TS service does without a reranker.
- `PickerPort` with `tauri-plugin-dialog`, and `vectorStores.sources.pick`
- Every `vectorStores.*` and `graphRelations.*` channel except `vectorStores.index`, with DTOs
  and fixtures
- An evaluation parity check: run the TASK_021_EXTRA_5 evaluation set on the same fixture
  corpus in both hosts and record the recall and MRR from each in the handoff. They must
  match within floating-point noise.

**Out of scope** (do not do these, even if tempting)
- Indexing (TASK_072)
- Tuning anything. A number that changes is a port bug, not an improvement.

## Steps

1. Repositories on `temporary_database()`.
2. Pure retrieval modules with their table tests ported first. They pin the arithmetic.
3. `GraphService` with hydrate, reconcile and traversal tests.
4. `VectorStoreService` and `VectorEvalService`, with their suites ported against
   `InProcessCore` on temporary directories.
5. Picker port and channels.
6. The evaluation parity run. Record both hosts' numbers.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, create a store, add a source with the native picker, and
open the Relations tab. Test search returns the same top hits as the Electron build on the
same corpus (index it under Electron, copy nothing, and compare queries side by side). The
evaluation tab runs.

## Definition of done

- [x] Vector, graph and evaluation services and every repository are ported with their tests
- [x] Evaluation numbers match the TS host, recorded in the handoff
- [x] Every `vectorStores.*` (except `index`) and `graphRelations.*` channel works under Tauri
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract`: `vector_stores` and `graph` modules with every DTO the `vectorStores.*` and
  `graphRelations.*` channels carry (inputs with the Zod defaults behind accessors, outputs
  with their defaulted fields always written), the `VectorEvalCaseId` branded id,
  `below_relevance_floor`, `is_relation_name` and `validate::js_slice` (UTF-16 slicing, as
  `String.prototype.slice`). 41 golden fixture pairs under `packages/shared/fixtures/vectorStores`
  and `graph`, registered in both fixture tables; both suites accept and reject the same cases.
- Data: row structs and repositories for `vector_store`, `vector_source`, `vector_document`,
  `vector_eval_case`, `graph_edge` and `graph_relation_type`, each with tests on
  `temporary_database()` (upsert identity, cascades, the provider restriction, unique keys,
  ordering). Migration `0001_graph_relation_seed` restores the four starter relations.
- Pure modules with their TS table tests ported first: `services::rerank`,
  `retrieval_merge`, `retrieval_metrics`, `retrieval_query` and `vector_health`.
- `GraphService` (hydrate once per store, retried after a failure; reconcile; neighbours,
  shortest path, k-hop; row rollback on a failed engine write; `graph_stale` on a failed
  removal), `VectorStoreService` (create/rollback, list/get/reconcile, update with the
  frozen-settings rules, remove, clear, recreate, search and timed search with the embedding
  outside the lease, rerank with every fallback, chunks, related chunks, leases),
  `VectorSourceService` (sources and documents, the part of `IndexingService.ts` the channels
  need) and `VectorEvalService` (the `EvalSearch` port, modes, per-case failures, `last_eval`).
  The suites of `vectorStoreService.test.ts`, `vectorRerank.test.ts`, `vectorEval.test.ts` and
  `graphService.test.ts` are ported against `InProcessCore` on temporary directories, through
  probe wrappers that inject the failures the TS fakes injected.
- `platform::picker::PickerPort`; `zvs-studio` implements it with `tauri-plugin-dialog`
  `=2.8.3` (`DialogPicker`: one folder, or several files, parented to the studio window).
- `ipc::vector_stores` and `ipc::graph_relations`: every channel of both families except
  `vectorStores.index`, which still answers `NOT_IMPLEMENTED`. `Services` gains
  `vector_stores`, `vector_sources`, `vector_eval`, `graph` and `picker`; `compose` shares one
  `ProviderRegistry` between providers and vector stores and one `InProcessCore` between
  system, vector stores and the graph. `cargo test --workspace`, workspace clippy with
  `-D warnings`, `pnpm typecheck`, `pnpm lint`, `pnpm build` and the full Vitest suite
  (1392 tests) pass.

**Evaluation parity:** the TASK_021_EXTRA_5 set itself was ten questions over a copy of a
real user store that lives outside the repo, so it cannot run here. In its place,
`test/fixtures/retrieval/eval-corpus.json` is a fixed corpus (12 documents, 24 chunks with
8-dimensional vectors, Russian text with case numbers and abbreviations) and 12 questions
with their query vectors, plus a deterministic token-overlap reranker written identically in
both hosts. `apps/studio/tests/vectorEvalParity.test.ts` runs it through `VectorStoreService.ts`
and `VectorEvalService.ts` on the native addon; `services::vector_eval::parity` runs it
through the Rust services on `InProcessCore`. Each host was run before anything was pinned,
and both produced the same numbers, to the last digit and with identical per-case ranks.
Both tests now assert the pinned block, k = 10:

| | hit@1 | hit@5 | recall@k | MRR | per-case ranks |
| --- | --- | --- | --- | --- | --- |
| TS vector | 0.6667 | 1.00 | 1.00 | **0.8333** | `1 1 2 1 2 2 1 1 2 1 1 1` |
| Rust vector | 0.6667 | 1.00 | 1.00 | **0.8333** | `1 1 2 1 2 2 1 1 2 1 1 1` |
| TS hybrid | 1.00 | 1.00 | 1.00 | **1.0000** | `1 1 1 1 1 1 1 1 1 1 1 1` |
| Rust hybrid | 1.00 | 1.00 | 1.00 | **1.0000** | `1 1 1 1 1 1 1 1 1 1 1 1` |
| TS hybrid + rerank | 0.9167 | 1.00 | 1.00 | **0.9583** | `1 1 1 1 1 1 2 1 1 1 1 1` |
| Rust hybrid + rerank | 0.9167 | 1.00 | 1.00 | **0.9583** | `1 1 1 1 1 1 2 1 1 1 1 1` |

**Deviated from plan:**

- The parity run uses the fixture corpus above instead of the TASK_021_EXTRA_5 questions (see
  above). Rerank in it is the token-overlap stand-in, since no local reranker exists in Rust
  until TASK_071.
- The relation seed is a new migration `0001` rather than part of the frozen baseline,
  because the baseline was squashed from schema snapshots and dropped Drizzle `0015`'s
  `INSERT`. Before this, a Tauri database had an empty relation catalogue.
- Sources and documents live in `VectorSourceService`, not in an indexing service, since
  indexing is TASK_072. TASK_072 should build `IndexingService` on top of it (or absorb it)
  and reuse `VectorStoreService::lease`, `store_path`, `core` and `recreate_table`.
- `with_exclusive_store(id, work)` became `lease(id)`, which returns the row and a guard that
  releases the store on drop.
- Output DTOs with Zod defaults always serialise the defaulted fields, so their fixtures
  spell them out; the round-trip rule for minimal cases applies to inputs only (contract
  README).
- `llamaRerank.test.ts` belongs to `LlamaServer` and moves with TASK_071; only its logit
  normalisation case is ported here (`normalise_rerank_score`).
- The rerank tie-break compares ids bytewise, where the TS host used `localeCompare`. The two
  agree on the `<uuid>:<n>` ids indexing writes; they could differ for mixed-case or
  punctuation-heavy ids.

**Next session should know:**

- The manual check (`pnpm dev:tauri`: create a store, add a source with the native picker,
  open the Relations tab, compare Test search with the Electron build on one corpus, run the
  evaluation tab) has **not** been done. Under Tauri there is no indexing until TASK_072, so a
  store only gets vectors by indexing it under Electron first, which the task's comparison
  step expects anyway.
- `GraphService` keeps an in-memory graph per store id. Removing a store does not `forget` it,
  as in the TS host, so a store id is never reused in practice but the registry entry stays
  until restart.
- `VectorStoreService` has no shutdown task; the TS host had none either.
- Re-run both parity tests after any change to `zvs-core` search, fusion, the eval pipeline or
  the fixture; regenerating the fixture means running each host unpinned first and pinning only
  numbers both agree on.
