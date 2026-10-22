---
task: TASK_021
title: Vector Stores page — list, detail, test search
phase: 3 — Rust core and vector search
status: done
depends_on: [TASK_020]
unblocks: [TASK_022]
estimate: 4 h
---

# TASK_021 — Vector Stores page — list, detail, test search

## Context

This task builds the Vector Stores page and closes Phase 3. After it, retrieval is visible and testable by hand, which makes every later retrieval bug far easier to diagnose.

The spec is `/design/VectorStores.dc.html`: a 262 px store list on the left; a detail header carrying the name, description and the manage buttons (Edit, Re-index, Export, Delete); a read-only stat strip with six cells; then a tabbed card — Overview, Documents, **Test search**, Settings — with Test search as the active tab in the mockup.

Test search is the reason this page earns its place. Query, top-k, minimum score, a Run search button, then result rows with a score, a score bar, a source path with chunk position, and the chunk text. It is a debugging tool for the retrieval quality of a store, and it is the same call chat will make in TASK_024.

## Why

Without a way to look at what a store actually returns, retrieval quality problems in chat are invisible and get misattributed to the model.

## Depends on

- `TASK_020` — Vector store schema, repository and service.

## Read first

- `/design/VectorStores.dc.html` — the whole page, especially the stat strip proportions and the result row layout
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/vector-stores/`: store, form view-model, organisms
- Store list with health dots and backend chips
- Detail header with manage buttons; read-only stat strip
- Test search tab, fully working against the real service
- Overview and Settings tabs; Documents tab as an honest stub until TASK_028 fills it
- Create and edit flows, including the embedding-provider picker

**Out of scope** (do not do these, even if tempting)
- Ingestion, file adding, re-indexing progress (TASK_028) — Re-index calls reconcile for now
- Export (leave the button disabled, naming a later task)
- Qdrant and pgvector backends

## Steps

1. `VectorStoreStore` (MobX) and `VectorStoreFormVm`, following the pattern from TASK_012 and TASK_016.
2. Store list: name, health dot, backend chip, description line, and the mono `N vectors · D dim` line from the mockup, plus the dashed 'Connect a store' row.
3. Detail header: 38 px icon tile, name, health chip, description, and the four manage buttons. Delete is destructive-styled and confirms, naming anything that depends on the store.
4. Stat strip: six cells divided by 1 px rules — documents, vectors, dimensions, index/metric, on-disk size, embedding model. Values come from the service's reconciled stats, not from a guess.
5. Test search tab: query field, top-k, min score, Run search. Show the timing chips from the mockup (`5 hits · 63 ms`, embedding vs search split) — the service already measures both, so surface them rather than a single number.
6. Result rows: score in mono accent, a score bar beneath it, the source path with chunk position, and the chunk text at 12 px. Rows below the score floor render dimmed, as drawn.
7. Create/edit form: name, description, embedding provider (a picker over providers filtered to the `embedding` capability from TASK_013), dimension, metric, chunk size and overlap. Dimension is required and visible — never silently defaulted.
8. States: empty (no stores), pending (created but not indexed), broken (table missing — offer reconcile), search in flight, zero results with a hint that the score floor may be too high.
9. Tests: form view-model validation; store actions against the fake bridge; search result mapping. No snapshot tests.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Create a store against a real embedding provider. Seed a few vectors with a scratch script, then run a test search from the page and confirm the scores and timings are real. Delete the LanceDB directory by hand and confirm the page shows `broken` and offers reconcile.

## Definition of done

- [x] Page matches the mockup at 1440×900
- [x] Store CRUD works against the real service
- [x] Test search returns real scored results with real timings
- [x] Stat strip reflects reconciled stats, not guesses
- [x] Broken and pending states handled honestly
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** MobX VectorStoreStore and VectorStoreFormVm, a 262 px searchable store list with health dots and backend chips, create/edit form with embedding-provider picker and explicit dimension validation, detail management actions, six reconciled stat cells, Overview/Documents/Test search/Settings tabs, scored chunk rows with source and position, and empty/pending/broken/error/in-flight states. Delete confirms the store name and document count; chat/scenario dependencies do not exist in the current schema. Re-index reconciles; Export remains disabled with TASK_028 attribution. No code comments added.
**Deviated from plan:** TASK_020 did not measure search timings. Added vectorStores.searchTimed and VectorSearchResultDto with host-measured embeddingMs/searchMs while preserving the existing array-returning vectorStores.search channel. Test search requests the top K with minScore 0, then applies the user floor for hit counts and dimming so below-floor chunks remain inspectable. Existing service restrictions remain visible: backend/provider/model/dimension/metric are immutable, chunk settings lock after indexing. Chunk totals are shown only when payload.chunkCount is present; missing text is explicit. The production Electron smoke test replaces the interactive dev check and uses a deterministic local HTTP embedding endpoint with the real provider driver, service, IPC and LanceDB. No local Ollama endpoint was available, so retrieval quality against a live model remains unverified.
**Next session should know:** Verified pnpm --filter studio test (277 passed, 2 existing skips), pnpm typecheck, pnpm lint, pnpm build, git diff --check, and pnpm exec playwright test e2e/vectorStores.spec.ts (passed). The Electron test creates through the form, seeds real native vectors, searches from the page, checks split timings and dimmed rows, edits, deletes its isolated LanceDB directory, verifies broken/reconcile UI and confirms deletion. Screenshot inspected at 1440x900: test-results/vectorStores-vector-stores-c1da3--loss-and-confirms-deletion/vector-stores-1440x900.png. TASK_024 can consume searchTimed for diagnostics or the unchanged search channel for filtered retrieval. TASK_028 must add ingestion, document management and actual dependency reporting when those relationships exist; reconciliation cannot restore deleted data. Task files remain under the existing private ignore rule.

---

**Phase 3 is complete when this task is done.** Rust is in the build, LanceDB is embedded, and retrieval can be exercised by hand.
