# Execution kernel

The behavioural specification of the kernel and the run services around it. How the Rust types
fit together is in the crate README (`crates/zvs-host/README.md`, "Kernel"); this file says
what they must do.

`RunService` owns admission, inspection, cancellation and boot recovery. `Scheduler` runs one
graph with one `CancellationToken` and a per-run concurrency cap. Register node definitions in
`NodeRegistry` before calling `recover()`. Production registers `llm.generate`,
`vector.search`, `flow.branch`, `flow.map`, `chat.prepare`, `chat.generate` and
`chat.persist`, plus the job, download and MCP nodes.

## Graphs

Graph nodes have unique string IDs, a registered type, dependencies, JSON input and optional
bindings. Ready nodes are ordered by ID. Bindings replace top-level input properties with
either the run input (`{ source: "run" }`) or a direct dependency's complete output
(`{ source: "node", nodeId: "retrieve" }`). A node with bindings must have an object input.
The node receives an isolated copy; its mutations cannot change persisted inputs.

## Retries and cancellation

Retries create separate step rows. `maxAttempts` includes the initial attempt. Backoff is
linear: `backoffMs * failedAttempt`, without occupying a concurrency slot. Cancellation
interrupts backoff and cancels all active nodes. The default grace period is 500 ms; nodes
that ignore cancellation become abandoned, emit an error log, and cannot publish late
results or events. Abandonment cannot stop external effects; implementations must honour the
token. A terminal node failure cancels sibling work and fails the run.

## Recovery and shutdown

Every node declares `side_effect`. Recovery resumes queued, running and blocked runs only when
every referenced definition exists and has no side effects. Runs interrupted during a
persisted approval fail with `APPROVAL_DENIED` on recovery. All other unfinished runs become
`interrupted`. Successful steps are checkpoints and are never replayed; in-flight attempts
become abandoned and any replay gets a new attempt number. Completed failures retain their
retry budget and backoff. Graceful shutdown suspends runs using the same cancellation bound,
leaves them unfinished, and drains kernel work before closing services and SQLite. Resume
only after constructing the complete registry.

## Events

Run and step changes and their events are persisted transactionally. `run_event` records the
complete stream, including tokens, so recovery retains its stream ID and continues after the
last durable sequence. Each completed run has one terminal `end`. Renderer delivery and
optional recorder failures cannot invalidate a checkpoint.

`StepContext` gives a node its run and step identity, its cancellation token, event emission,
the kernel services and tool invocation. Nodes emit event drafts; the kernel supplies stream
envelopes. `KernelServices` exposes provider driver acquisition, vector search and the graph.

## Permissions and approvals

`PermissionService::admit` composes the node requirement, the global ceiling and every
matching graph `permissionScopes` entry using `off > ask > auto`. A scenario's subject also
adds its scenario scope (`admit_subject`). Scope names are `skill:<id>`, `scenario:<id>` and
`site:<origin>`. Graph producers must include all applicable scopes. Missing global policy
means `ask`; a scoped `auto` cannot widen that default or an explicit global `ask`/`off`.

Admission persists a pending approval before emitting its event. It blocks new scheduling,
waits for a user decision, and denies after the approval timeout (60 seconds by default).
Already active siblings retain their ordinary cancellation behaviour. Denial and permission
failures terminate immediately without retries. Cancellation resolves pending requests;
restart conservatively denies interrupted approvals instead of replaying the operation.
Approval decisions require both run ID and approval ID; expired and duplicate decisions fail.
The current ceiling is checked again before resuming an approved operation.

`runs.approve({ id, approvalId, always: true })` writes an `auto` grant at the request scope.
For unscoped runs that explicitly changes the global ceiling. Scoped grants never change the
global ceiling, so a global `ask` continues to require approval. `PermissionService::grant`
and `revoke` are host APIs. Grant uses persist in `permission_use`.

## Questions

`StepContext::ask_user(request)` is the second gate. The scheduler counts an open question like
a pending approval (the run is `blocked` and schedules nothing new), emits a `question` event
with the `QuestionRequestDto`, and resumes `running` once the hook resolves. `QuestionService`
(owned by `RunService`) keeps the waiters in memory, keyed by request id, with no timeout.
`chat.answer({ id, requestId, answers })` validates the answers against the request (known
question and option ids, one option for `single`, text only where `allowOther`, nothing empty)
and `chat.skip` resolves without answers; a request that is not pending for that run is a
conflict. Cancellation ends the wait with `RUN_CANCELLED`, and a restart ends the turn like any
other interrupted chat run.

## Core nodes

Core node inputs and outputs are available through `NodeRegistry::resolve`:

- `llm.generate`: provider ID, model, messages and optional generation settings; streams text
  and reasoning token events and returns `{ text, reasoning }`. It has side effects because
  generation can incur charges or modify a provider session, so recovery never replays it.
  It also accepts `{ request }` to bind a complete request from a preparation node.
- `vector.search`: the shared vector search input; returns the service's validated hits.
- `flow.branch`: `{ condition, then, else }`; returns the selected JSON value.
- `flow.map`: `{ items, path? }`; projects an own-property path from each item, preserving
  order. An empty path is identity; missing fields fail validation. These flow nodes transform
  data in the static DAG and do not execute arbitrary code or invoke nested nodes.

## Runs channels

IPC: `runs.start`, `runs.cancel`, `runs.list`, `runs.get`, `runs.steps`, `runs.detail`,
`runs.retry`, `runs.clearFinished`, `runs.approve`, `runs.deny`. Start returns
`{ id, streamId }` before graph execution is scheduled.

`runs.list` takes one `RunListFilter` (statuses, kinds, text query, date range, `live`, cursor
paging) and answers with `RunSummaryDto` rows plus global counts, so the Tasks page and Runs &
Logs are the same query at different filters. `live` means unfinished plus failures from the
last 24 hours. `runs.retry` re-runs a finished graph as a new run carrying `retryOfId`; chat
turns are refused because their graph binds the messages of the original turn.

`RetentionService` sweeps finished runs on a timer: past `runs.retention.days` (default 30) a
run keeps its summary, steps, timings and errors while step inputs, outputs and recorded events
are dropped and `prunedAt` is stamped. Whole runs are only ever deleted by
`runs.clearFinished`, which spares any run a conversation message still references.

## Chat

Construct `ChatService` with the same registry and run service before recovery. A turn runs
one `vector.search` per attached store, followed by `chat.prepare`, `chat.generate` and
`chat.persist`. Retrieval nodes are independent and run within the kernel concurrency cap;
no retrieval nodes exist for a conversation without stores. Embedding receives the run's
cancellation token. A native search cannot be interrupted, but cancelled results are
discarded.

The channels are `chat.conversations.list|get|create|remove|rename`, `chat.send` and
`chat.cancel`. Create accepts providerId, modelId (a discovered model row ID or external ID),
settings, systemPrompt, attachedStoreIds and an optional title. Get returns the conversation
with ordered messages. Send accepts `{ conversationId, text }` and returns `{ id, streamId }`;
cancel accepts `{ id }` with the run ID. Only one turn may run in a conversation at a time.
Removal rejects active conversations and cascades messages while retaining run history.
`chat.answer` and `chat.skip` resolve a question the model asked through `chat.ask`.

Every turn whose model can call tools is offered `chat.ask` (wire name `ask_user`) with a short
prompt section, whatever tools the conversation picked. The loop intercepts it before tool
invocation: the host assigns question and option ids, validates the request and returns a
malformed call to the model as the call result. A second `chat.ask` in a round is refused, and
the call does not count against the call budget. The call record is `awaiting-answer` with a
`QuestionRecordDto` as meta, then `succeeded` with the answers or `skipped`; the model receives
a plain-text summary of the answers.
Missing model/provider references fail future sends while existing history stays readable.

The renderer subscribes to the returned stream immediately, appends text token deltas to the
pending answer, and refreshes the conversation on `end`. Reasoning deltas are distinct from
answer text. `RunService::subscribe` supplies synchronous host observers before renderer
delivery and removes them on end or shutdown. Chat uses it to save each accepted token, so
cancellation, provider failure and shutdown retain a partial answer. `chat.persist` marks a
complete answer non-partial. Recovery never repeats a charged generation. A cancellation
before preparation leaves only the user message.

Prompt windowing reserves output capacity and always keeps the system prompt and current
turn. If those cannot fit, send fails before creating messages or a run. Only retrieved
passages that fit are included and cited. Citations contain storeId, documentId, sourcePath,
chunkIndex and score. Remaining space holds a contiguous recent history, without a leading
orphan assistant message. Trimming is logged both to the run and the host log.

Counts use `estimate_tokens`, a conservative script-aware estimate, and are marked
`usageEstimated: true`. Unknown context windows use 8192;
output defaults to at most 1024, the model's output limit, and one quarter of context. An
untouched default title is derived from the first question after an answer arrives, without
another model call.
