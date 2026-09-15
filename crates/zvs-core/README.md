# zvs-core

Shared Rust logic for the napi addon and job sidecar. Adapters own transport and
runtime integration; this crate never opens SQLite.

## Cancellation

Every long-running function accepts a `CancellationToken` from `tokio-util` and
checks it before work, at loop boundaries, and before returning its result.
Cancellation returns `Error::Cancelled` (`RUN_CANCELLED`). Vector upserts retain
already committed batches; callers may retry the same IDs or delete by source.
TASK_027 must pass the job token (or a child token) into every core operation.
Blocking backend calls must be bounded or support cancellation themselves.

## Chunking

`chunk::chunk_text` borrows source text and returns chunks with UTF-8 byte ranges
and token counts. `ChunkConfig` defaults to size 256 and overlap 32. Size must be
positive; overlap must be smaller than size.

A token is a run of non-whitespace graphemes, a word-count approximation rather
than a model tokenizer. Unbroken words and text without spaces remain one token,
regardless of length. Consumers needing a model's context limit must use that
model's tokenizer separately. Grapheme clusters are never divided, including
combining marks, emoji sequences, and whitespace joined to combining marks.

Chunks contain at most `size` tokens and consecutive chunks share exactly
`overlap` tokens. Interior whitespace is preserved; exterior whitespace is
omitted. Before the final chunk, the last sentence-ending token within the size
limit is preferred when it leaves enough tokens to guarantee forward progress.
Sentence endings recognize `.`, `!`, `?`, their CJK equivalents, and common
closing quotes or brackets. This is a punctuation heuristic, not language-aware
sentence parsing. Without a suitable boundary, the size limit wins.

## Hashing and errors

`hash::content_hash` returns the lowercase, 64-character BLAKE3 digest of raw bytes,
checking cancellation between 64 KiB blocks. It performs no text normalization.
TASK_042 can use the digest as a content-cache key.

`Error` and `Result<T>` are shared across capabilities. Error codes match
`packages/shared/src/errors/AppErrorCode.ts`: invalid input maps to
`VALIDATION_FAILED`, cancellation to `RUN_CANCELLED`, backend failures and general
IO failures to `NATIVE_ERROR`. Missing files and denied access retain `NOT_FOUND`
and `PERMISSION_DENIED`. IO errors preserve their source.

## Module handoffs

| Module                   | Implementation task                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------------------ |
| `chunk`, `hash`, `error` | TASK_017                                                                                         |
| `index`                  | TASK_019 storage/search; TASK_028 indexing orchestration                                         |
| `graph`                  | TASK_021_EXTRA_1 store and traversal; TASK_021_EXTRA_2 napi exposure                             |
| `embed`                  | TASK_028 embedding pipeline                                                                      |
| `ocr`                    | Reserved for sidecar document processing after TASK_027; no OCR implementation task assigned yet |

The crate root includes this README as its Rust documentation. Empty modules have
no comments, as requested for TASK_017.

## Build

LanceDB is pinned to [0.38.0](https://docs.rs/crate/lancedb/0.38.0), the current
stable release checked for TASK_019. Its default feature build references a
feature-gated `Error::Http` in `job.rs`; the `remote` feature is enabled as a
compilation workaround. The index API accepts only host-supplied absolute local
paths and does not use remote connections.

petgraph is pinned to [0.8.3](https://docs.rs/crate/petgraph/0.8.3), the current stable
release checked for TASK_021_EXTRA_1.

Lance requires the Protocol Buffers compiler (`protoc`, verified with 36.1).
Install it from the official Protocol Buffers release and put it on PATH or set
`PROTOC` to its executable. CI provisions it in every Rust-building job. The napi
build script also recognises `target/tools/protoc/bin/protoc[.exe]` for a local
installation. For direct Cargo commands in PowerShell with that installation:
`$env:PROTOC = (Resolve-Path target/tools/protoc/bin/protoc.exe).Path`.

Use `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`
from the repository root. `pnpm build` runs `build:rust` before package builds,
using `cargo build --workspace --release --locked`. Turbo delegates incremental
Rust caching to Cargo so target artifacts remain specific to the current machine.

## Vector storage

`index::VectorIndex::create(path, dimension, metric)` opens or creates a `vectors`
table in the host-supplied directory. Dimension is required; the confirmed UI
default is 1024 and the default metric is cosine. Both are persisted in the Arrow
schema and checked when reopening. A mismatch rejects with `VALIDATION_FAILED`.
`open` does not create a missing store. TypeScript resolves per-store paths with
`vectorStoresDir` and `vectorStorePath` in `platform/paths.ts`.

Rows contain a unique nonempty ID, finite float32 vector, JSON payload, document
ID, chunk index and source path. Upsert validates the complete input, then uses
merge-insert in batches of 256, replacing all fields for an existing ID. Duplicate
IDs within one request are rejected. Cancellation is checked during validation,
between batches and before returning. A running commit finishes atomically before
cancellation takes effect; the entire request is not a transaction.

Search performs an exact FLAT scan with the stored metric, applies an optional
Lance SQL predicate before selecting top-k, then applies the inclusive score
floor. Filter fields include `document_id`, `chunk_index`, `path`, `id` and
`payload`; filters are host-controlled SQL expressions, not renderer input.

Scores always range from 0 to 1, with higher scores better:

- Cosine: `clamp(1 - cosine_distance, 0, 1)`. A score of 0.91 means cosine
  similarity 0.91. Orthogonal and negatively aligned vectors score 0. Zero
  vectors are rejected.
- L2: `1 / (1 + squared_euclidean_distance)`. Identical vectors score 1; squared
  distance 1 scores 0.5.
- Dot: `sigmoid(dot_product) = 1 / (1 + exp(distance - 1))`, because Lance's
  dot distance is `1 - dot_product`. A zero dot product scores 0.5.

These are similarity values, not probabilities or values comparable across
embedding models and metrics. These semantics are documented here instead of
code comments at the user's request.

Delete methods quote IDs as SQL literals. Stats report live row count, stored
dimension and metric, `FLAT` index type, and physical file bytes within this
table's `vectors.lance` directory, including retained versions. No HNSW index is
created or claimed. Store descriptions, embedding providers and document counts
remain owned by the host's SQLite repositories.

The addon accepts JSON through async `vectorCall` and catches panics across all
future polls. `RustCore` exposes typed create/open/upsert/search/delete/stats
methods through `VectorCorePort`. Upsert bridges `AbortSignal` through async
operation-token allocation, cancellation and release; release also handles JSON
serialization failures before the native call starts.

## Relationship graph

`graph::GraphStore` answers "what is connected to this" the way `index` answers
"what is similar to this". TASK_021_EXTRA_1 chose petgraph in memory over CozoDB
and over hand-written SQL traversal: the crate gains no persistence engine, and
the memory bound is accepted until real usage disproves it.

The store is therefore not opened from a path. Persistence belongs to the host,
which keeps node and edge rows in SQLite and calls `GraphStore::hydrate` at
startup; `nodes()` and `edges()` return the current rows back, both sorted, for
the host to write. Rust still never opens SQLite. This is the one deviation from
the task's steps, which assumed an engine with its own on-disk format.

A node is an id plus a `document` or `chunk` kind. Ids are the ids already in
`vector_document` and the LanceDB rows; the graph never invents one, and an edge
whose endpoints are not both present is rejected with `VALIDATION_FAILED`. An
edge is a from, a to, a `relation` string, an optional finite weight and a JSON
metadata blob. `relation` is an open string — `cites`, `derived_from`,
`co_occurs`, `user_linked` and anything later tasks need — so a new relation
kind costs no Rust change.

Ids and relations are trimmed and must be nonempty. Self-loops are rejected.
`(from, to, relation)` identifies an edge: adding the same triple again replaces
its weight and metadata rather than creating a parallel edge, and `add_node`
on a known id updates its kind. Both add methods return whether the row was new,
and the batch forms validate everything before mutating anything.

`remove_node` is the cascade TASK_020's document delete will call: it drops the
node and every edge touching it in one call, returning the edge count.
`remove_edges_touching` does the same while keeping the node.

`neighbors` takes an optional relation and a direction (`outgoing`, `incoming`
or `both`) and reports which side each edge was traversed from. `shortest_path`
is breadth-first over hop count, not over weight — weights here are relevance,
not distance. `k_hop_subgraph` returns every node within `hops` of the root, each
with its hop distance, and the induced edges among them: an edge between two
nodes in the returned set is included whatever direction the traversal reached
them from. Results are sorted — neighbors and edges by id, subgraph nodes by
hops then id — so repeated queries return identical output.

Traversal is bounded, not merely finite. `hops` may not exceed `MAX_HOPS` (8) and
the node budget defaults to `DEFAULT_NODE_BUDGET` (4096), capped at
`MAX_NODE_BUDGET` (65536). A k-hop expansion that would pass its budget stops and
returns `Error::LimitExceeded`, which also maps to `VALIDATION_FAILED`, rather
than walking a dense graph to exhaustion. An unknown node id in any query is
`VALIDATION_FAILED`, not an empty result.

Cancellation follows the crate convention: `add_nodes`, `add_edges`,
`remove_nodes`, `shortest_path` and `k_hop_subgraph` check the token before work,
at frontier and batch boundaries, and before returning. `neighbors` and the
single-item CRUD calls are O(degree) and take no token.
