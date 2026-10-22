---
task: TASK_021_EXTRA_2
title: Expose graph store through the napi addon
phase: 3 — Rust core and vector search (extension)
status: done
depends_on: [TASK_021_EXTRA_1, TASK_018]
unblocks: [TASK_021_EXTRA_3]
estimate: 2 h
---

# TASK_021_EXTRA_2 — Expose graph store through the napi addon

## Context

`zvs-core` can now store and traverse a relationship graph, but nothing on the Node side can reach it — same gap TASK_018 closed for chunking and hashing. This task extends `crates/zvs-napi` with graph exports and gives the host a typed wrapper, so no service ever imports the raw addon to talk to the graph.

This is deliberately a thin, mechanical task. All the real design decisions (which engine, what the node/edge model looks like) were made in TASK_021_EXTRA_1. Re-litigating them here would be the same mistake TASK_018 was written to avoid — the panic guard and async discipline apply uniformly, not just to the first function that used them.

## Why

TASK_021_EXTRA_3's service and TASK_021_EXTRA_4's UI both need a call path into the graph store. Without a typed wrapper, they'd each invent their own ad hoc bridge, and the panic-safety guarantee TASK_018 established for `chunk_text`/`hash_bytes` would quietly not apply to graph calls.

## Depends on

- `TASK_021_EXTRA_1` — Graph relationship store in zvs-core.
- `TASK_018` — napi-rs addon, panic guard, error conversion, and the `RustCore.ts` pattern this task extends.

## Read first

- `/docs/PASSPORT.md` §10 (addon rules — async exports, panic guard, plain-JSON payloads)
- `TASK_018.md` — the exact pattern being repeated here

## Scope

**In scope**
- `#[napi]` async exports for: add node, add edge, remove edge, remove node (cascade), neighbors, shortest path, k-hop subgraph
- Every export wrapped in the existing panic guard from TASK_018 — no new guard, reuse it
- Error conversion reuses `From<CoreError> for napi::Error` from TASK_018; graph-specific errors (e.g. "path exceeds hop cap") get their own stable code appended to the existing scheme
- Extend `RustCore.ts` with a `graph` namespace of typed methods mirroring the exports
- Tests: each export round-trips through the addon; a deliberate panic in a graph call surfaces as an error, not a crash (reusing TASK_018's panic test pattern)

**Out of scope** (do not do these, even if tempting)
- Any new panic-guard or error-conversion mechanism — reuse TASK_018's exactly
- The SQLite relation catalog (`TASK_021_EXTRA_3`)
- UI or chat integration (`TASK_021_EXTRA_4`)

## Steps

1. Add the graph functions to `crates/zvs-napi`, one `#[napi]` async export per `zvs-core` graph function, each going through the panic-guard wrapper from TASK_018 step 3.
2. Confirm payloads are plain JSON per PASSPORT §10 — node ids and relation strings serialize trivially; make sure edge metadata JSON round-trips without double-encoding.
3. Extend `host/drivers/rust/RustCore.ts` with a `graph` namespace: `addNode`, `addEdge`, `removeEdge`, `removeNode`, `neighbors`, `shortestPath`, `kHopSubgraph` — same naming discipline as the existing `chunk`/`hash` methods.
4. Tests: one per exported method proving the round trip; reuse the deliberate-panic test pattern from TASK_018 against one graph export to confirm the guard covers new code, not just the original two functions.

## Verify

```bash
pnpm build:rust
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: from DevTools, call `RustCore.graph.addNode`/`addEdge` a few times and then `neighbors`, and confirm real data comes back across the boundary.

## Definition of done

- [x] All graph functions exported as async, panic-guarded napi calls
- [x] Error codes extend the existing scheme, no parallel mechanism introduced
- [x] Typed `graph` namespace added to `RustCore.ts`; no service touches the raw addon
- [x] Panic-safety demonstrated on a graph export specifically
- [x] Tests and gates pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** `crates/zvs-napi/src/graph.rs` with seven `#[napi]` async exports —
`graphAddNode`, `graphAddEdge`, `graphRemoveEdge`, `graphRemoveNode`,
`graphNeighbors`, `graphShortestPath`, `graphKHopSubgraph` — each through
TASK_018's existing `guarded` wrapper, plus `graphDebugPanic` behind the same
`debug-panic` feature. Because `GraphStore` has no on-disk format, the module owns
a process-wide `Mutex<HashMap<String, GraphStore>>` keyed by a caller-supplied
`graphId`, so TASK_021_EXTRA_3 can scope one graph per vector store. Node, edge
and query payloads cross as JSON text matching zvs-core's camelCase serde shapes;
scalars that were already scalars stay scalars. `apps/studio/src/host/drivers/rust/graphTypes.ts`
holds the zod schemas, `GraphNamespace` and `GraphCorePort`; `RustCore` implements
it as a `graph` namespace that validates requests, serializes once and validates
every response. `addon.ts` lists the new exports and checks for them at load.
Tests: two Rust unit tests in the new module, six wrapper tests in
`apps/studio/tests/graphRust.test.ts`, and two native tests in
`test/native/addon.test.mjs` — a four-node round trip through every export plus a
deliberate panic inside a graph export that rejects as `NATIVE_ERROR` and leaves
the shared store usable.

**Deviated from plan:**

- The task asked for a graph-specific error code appended to the scheme.
  `Error::LimitExceeded` was mapped to `VALIDATION_FAILED` by TASK_021_EXTRA_1; it
  now maps to a new `LIMIT_EXCEEDED`, appended to `AppErrorCode` (append-only, per
  its own comment) with copy in `errorCopy.ts`. No parallel mechanism: the same
  `CoreFailure` conversion and `{ code, message }` envelope carry it. The core
  README and `crates/zvs-core/tests/graph.rs` were updated to match.
- Exports take and return JSON *text* rather than `#[napi(object)]` structs.
  napi objects cannot carry `serde_json::Value`, so edge `metadata` would have had
  to become a nested JSON string — exactly the double-encoding step 2 forbids.
- Graph exports were not folded into a single `graphCall` dispatcher like
  `vectorCall`; the task asked for one export per core function.
- `pnpm test:rust`'s `node --test test/native/` cannot resolve a bare directory on
  Node 26; `node --test test/native/addon.test.mjs` runs the same tests and all six
  pass. Pre-existing, unrelated to this task.
- `cargo fmt --check` still reports the three pre-existing `zvs-core/src/pdf.rs`
  diffs TASK_021_EXTRA_1 documented. Left alone.
- Three renderer tests (`runHistoryStore`, `tasksPage`, `taskStore`) fail on `main`
  before these changes and still fail; unrelated to the addon.

**Next session should know:**

- The registry is in-memory and lives for the life of the loaded addon. A restart
  starts empty, so TASK_021_EXTRA_3 must hydrate a store's nodes and edges from
  SQLite before the first traversal, not lazily on first miss.
- Only the seven functions in scope are exported. `hydrate`, `nodes`, `edges` and
  `stats` exist in `zvs-core` but have no napi export yet; `reconcile(storeId)` in
  TASK_021_EXTRA_3 needs the engine's edge count, so that task should add a
  `graphStats` export following the same shape rather than reading the raw addon.
- `removeNode` returns the cascaded edge count, or `null` when the node was absent
  — that null is the "nothing to roll back" signal, distinct from `0`.
- `shortestPath` returns `null` for no path; an unknown node id is
  `VALIDATION_FAILED`, and a passed node budget is `LIMIT_EXCEEDED`. Those are three
  different outcomes and services should not collapse them.
- `RustCore.graph` rejects a bad request before it crosses the boundary: empty ids,
  an unknown direction, `hops > 8`, a budget outside 1..65536, a non-finite weight.
  Services get `VALIDATION_FAILED` without a native call.
