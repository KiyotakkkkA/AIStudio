# Execution kernel

`RunService` owns admission, inspection, cancellation and boot recovery. `Scheduler` runs
one graph with one abort controller and a per-run concurrency cap. Register node definitions
in `NodeRegistry` before calling `recover()`. Production registers `llm.generate`, `vector.search`, `flow.branch`, `flow.map`, `chat.prepare` and `chat.persist`.

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
  It also accepts `{ request }` to bind a complete request from a preparation node.
- `vector.search`: the shared vector search input; returns the service's validated hits.
- `flow.branch`: `{ condition, then, else }`; returns the selected JSON value.
- `flow.map`: `{ items, path? }`; projects an own-property path from each item, preserving
  order. An empty path is identity; missing fields fail validation. These flow nodes transform
  data in the static DAG and do not execute arbitrary code or invoke nested nodes.

IPC: `runs.start`, `runs.cancel`, `runs.list`, `runs.get`, `runs.steps`, `runs.approve`, `runs.deny`. Start returns
`{ id, streamId }` before graph execution is scheduled. `wait()` is a host-only completion
hook used by tests and callers that need to drain a run.

## Chat

Construct `ChatService` with the same registry and run service before recovery. A turn runs
one `vector.search` per attached store, followed by `chat.prepare`, `llm.generate` and
`chat.persist`. Retrieval nodes are independent and run within the kernel concurrency cap;
no retrieval nodes exist for a conversation without stores. Embedding receives the run's
abort signal. Native search cannot be interrupted, but cancelled results are discarded.

The channels are `chat.conversations.list|get|create|remove|rename`, `chat.send` and
`chat.cancel`. Create accepts providerId, modelId (a discovered model row ID or external ID),
settings, systemPrompt, attachedStoreIds and an optional title. Get returns the conversation
with ordered messages. Send accepts `{ conversationId, text }` and returns `{ id, streamId }`;
cancel accepts `{ id }` with the run ID. Only one turn may run in a conversation at a time.
Removal rejects active conversations and cascades messages while retaining run history.
Missing model/provider references fail future sends while existing history stays readable.

Subscribe to the returned stream immediately, append text token deltas to the pending answer,
and refresh the conversation on `end`. Reasoning deltas are distinct from answer text.
`RunService.subscribe` supplies synchronous host observers before renderer delivery and
removes them on end or shutdown. Chat uses it to save each accepted token, so cancellation,
provider failure and shutdown retain a partial answer. `chat.persist` marks a complete
answer non-partial. Recovery never repeats a charged generation. A cancellation before
preparation leaves only the user message.

Prompt windowing reserves output capacity and always keeps the system prompt and current
turn. If those cannot fit, send fails before creating messages or a run. Retrieved passages
use payload.text (or a string payload); only passages that fit are included and cited.
Citations contain storeId, documentId, sourcePath, chunkIndex and score. Remaining space
holds a contiguous recent history, without a leading orphan assistant message. Trimming is
logged both to the run and host log.

The streaming driver port currently exposes no provider usage or tokenizer. Counts use a
conservative UTF-8 byte estimate with message overhead and are marked `usageEstimated: true`.
Unknown context windows use 8192; output defaults to at most 1024, the model's output limit,
and one quarter of context. An untouched default title is derived from the first question
after an answer arrives, without another model call. TASK_025 owns the renderer integration.
