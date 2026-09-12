# Execution kernel

`RunService` owns admission, inspection, cancellation and boot recovery. `Scheduler` runs
one graph with one abort controller and a per-run concurrency cap. Register node definitions
in `NodeRegistry` before calling `recover()`. The production registry is intentionally empty
until TASK_023 supplies node implementations.

Graph nodes have unique string IDs, a registered type, dependencies, JSON input and optional
bindings. Ready nodes are ordered by ID. Bindings replace top-level input properties with
either the run input (`{ source: "run" }`) or a direct dependency's complete output
(`{ source: "node", nodeId: "retrieve" }`). A node with bindings must have an object input.
The node receives an isolated copy; its mutations cannot change persisted inputs.

Retries create separate step rows. `maxAttempts` includes the initial attempt. Backoff is
linear: `backoffMs * failedAttempt`, without occupying a concurrency slot. Cancellation
interrupts backoff and aborts all active nodes. The default grace period is 500 ms; nodes
that ignore cancellation become abandoned, emit an error log, and cannot publish late
results or events. In-process abandonment cannot stop external effects; implementations
must honour the signal. A terminal node failure aborts sibling work and fails the run.

Every node must declare `sideEffectFree`. Recovery resumes queued, running and blocked runs
only when every referenced definition exists and declares `true`. All other unfinished
runs become `interrupted`. Successful steps are checkpoints and are never replayed;
in-flight attempts become abandoned and any replay gets a new attempt number. Completed
failures retain their retry budget and backoff. Graceful shutdown suspends runs using the
same cancellation bound, leaves them unfinished, and drains kernel work before closing
services and SQLite. Resume only after constructing the complete registry.

Run and step changes and their events are persisted transactionally. `run_event` records
the complete stream, including tokens, so recovery retains its stream ID and continues
after the last durable sequence. Each completed run has one terminal `end`. Renderer
delivery and optional debug recorder failures cannot invalidate a checkpoint.

`StepContext` keeps the passport's five members. `KernelServices` exposes only provider
driver acquisition and vector search. `requestApproval` delegates to an injected hook and
tracks blocked/running state. The optional `admit` hook is the TASK_023 permission seam;
without it only definitions declaring `{ kind: "none" }` can execute. Tool permissions
and approvals fail closed when their services are absent.

IPC: `runs.start`, `runs.cancel`, `runs.list`, `runs.get`, `runs.steps`. Start returns
`{ id, streamId }` before graph execution is scheduled. `wait()` is a host-only completion
hook used by tests and callers that need to drain a run.
