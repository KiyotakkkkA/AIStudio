# drivers

Everything the host talks to outside its own process or crate, behind ports so services and
the kernel can be tested with fakes (`../testing.rs`).

| Submodule | Talks to                                                                                                                       | README          |
| --------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------- |
| `ai`      | Model vendors: OpenAI-dialect APIs, Ollama, local embedding engines, Qwen / Z.ai web accounts, GitHub Copilot, ChatGPT (Codex) | `ai/README.md`  |
| `mcp`     | MCP servers over stdio and streamable HTTP, OAuth                                                                              | `mcp/README.md` |
| `core`    | `zvs-core` in-process: chunking, hashing, vectors, graph                                                                       | this file       |
| `sidecar` | The `zvs-jobd` process: downloads and document jobs                                                                            | this file       |

## core (`core/`)

Three ports: `TextCore` (chunk, hash), `VectorCore` (LanceDB-backed indexes) and `GraphCore`
(per-store knowledge graphs). `InProcessCore` implements all three over `zvs-core`
(`crates/zvs-core/README.md` has the algorithms).

- Synchronous work (chunking, hashing, every graph call, removing a store) runs on
  `spawn_blocking`; vector calls are LanceDB futures.
- Every call is wrapped in `catch_unwind`: a panic becomes `NATIVE_ERROR` and the next call
  works normally.
- Input is validated before it reaches the core: `ChunkConfig` sizes and overlap, graph ids,
  hop counts (`MAX_HOPS`) and node budgets (`MAX_NODE_BUDGET`).
- `upsert` passes the caller's `CancellationToken` into `VectorIndex::upsert`; an already
  cancelled token returns `RUN_CANCELLED` without touching the store.
- `GraphRegistry` (graphs keyed by `graphId`) lives inside `InProcessCore`, so the one
  instance built by the composition root is the process-wide registry.

## sidecar (`sidecar/`)

`SidecarDriver` runs `zvs-jobd` (`crates/zvs-jobd/README.md`) and multiplexes jobs over its
stdin/stdout.

Protocol: newline-delimited JSON (`protocol.rs`). The host sends `{ id, method, params }`;
the sidecar answers with progress frames and one result or error per id. A line over 8 MiB
kills the child.

Lifecycle:

1. **Spawn on demand** with `CREATE_NO_WINDOW`; stderr lines are logged as
   `Sidecar diagnostics`.
2. **Health.** A `ping` every 5 s with a 2 s timeout; a silent child is killed.
3. **Run.** `run(job, params, JobRunOptions { cancel, on_progress })`. When the token fires
   the driver sends `job.cancel`, waits a 2 s grace, then kills the process if the job is
   still running.
4. **Crash.** Every pending job fails with `SIDECAR_UNAVAILABLE`; the next spawn is delayed by
   exponential backoff, `250 ms × 2^restarts` capped at 30 s. A successful ping resets the
   restart count.
5. **Dispose** kills and waits for the child; later `run` calls answer `CONFLICT`.

`StudioPaths::sidecar_path` points at `zvs-jobd(.exe)` next to the main executable. Its tests
are in `crates/zvs-jobd/tests/driver.rs` and run against the real binary.
