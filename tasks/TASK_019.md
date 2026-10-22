---
task: TASK_019
title: LanceDB in zvs-core — create, upsert, search
phase: 3 — Rust core and vector search
status: done
depends_on: [TASK_018]
unblocks: [TASK_020]
estimate: 4 h
---

# TASK_019 — LanceDB in zvs-core — create, upsert, search

## Context

The addon works. This task puts the real payload behind it: **LanceDB**, embedded in `zvs-core` as a Rust dependency, providing the vector storage and search that retrieval depends on.

The division of responsibility is set by PASSPORT decision D2 and it is easy to get wrong here. LanceDB owns the **vectors** — the tables live in their own directory on disk, managed entirely by Rust. SQLite owns the **metadata** — which stores exist, their names and descriptions, document counts, which embedding provider they use. Rust never opens SQLite. When indexing writes vectors (TASK_028), the metadata that results comes back across the boundary and the host persists it through a repository.

So this task is pure Rust plus a thin addon surface: open or create a table at a path, upsert rows carrying a vector and a payload, and search by vector with top-k and a score floor.

## Why

Retrieval is the feature that makes the whole product worth having — chat with citations, the vector store test-search page, the scenario vector node all sit on this.

## Depends on

- `TASK_018` — napi-rs addon — build, load, panic guard, first async call.

## Read first

- `/docs/PASSPORT.md` §10 (what lives in the addon), §2 D2 (Rust never touches SQLite)
- `/design/VectorStores.dc.html` — the stat strip names what a store must be able to report: documents, vectors, dimensions, index type, on-disk size

## Scope

**In scope**
- LanceDB wired into `zvs-core` with a small internal API
- Create/open a table with a declared dimension and metric
- Upsert rows: id, vector, payload JSON, source metadata
- Search: query vector, top-k, minimum score, optional filter
- Delete by id and by source document
- Stats: row count, dimension, on-disk size
- Exposure through the napi addon with tests on both sides

**Out of scope** (do not do these, even if tempting)
- Embedding generation — that is the AI driver's job (TASK_014); this layer takes vectors as input
- Indexing orchestration and progress (TASK_028)
- The vector store aggregate in SQLite (TASK_020)

## Steps

1. Add LanceDB to `zvs-core`. Resolve the current stable version at the time of writing and pin it.
2. `index` module: `VectorIndex` type wrapping a table handle, opened from a path under a directory the host supplies. Rust must never invent the storage location — the host passes it, resolved from `paths.ts`.
3. `create(path, dim, metric)` — cosine by default, matching the design's stat strip. Fail clearly if a table exists with a different dimension; a silent mismatch produces nonsense results that are very hard to trace back.
4. `upsert(rows)` — batched, taking id, vector, payload JSON and source fields (document id, chunk index, path). Respect the cancellation token between batches.
5. `search(query_vec, k, min_score, filter?)` — returns id, score and payload. Score normalisation must be documented in a comment: the numbers reach the UI as the score bars on the Vector Stores page, so what 0.91 means has to be stable.
6. `delete_by_source(document_id)` and `delete_by_ids(ids)` — re-indexing a changed document must not leave orphans.
7. `stats()` — row count, dimension, on-disk bytes, index type.
8. Expose all of it through `zvs-napi` as async functions, and extend `RustCore.ts` with the typed wrapper.
9. Tests in Rust: round-trip upsert and search returns the nearest neighbour; dimension mismatch is rejected; delete-by-source removes exactly the right rows; cancellation stops a large upsert partway; stats are accurate. Tests use a temp directory.

## ❓ DECISION — ask the user before implementing this part

Confirmed by the user: cosine, 1024 dimensions. Dimension remains explicit in the creation API.

> **Question to ask, verbatim:**
> "What embedding dimension and distance metric should vector stores default to? Cosine at 1024 dimensions matches the mockup and mxbai-embed-large, but if you plan to use a different embedding model day to day, the default should match it so stores do not need rebuilding later."
>
> **Why it matters:** Dimension is fixed at table creation — changing it means re-indexing the whole corpus. The default shapes every store created before someone thinks to change it.
> **Options:** (a) cosine, 1024, matching the mockup's mxbai-embed-large; (b) cosine, 768, matching smaller/faster embedding models; (c) no default — force an explicit choice when creating a store.
> **If the user is unavailable:** take (a), and make dimension a required, visible field when creating a store so the default is never silently wrong.

## Verify

```bash
cargo test -p zvs-core
pnpm build:rust
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: From a scratch script, create a table, upsert a few hundred random vectors, search, and confirm nearest neighbours look right. Check the on-disk directory appears where the host said it should.

## Definition of done

- [x] LanceDB create, upsert, search, delete and stats implemented in `zvs-core`
- [x] Dimension mismatch rejected loudly
- [x] Cancellation honoured during long upserts
- [x] Score semantics documented
- [x] Exposed through the addon with a typed wrapper
- [x] Rust and TypeScript tests pass; gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** LanceDB 0.38.0 pinned in the Cargo workspace. VectorIndex creates/opens a host-supplied absolute directory, persists and checks dimension/metric in the schema, validates rows, merge-upserts in batches of 256, searches with top-k, inclusive score floor and an optional SQL prefilter, deletes by IDs/source, and reports live rows, dimension, metric, physical table bytes and FLAT index type. JSON payloads and document/chunk/path source metadata round-trip. Async napi JSON dispatch catches panics across future polls; operation tokens bridge AbortSignal cancellation and are released on success/failure. RustCore implements the typed VectorCorePort, validates native responses and maps errors. paths.ts owns vector store directories. User confirmed cosine/1024; creation still requires an explicit dimension. No code comments added.

**Deviated from plan:** Score formulas are documented in crates/zvs-core/README.md instead of a code comment, per the user's instruction. The raw addon uses one serde-validated JSON operation dispatcher plus async cancellation lifecycle functions; the host exposes separately typed methods. Searches use exact FLAT scans; no HNSW index is created or claimed. LanceDB 0.38.0 fails to compile with default features because job.rs references feature-gated Error::Http, so its remote feature is enabled solely as an upstream compilation workaround; our API accepts absolute local paths. Lance requires protoc: official 36.1 was installed under ignored target/tools/protoc, the addon build script recognises that location, Turbo passes PROTOC through, and CI provisions protoc for all native jobs.

**Next session should know:** Verified on Windows MSVC: cargo test -p zvs-core (14 passed), cargo test --locked --all-features (16 passed), cargo clippy --locked --all-targets --all-features -- -D warnings, cargo fmt --check, pnpm build:rust, pnpm build:rust:debug, node --test test/native/addon.test.mjs (4 passed), pnpm --filter studio test (258 passed, 2 existing skips), pnpm typecheck, pnpm lint, pnpm build, and changed-file formatting/whitespace checks. The native test creates 400 seeded random vectors, verifies self-nearest search, replacement, filtering, deletion, cancellation, persistence and the host-selected disk path. Rust tests verify exact score semantics for cosine/L2/dot, schema mismatch, physical bytes and cancellation after a committed batch. Upserts are atomic per batch, not per request; cancellation retains prior commits and retries replace IDs. Direct Cargo commands need protoc on PATH or PROTOC set (PowerShell: $env:PROTOC = (Resolve-Path target/tools/protoc/bin/protoc.exe).Path). TASK_020 owns SQLite aggregates and the visible dimension choice; TASK_028 owns orchestration. Filters use host-controlled Lance SQL with snake_case source fields. Other platforms and hosted CI were not executed locally. Task files remain under the existing ignore rule.
