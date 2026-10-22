---
task: TASK_021_EXTRA_1
title: Graph relationship store in zvs-core — embedded graph database
phase: 3 — Rust core and vector search (extension)
status: done
depends_on: [TASK_020]
unblocks: [TASK_021_EXTRA_2]
estimate: 4 h
---

# TASK_021_EXTRA_1 — Graph relationship store in zvs-core — embedded graph database

## Context

LanceDB (TASK_019) gives the app vector search: "what is similar to this chunk." It cannot answer "what is *connected* to this chunk" — citations between documents, shared entities across chunks, explicit user-drawn links between sources. That's a graph problem, and vector search degrades badly when asked to fake it with cosine distance alone.

This task adds a second embedded store to `zvs-core`, alongside LanceDB, following the same shape as PASSPORT decision D2 (LanceDB owns vectors, SQLite owns metadata): a **graph store owns relationships between documents and chunks**. It does not replace or duplicate `vector_store` / `vector_document` in SQLite (TASK_020) — those remain the source of truth for what exists. The graph store only holds edges between ids that already exist there.

## Why

TASK_021_EXTRA_3 (the SQLite aggregate) and TASK_021_EXTRA_4 (graph-augmented search in the UI/chat) both need something to sit on top of. Establishing the embedded engine, the node/edge model, and the crate boundary now — before any caller depends on it — means the same mistake TASK_017 was written to avoid (retrofitting standards onto existing code) doesn't repeat here.

## Depends on

- `TASK_020` — Vector store schema, repository and service. Document/chunk ids this task references as graph nodes come from `vector_document`.

## Read first

- `/docs/PASSPORT.md` §10 (the Rust boundary), §2 D2 (Rust never touches SQLite — this task extends that boundary, it doesn't cross it)
- `TASK_019.md` and `TASK_020.md` — the LanceDB/SQLite split this task mirrors on the graph side

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "Three ways to store the relationship graph, in order of how much they buy you: (a) **CozoDB**, an embeddable Rust graph/relational database with Datalog queries and real persistence — most capable, adds a real dependency and a query language to learn; (b) **petgraph** in memory, rehydrated from edge rows kept in SQLite — simplest, no new persistence engine, but every process start pays a rehydration cost and graph size is bounded by memory; (c) plain adjacency table in SQLite with hand-written traversal queries — no new dependency at all, but multi-hop queries get ugly fast. Which one?"
>
> **Why it matters:** This is a storage-engine choice, same weight class as picking LanceDB in TASK_019. Changing it later means a data migration, not a config flag.
> **Options:** (a) CozoDB; (b) petgraph + SQLite-backed edges; (c) plain SQLite adjacency table, no Rust graph engine at all.
> **If the user is unavailable:** take (b). It keeps the crate dependency surface small and defers commitment to a dedicated graph engine until real usage shows the memory bound is actually a problem — consistent with TASK_019 taking the concrete, mockup-matching default rather than the most powerful option.

## Scope

**In scope**
- `graph` module in `zvs-core`, following the chosen option above
- Node model: `{ id, kind }` where `kind` is `document | chunk` — ids are the same ids already used in `vector_document` / LanceDB rows, never invented here
- Edge model: `{ from, to, relation, weight?, metadata JSON }` — `relation` is a string (`cites`, `derived_from`, `co_occurs`, `user_linked`, …), open-ended, not an enum, so new relation kinds don't need a Rust change
- CRUD: add node, add edge, remove edge, remove all edges touching a node (for cascade on document delete)
- Traversal: neighbors(node, relation?, direction), shortest path between two nodes, k-hop subgraph around a node
- Respect the crate's existing cancellation and error conventions from TASK_017 — long traversals take a `CancellationToken`, errors use the crate's error enum with a stable `code()`
- Tests: add/remove nodes and edges, neighbor queries, k-hop subgraph correctness, cascade removal, an empty-graph and single-node edge case

**Out of scope** (do not do these, even if tempting)
- Extracting relationships from content (entity extraction, citation parsing) — that's an indexing-time concern for a later task, not this one
- Exposure through the napi addon (`TASK_021_EXTRA_2`)
- The SQLite-side relation catalog and reconciliation (`TASK_021_EXTRA_3`)
- Any UI (`TASK_021_EXTRA_4`)
- Graph visualization layout algorithms — this task returns data, not pixel positions

## Steps

1. Add the chosen dependency to `crates/zvs-core`'s `Cargo.toml` and `[workspace.dependencies]`, pinning the current stable version (same discipline as TASK_019 step 1).
2. Create the `graph` module (it currently exists only as an empty placeholder per TASK_017 step 2 — fill it in).
3. Define node and edge types and the store handle, opened from a path the host supplies via `paths.ts` — same rule as TASK_019 step 2: Rust never invents the storage location.
4. Implement add/remove for nodes and edges, batched where the chosen engine benefits from it.
5. Implement `neighbors`, `shortest_path`, and `k_hop_subgraph`, each honoring the cancellation token on anything that could be unbounded (a k-hop query on a densely connected graph is the obvious runaway case — cap it explicitly and return a clear error past the cap rather than hanging).
6. Implement cascade removal: deleting a document's node removes every edge touching it, in one call, so TASK_020's document delete has something to call into later.
7. Tests as listed in scope, using a temp directory (same pattern as TASK_019's tests).

## Verify

```bash
cargo test -p zvs-core
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Manual check: from a scratch script, add a handful of document and chunk nodes, wire some `cites` and `co_occurs` edges between them, run a 2-hop subgraph query from one node, and confirm the returned set is what you'd draw by hand on paper.

## Definition of done

- [x] Graph engine decision made (by the user, or the fallback taken) and recorded in the crate doc comment
- [x] Node/edge CRUD and traversal implemented and covered by tests
- [x] Cancellation and error conventions from TASK_017 honored
- [x] k-hop queries are bounded, not just theoretically bounded
- [x] Cascade removal implemented
- [x] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test -p zvs-core` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `graph` module in `zvs-core` on petgraph 0.8.3 (`StableDiGraph`, so ids
survive removals), pinned in `[workspace.dependencies]`. `Node { id, kind }` with a
`document | chunk` kind; `Edge { from, to, relation, weight?, metadata }` with
`relation` an open string. `GraphStore` offers `new`/`hydrate`, `add_node(s)`,
`add_edge(s)`, `remove_edge`, `remove_edges_touching`, `remove_node`,
`remove_nodes`, `nodes`/`edges`/`node`/`contains_node`/`stats`, and the three
traversals `neighbors`, `shortest_path`, `k_hop_subgraph`. Ten integration tests in
`crates/zvs-core/tests/graph.rs`. Documented in the crate README under
"Relationship graph", which is the crate's Rust doc comment.

**Deviated from plan:**

- The user chose option (b), petgraph plus SQLite-backed edges. Step 3's
  "opened from a path the host supplies" does not apply: with option (b) there is
  no Rust-side on-disk format, so the store is constructed in memory and the host
  persists rows. `GraphStore::hydrate(nodes, edges, token)` rebuilds it and
  `nodes()` / `edges()` hand the rows back, both sorted. Rust still never opens
  SQLite.
- Added `Error::LimitExceeded` for a traversal that passes its node budget. It maps
  to the existing `VALIDATION_FAILED` code, so `AppErrorCode.ts` is unchanged and
  stays append-only.
- Self-loops and edges with a nonexistent endpoint are rejected rather than
  silently created; `(from, to, relation)` upserts instead of creating parallel
  edges. Neither was specified.
- `cargo fmt --check` also reports three pre-existing diffs in
  `crates/zvs-core/src/pdf.rs`, present at HEAD and unrelated to this task — local
  rustfmt is 1.8.0-stable, which formats that file's multi-line string literals
  differently from whatever last formatted it. Left alone.
- `protoc` 36.1 was not installed locally; it is now at
  `target/tools/protoc/bin/protoc.exe`, the path the README already documents.

**Next session should know:**

- TASK_021_EXTRA_2 exposes this over napi. Every public type is serde
  camelCase-ready, and `Direction`, `NodeKind` serialize lowercase. The store is
  `&mut self` for writes and holds no interior mutability, so the addon needs its
  own lock around a long-lived instance.
- TASK_021_EXTRA_3 owns the SQLite side. The host is the persistence layer here:
  it must hydrate at startup and write back after mutations. Nothing in the crate
  does that for it.
- Bounds are `MAX_HOPS = 8`, `DEFAULT_NODE_BUDGET = 4096`, `MAX_NODE_BUDGET =
  65536`. A budget overrun is `Error::LimitExceeded`, a distinct failure from an
  empty result.
- `shortest_path` minimizes hops, not weight. If TASK_021_EXTRA_4 wants
  weight-aware ranking, that is a new method, not a change to this one.
- The manual check in Verify is `k_hop_subgraph_matches_a_hand_drawn_expansion`,
  which asserts the exact 2-hop node and edge sets of a seven-node sample graph.
