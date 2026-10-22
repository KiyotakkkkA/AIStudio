---
task: TASK_021_EXTRA_4
title: Relations tab on the Vector Stores page + graph-augmented test search
phase: 3 — Rust core and vector search (extension)
status: done
depends_on: [TASK_021_EXTRA_3, TASK_021]
unblocks: []
estimate: 4 h
---

# TASK_021_EXTRA_4 — Relations tab on the Vector Stores page + graph-augmented test search

## Context

Every layer below this one now exists: the graph engine (TASK_021_EXTRA_1), the addon bridge (TASK_021_EXTRA_2), and the SQLite-side service (TASK_021_EXTRA_3). This task makes it visible and usable from the page TASK_021 already built, and lets Test search optionally widen its results using the graph — the concrete payoff for "increase data fetching quality" that started this whole extension.

Two additions to the existing Vector Stores page:

1. A **Relations** tab alongside Overview / Documents / Test search / Settings, where a user can see and draw edges between documents in a store.
2. A **"Expand via graph" toggle** on Test search: when on, after the normal vector search runs, results are widened by walking `co_occurs`/`cites`/`user_linked` edges one hop out from the top hits, merged in below the vector-ranked results with a visual marker distinguishing "found by vector search" from "found by graph expansion."

## Why

Vector similarity alone misses relevant chunks that are related but not textually similar — a definitions section and the paragraph that uses the term, two documents that cite each other, a note the user explicitly linked to a source. This is the retrieval-quality improvement the graph store exists to deliver; without this task the graph is built but nothing in the product benefits from it.

## Depends on

- `TASK_021_EXTRA_3` — Graph relation catalog, repository and service.
- `TASK_021` — the Vector Stores page this task extends; reuses `VectorStoreStore` and the existing tab structure.

## Read first

- `/design/VectorStores.dc.html` — the tab structure and Test search result row layout this task extends rather than replaces
- `TASK_021.md` — the store/view-model pattern and the result-row visual language to stay consistent with

## Scope

**In scope**
- `GraphRelationStore` (MobX), following the pattern from TASK_021's `VectorStoreStore`
- Relations tab: a simple node/edge list view (not a force-directed canvas — that's a nice-to-have, not this task) showing each document's outgoing and incoming edges, relation type, and a way to add/remove a `user_linked` edge between two documents picked from the store's document list
- "Expand via graph" toggle on the existing Test search tab, off by default
- When on: after vector results return, call `graphRelations.neighbors` one hop out from each top-k hit; merge in any documents not already present, capped at a configurable max additional results (default 5), each tagged "via graph" instead of a score
- Result rows for graph-expanded hits reuse the existing row layout from TASK_021 but replace the score bar with a small relation-type chip (`cites`, `co_occurs`, etc.) so the two provenances are never visually confused
- Empty/edge states: a store with no graph edges shows the Relations tab as an honest empty state ("No relations yet — add one, or run indexing that infers them") rather than hiding the tab
- Tests: view-model logic for adding/removing edges; merge logic for graph-expanded results (no duplicates, cap respected, correct tagging); no snapshot tests, consistent with TASK_021

**Out of scope** (do not do these, even if tempting)
- Force-directed or any visual graph canvas — list view only
- Automatic relationship inference during indexing — edges here are still explicit, same boundary TASK_021_EXTRA_3 drew
- Wiring graph expansion into the actual chat retrieval path — this task proves it on the Test search debugging tool only; chat integration is a separate task once this is validated by hand
- Multi-hop expansion beyond one hop — start narrow, widen later if it earns its keep

## Steps

1. `GraphRelationStore`: list edges for a store, add edge, remove edge, backed by the `graphRelations.*` channels from TASK_021_EXTRA_3.
2. Relations tab UI: document picker (reuse the document list from TASK_021's Documents tab data source), relation-type dropdown seeded from the catalog, edge list grouped by document.
3. Test search toggle: add "Expand via graph" checkbox near the existing top-k/min-score fields.
4. Expansion logic in the search flow: run the existing vector search unchanged, then if the toggle is on, call `neighbors` for each of the top-k result documents, dedupe against existing results, cap at the configured max, tag each with its relation type.
5. Result row variant: score bar swapped for a relation-type chip when a row's provenance is "via graph"; keep everything else (source path, chunk position, chunk text at 12 px) identical to TASK_021's layout.
6. Empty/edge states as listed in scope.
7. Tests as listed in scope.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in a store with a few documents, draw a `user_linked` edge between two that aren't textually similar. Run Test search with a query that only matches one of them, toggle "Expand via graph" on, and confirm the linked document now appears, tagged as graph-expanded, not scored.

## Definition of done

- [x] Relations tab matches the page's existing visual language and lets a user add/remove edges
- [x] Test search toggle works end to end against the real service
- [x] Graph-expanded results are visually distinct from vector-ranked results, never conflated
- [x] One-hop cap and dedupe behave correctly
- [x] Empty/broken graph states handled honestly
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass — TS typecheck, lint and the studio build pass; `//#build:rust` is blocked by a running Electron instance holding `zvs-core.node` (see handoff), not by this change
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `graphRelations.listEdges` (contract + `GraphService.listEdges` + handler) — TASK_021_EXTRA_3 exposed relation *types* and graph queries, but nothing could read a store's edge rows, which the Relations tab needs.
- `GraphRelationStore` (MobX, `apps/studio/src/renderer/features/vector-stores/GraphRelationStore.ts`): relation catalog, edge list, document/relation pickers, add/remove edge, one-hop expansion. Owned by `VectorStoreStore` as `store.graph`, so the Relations tab and Test search share one source of truth and the page keeps a single store in `RootStore`.
- `graphExpansion.ts`: `mergeGraphNeighbors` (pure merge — skips ranked documents, first edge to a document wins, hard cap, document nodes only) and `graphRows` (row mapping).
- Relations tab (`VectorRelationsPanel.tsx`), fourth in the switcher between Documents and Test search: from/type/to pickers, edges grouped by document with outgoing and incoming lines, delete behind the page's usual confirm modal, `graphStale` chip, and two distinct empty states (fewer than two documents vs. no edges yet).
- Test search: «Расширить через граф» toggle (off by default) plus a «Макс. по графу» cap field (default 5). Graph rows render below the vector rows with a dashed border and a relation-type chip where the score bar sits.
- Tests: `apps/studio/tests/renderer/graphRelations.test.ts` (7) and one added to `apps/studio/tests/graphService.test.ts`.

**Deviated from plan:**

- Graph-expanded rows cannot show chunk text: `graphRelations.neighbors` returns document nodes and no channel exposes a document's chunks, so the row shows the source path, the store's chunk count in place of the chunk position, and a line saying the document came in by relation rather than by ranking. Everything else in the row layout is TASK_021's.
- Added `graphRelations.listEdges` rather than deriving the edge list from `kHopSubgraph`; the subgraph walk answers from the engine, and the tab should show what the ledger actually holds.

**Next session should know:**

- Chat retrieval still does not use the graph — that was out of scope here and is the next step now the toggle proves the shape end to end.
- `graphExpansion.mergeGraphNeighbors` is deliberately one hop and provenance-tagged; multi-hop would need a different merge order (nearest hop first) than the current per-seed order.
- `pnpm build` currently stops at `//#build:rust` with EBUSY on `zvs-core.node` because a running Electron instance holds the addon; `pnpm --filter studio build`, `pnpm --filter studio typecheck`, lint and the full vitest run all pass.

---

**This extension is complete when this task is done.** The graph store exists, is queryable end to end, is visible on the Vector Stores page, and demonstrably improves Test search results on at least one hand-verified example.
