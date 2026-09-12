# Execution kernel

`RunService` owns admission, inspection, cancellation and boot recovery. `Scheduler` runs
one graph with one abort controller and a per-run concurrency cap. Register node definitions
in `NodeRegistry` before calling `recover()`. Production registers `llm.generate`, `vector.search`, `flow.branch` and `flow.map`.

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

Every node must declare `sideEffect` (the legacy `sideEffectFree` spelling remains supported). Recovery resumes queued, running and blocked runs
only when every referenced definition exists and has no side effects. Runs interrupted during a persisted approval fail with `APPROVAL_DENIED` on recovery. All other unfinished
runs become `interrupted`. Successful steps are checkpoints and are never replayed;
in-flight attempts become abandoned and any replay gets a new attempt number. Completed
failures retain their retry budget and backoff. Graceful shutdown suspends runs using the
same cancellation bound, leaves them unfinished, and drains kernel work before closing
services and SQLite. Resume only after constructing the complete registry.

Run and step changes and their events are persisted transactionally. `run_event` records
the complete stream, including tokens, so recovery retains its stream ID and continues
after the last durable sequence. Each completed run has one terminal `end`. Renderer
delivery and optional debug recorder failures cannot invalidate a checkpoint.

`StepContext` keeps the passport's five members. Nodes emit event drafts; the kernel supplies
stream envelopes. `KernelServices` exposes provider driver acquisition and vector search.
`PermissionService.admit(nodeType, context)` composes the node requirement, global ceiling
and every matching graph `permissionScopes` entry using `off > ask > auto`. A scenario's
subject also adds its scenario scope. Scope names are `skill:<id>`, `scenario:<id>` and
`site:<origin>`. Graph producers must include all applicable scopes. Missing global policy
means `ask`; a scoped `auto` cannot widen that default or an explicit global `ask`/`off`.

Admission persists a pending approval before emitting its event. It blocks new scheduling,
waits for a user decision, and denies after `approvalTimeoutMs` (60 seconds by default).
Already active siblings retain their ordinary cancellation behavior. Denial and permission
failures terminate immediately without retries. Cancellation resolves pending requests;
restart conservatively denies interrupted approvals instead of replaying the operation.
Approval decisions require both run ID and approval ID; expired and duplicate decisions fail.
The current ceiling is checked again before resuming an approved operation.

`runs.approve({ id, approvalId, always: true })` writes an `auto` grant at the request scope.
For unscoped runs that explicitly changes the global ceiling. Scoped grants never change
the global ceiling, so a global `ask` continues to require approval. `PermissionService.grant`
and `revoke` are host APIs for future Tools integration. Grant uses persist in `permission_use`.
Production uses these services by default; injectable admission/approval hooks are host-only
test seams. No permission settings IPC or UI is introduced in this task.

Core node inputs and outputs are available through `NodeRegistry.resolve`:

- `llm.generate`: provider ID, model, messages and optional generation settings; streams text
  and reasoning token events and returns `{ text, reasoning }`. It has side effects because
  generation can incur charges or modify a provider session, so recovery never replays it.
- `vector.search`: the shared vector search input; returns the service's validated hits.
- `flow.branch`: `{ condition, then, else }`; returns the selected JSON value.
- `flow.map`: `{ items, path? }`; projects an own-property path from each item, preserving
  order. An empty path is identity; missing fields fail validation. These flow nodes transform
  data in the static DAG and do not execute arbitrary code or invoke nested nodes.

IPC: `runs.start`, `runs.cancel`, `runs.list`, `runs.get`, `runs.steps`, `runs.approve`, `runs.deny`. Start returns
`{ id, streamId }` before graph execution is scheduled. `wait()` is a host-only completion
hook used by tests and callers that need to drain a run.
