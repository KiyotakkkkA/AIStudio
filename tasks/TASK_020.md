---
task: TASK_020
title: Vector store schema, repository and service
phase: 3 — Rust core and vector search
status: done
depends_on: [TASK_019]
unblocks: [TASK_021]
estimate: 3 h
---

# TASK_020 — Vector store schema, repository and service

## Context

Rust can store and search vectors. Nothing in the app knows which stores exist. This task adds the SQLite side — the vector store aggregate — and the service that keeps the two halves consistent.

A vector store row carries what `/design/VectorStores.dc.html` shows: name, description, backend, embedding provider and model, chunk size and overlap, index type and metric, plus derived counts. The **vectors** live in LanceDB under a directory named by the store id; the **row** is the metadata and the source of truth for the list, the health dot and the stat strip.

The interesting work is keeping the two consistent. A store row can exist with no LanceDB table yet (just created), a table can exist with a row that says it has more documents than it does (a crashed index run), and deleting a store must remove both. Model these states honestly rather than assuming they cannot happen — the Vector Stores page already has a "Re-index pending" amber state in the mockup, which is exactly this.

## Why

TASK_021's page and TASK_028's indexing both need an aggregate to hang off, and retrieval in chat needs a way to name a store rather than a filesystem path.

## Depends on

- `TASK_019` — LanceDB in zvs-core — create, upsert, search.

## Read first

- `/design/VectorStores.dc.html` — the store list, the stat strip and the health states
- `/docs/PASSPORT.md` §9 (aggregates), §8 (repository rules), §2 D2

## Scope

**In scope**
- `vector_store` and `vector_document` tables plus migration
- `VectorStoreRepository`, `VectorDocumentRepository`
- `VectorStoreService`: CRUD, stats reconciliation, search passthrough, deletion of both halves
- Health derivation: healthy / stale / pending / broken
- DTOs and channels
- Tests with a fake `RustCore`

**Out of scope** (do not do these, even if tempting)
- Ingestion and indexing (TASK_028) — a store can be created and searched, but not filled yet
- The UI (TASK_021)
- Multiple backends — `qdrant` and `pgvector` appear in the mockup; model the `backend` column for them but implement LanceDB only

## Steps

1. `vector_store` table: id, name, description, backend, `embeddingProviderId` fk, `embeddingModelId`, dimension, metric, chunkSize, chunkOverlap, indexType, status, `lastIndexedAt`, counts (documents, vectors, bytes), createdAt, updatedAt. Unique on name.
2. `vector_document` table: id, `storeId` fk cascade, sourcePath, contentHash, chunkCount, bytes, indexedAt. The content hash is what makes re-indexing skip unchanged files in TASK_028.
3. Migration generated, reviewed, committed.
4. Repositories with intention-revealing methods, per PASSPORT §8. Register `secret_usage` indirectly — a store depends on a provider, which depends on a secret, so the store's dependency is on the provider row.
5. `VectorStoreService`: `create` (write the row, then create the LanceDB table, in that order, and roll back the row if the table fails), `list`, `get`, `update`, `remove` (delete the table, then the row; a failed table delete must not orphan a row silently — log and surface it), `search(storeId, query)` which embeds the query through the store's embedding provider and calls into Rust.
6. `reconcile(storeId)` — compare the row's counts with LanceDB's actual stats and repair the row. Run it lazily when a store is opened, and expose it as an explicit action for the page's Re-index affordance.
7. Health derivation as a pure function: no table → `pending`; counts disagree → `stale`; table missing but row present → `broken`; else `healthy`.
8. Channels: `vectorStores.list|get|create|update|remove|search|reconcile`.
9. Tests with a fake `RustCore`: create rolls back the row when the table fails; delete removes both; reconcile repairs drifted counts; search embeds through the right provider; health derivation covers all four states.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Create a store through a channel call, confirm both the SQLite row and the LanceDB directory appear. Delete the directory by hand and confirm health reports `broken` rather than crashing.

## Definition of done

- [x] Aggregate and migration in place
- [x] Create and delete keep SQLite and LanceDB consistent, with rollback on partial failure
- [x] Reconcile repairs drifted counts
- [x] Health derivation covers pending, stale, broken, healthy
- [x] Search works end to end against a manually seeded table
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Generated and reviewed migration 0006_exotic_jane_foster adds vector_store and vector_document, a unique store name, provider dependency with restrictive deletion, and cascading documents with unique store/source paths. Repositories are registered in the unit of work; document upsert preserves identity and supports content-hash lookup. VectorStoreService implements create/list/get/update/remove/search/reconcile, per-store operation exclusion, rollback of partial creation, and logged deletion failures that retain metadata. Seven validated vectorStores channels are wired into the host. Rust stats now return distinct document counts alongside vector counts and bytes; whole-directory removal goes through the addon. No code comments added.

**Deviated from plan:** Added tableCreatedAt to distinguish an uncreated pending store from a previously created but missing broken store, including empty tables. If both creation and native cleanup fail, metadata remains broken so removal can be retried instead of leaving untracked storage. Backend, embedding model/provider, dimension, metric and index type are fixed at creation; chunk settings can change only before indexing, while name and description remain editable. LanceDB reports FLAT honestly. Reconciliation repairs aggregate counts from native data but does not perform ingestion, reconstruct document metadata, or invent lastIndexedAt. The manual check is an automated channel/service smoke test using a real addon and deterministic embedding driver; no external provider request is needed.

**Next session should know:** Verified pnpm --filter studio test (269 passed, 2 existing skips), shared/IPC package tests (46 passed), cargo test -p zvs-core --locked (14 passed), pnpm typecheck, pnpm lint, pnpm build, cargo fmt and whitespace checks. The native smoke test creates a store, seeds multiple chunks/documents, searches, reconciles, removes its directory, observes broken health and deletes metadata; a second store verifies normal deletion of both halves. The backup test now drops both new tables and closes its database even if migration fails. TASK_021 can use vectorStores channels directly; search takes storeId/query/k/minScore and embeddingModelId is the provider's external model name. TASK_028 should coordinate native writes with the service's per-store exclusion, record indexed documents and set lastIndexedAt only after indexing; distinct document stats scan the document_id column, so avoid frequent list polling on large corpora. Rebuild the addon when using the new required documentCount field. Task files remain under the existing private ignore rule.
