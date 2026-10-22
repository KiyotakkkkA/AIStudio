---
task: TASK_021_EXTRA_3
title: Graph relation catalog, repository and service
phase: 3 — Rust core and vector search (extension)
status: done
depends_on: [TASK_021_EXTRA_2, TASK_020]
unblocks: [TASK_021_EXTRA_4]
estimate: 3 h
---

# TASK_021_EXTRA_3 — Graph relation catalog, repository and service

## Context

The graph engine can store edges; nothing in the app tracks *what kinds* of relations exist, which store they belong to, or whether the graph agrees with what SQLite thinks documents exist. This is the same problem TASK_020 solved for vectors — two stores of truth that can drift — applied to the graph.

A vector store (TASK_020) can now optionally have a graph attached to it. Not every store needs one: a store used for simple similarity search never touches this. The relation catalog and reconciliation only matter once someone starts drawing edges.

## Why

TASK_021_EXTRA_4's UI needs an aggregate to read from and a place to record user-drawn links. Retrieval that wants to walk the graph (future chat work) needs a way to check a store's graph is actually consistent before trusting a traversal result.

## Depends on

- `TASK_021_EXTRA_2` — the typed `RustCore.graph` wrapper this service calls into.
- `TASK_020` — `vector_store` / `vector_document`, whose ids this task's edges reference.

## Read first

- `TASK_020.md` — the create/rollback, delete-both-halves, and reconcile/health pattern this task repeats on the graph side
- `/docs/PASSPORT.md` §9 (aggregates), §8 (repository rules)

## Scope

**In scope**
- `graph_relation_type` table: a small open catalog (id, name, description, directed boolean) seeded with `cites`, `derived_from`, `co_occurs`, `user_linked` — extendable without a migration for new rows, only for new columns
- `graph_edge` table: id, `storeId` fk, `fromDocumentId`, `toDocumentId`, `relationTypeId` fk, weight, metadata JSON, createdAt — this is SQLite's *record* of an edge; the graph engine (via TASK_021_EXTRA_2) holds the traversable structure. Same split as vectors: SQLite is the ledger, Rust is where you actually query it fast.
- Migration generated, reviewed, committed
- `GraphRelationRepository`, `GraphEdgeRepository`
- `GraphService`: `addEdge` (write the row, then write to the graph engine, roll back the row on engine failure — mirrors TASK_020 step 5's create ordering), `removeEdge` (both halves), `neighbors`/`shortestPath`/`kHopSubgraph` passthrough to the engine, `reconcile(storeId)` comparing SQLite edge count against the engine's reported edge count for that store
- Health contribution: a store's existing health (TASK_020) gains a `graphStale` flag when edge counts disagree — additive, does not replace the four states TASK_020 already defined
- Channels: `graphRelations.list|addEdge|removeEdge|neighbors|shortestPath|kHopSubgraph|reconcile`
- Tests with a fake `RustCore.graph`: rollback on engine failure, delete removes both halves, reconcile catches drift, neighbors/path calls map correctly

**Out of scope** (do not do these, even if tempting)
- Automatic relationship extraction from document content — edges are explicit (user-drawn or supplied by a caller) until a later indexing task decides to infer them
- Any UI (`TASK_021_EXTRA_4`)
- Cross-store edges — an edge's `fromDocumentId`/`toDocumentId` must belong to the same `storeId`; reject otherwise

## Steps

1. `graph_relation_type` table and seed migration with the four starter relations.
2. `graph_edge` table with the fk's above, unique on `(storeId, fromDocumentId, toDocumentId, relationTypeId)` to prevent duplicate edges.
3. Migration generated, reviewed, committed.
4. Repositories with intention-revealing methods per PASSPORT §8.
5. `GraphService.addEdge`: validate both documents belong to `storeId` (via `VectorDocumentRepository` from TASK_020), write the row, call `RustCore.graph.addEdge`, roll back the row if the engine call fails.
6. `GraphService.removeEdge`: remove from the engine, then the row; a failed engine removal must not silently orphan the row — log and surface it, same discipline as TASK_020 step 5's delete ordering.
7. `reconcile(storeId)`: compare SQLite's edge count for the store against `RustCore.graph` stats for that store's nodes; set/clear `graphStale` on the store's health.
8. Channels as listed in scope.
9. Tests as listed in scope.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: add a couple of edges between real documents in a store through a channel call, confirm both the SQLite rows and a `neighbors` call agree. Force a mismatch (delete an edge row by hand) and confirm reconcile flags `graphStale`.

## Definition of done

- [x] Relation catalog and edge table in place with migration
- [x] `addEdge`/`removeEdge` keep SQLite and the graph engine consistent, with rollback on partial failure
- [x] Cross-store edges rejected
- [x] Reconcile detects and flags drift without replacing TASK_020's existing health states
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Migration `0015_graph_relations` adds `graph_relation_type` (unique name, open
catalog seeded with `cites`, `derived_from`, `co_occurs`, `user_linked` at fixed ids),
`graph_edge` (store/document/relation foreign keys, weight, JSON metadata, unique on
`(storeId, fromDocumentId, toDocumentId, relationTypeId)`) and an additive `graph_stale`
column on `vector_store`. `GraphRelationRepository` and `GraphEdgeRepository` are registered
in the unit of work. `GraphService` implements `listRelations`, `addEdge`, `removeEdge`,
`neighbors`, `shortestPath`, `kHopSubgraph` and `reconcile(storeId)`; seven validated
`graphRelations.*` channels are wired into the host and the composition root. Two more napi
exports — `graphHydrate` and `graphStats` — plus the matching `RustCore.graph` methods carry
hydration and the engine edge count TASK_021_EXTRA_2 left for this task. No code comments
added.

**Deviated from plan:**

- Added `graphHydrate`/`graphStats` napi exports and `RustCore.graph.hydrate`/`stats`. The
  engine registry is in-memory, so without them `reconcile` would compare a populated ledger
  against an empty graph after every restart and flag every store stale. A store hydrates
  once per process, before its first traversal, as TASK_021_EXTRA_2's handoff asked.
- `graphStale` is a persisted column on `vector_store`, not a derived value, so the list can
  show it without re-running reconciliation. `deriveVectorHealth` and the four TASK_020
  states are untouched.
- `removeEdge` takes the natural key `(storeId, from, to, relationTypeId)` rather than a row
  id. It is the unique index, and it is reconstructible from a `neighbors`/`kHopSubgraph`
  result plus the catalog, so no extra edge-listing channel was needed. The scope's channel
  list is implemented exactly; `graphRelations.list` is the relation catalog.
- `reconcile` reports drift, it does not repair it. Either half could be the wrong one, so
  the ledger stays authoritative and the store is flagged.
- Self-loops are rejected, as is a second edge of the same relation between the same pair
  (`CONFLICT`, ahead of the unique index).
- A failed engine removal marks the store `graphStale` in addition to logging and surfacing
  `NATIVE_ERROR`, since the row and the engine are knowingly out of step at that point.
- The manual check is automated as `apps/studio/tests/graphNative.test.ts`: it draws two
  edges through the channels against the real addon, reads them back with `neighbors` and
  `shortestPath`, then deletes an edge row by hand and confirms reconcile reports
  `graphStale`.

**Next session should know:**

- Verified `pnpm typecheck`, `pnpm lint`, `pnpm build`, `pnpm test` (568 passed),
  `node --test test/native/addon.test.mjs` (7 passed), `cargo test -p zvs-core` and
  `-p zvs-napi`, `cargo fmt --check` and Prettier on every touched file. The three renderer
  failures (`runHistoryStore`, `tasksPage`, `taskStore`) fail on `main` before these changes;
  `sidecar.test.ts` is timing-flaky and passes in isolation. Workspace-wide `cargo test`
  still cannot link the lance rlibs in this environment, as in TASK_020.
- `GraphService` keeps a per-store hydration promise. Anything that writes `graph_edge` rows
  outside the service must call `forget(storeId)` or the engine will keep the stale copy.
- Relation names are the engine's edge labels, so a new catalog row needs a slug-shaped
  `name` (`GRAPH_RELATION_NAME`); the description is free copy. New rows need no migration.
- `neighbors`/`shortestPath`/`kHopSubgraph` reject a document id that is not in the store
  before calling the engine; an id the engine does not know is `VALIDATION_FAILED` from the
  addon, which after hydration only happens for a document with no row.
- TASK_021_EXTRA_4 can read `graphRelations.list` for the catalog, draw with `addEdge`, and
  render from `kHopSubgraph`; `VectorStoreDto.graphStale` is the flag to show next to the
  existing health dot.
