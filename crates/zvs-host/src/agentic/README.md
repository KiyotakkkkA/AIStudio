# agentic

Agentic sessions: a goal, a workspace folder, a set of tools and a budget, run by the model as
a plan of steps until it finishes, runs out of budget or is stopped. A session is one kernel
run with a single node, `agentic.session` (`SESSION_NODE_ID = "session"`), so it inherits the
kernel's persistence, cancellation, approvals and recovery (`../kernel/README.md`).

## Files

| File         | Contents                                                                                            |
| ------------ | --------------------------------------------------------------------------------------------------- |
| `mod.rs`     | `register_agentic_nodes`                                                                            |
| `service.rs` | `AgenticService`: create, get, list, update plan, set mode, instruct, extend budget, cancel, remove |
| `store.rs`   | `SessionStore` (rows, transcript, DTO mapping) and `session_tools` (the tools a session may see)    |
| `node.rs`    | `SessionNode`: the model loop, bridge tools, prompts, budget checks, the final output               |
| `plan.rs`    | The plan as a list of steps and every operation on it; `LoopGuard`; changed-file bookkeeping        |

## Pipeline: one session

1. `AgenticService::create` validates the goal, folder, tool keys and budget, stores the
   session row and starts a run with the `agentic.session` node.
2. `SessionNode` builds the system prompt: `AGENT_RULES`, plus `READ_ONLY_RULES` or
   `ASK_RULES` for the permission mode, plus `ARTIFACTS_RULES`. The tools offered are the
   session's selection filtered by `session_tools` (read-only mode offers only tools that
   read) plus three bridge tools: `update_plan`, `complete_step`, `finish`.
3. Loop, one round per model turn:
   - check the budget (`budget_exceeded`: steps, then tokens, then wall-clock minutes),
   - generate with the transcript so far,
   - run each tool call: bridge tools change the plan; other tools go through
     `StepContext` tool invocation and the permission model, with `minimum_tier` raising the
     tier for the mode (`Ask` mode asks for every non-read-only tool, `Auto` only for
     destructive ones),
   - link the kernel step ids to the current plan step, record file changes, save the
     transcript and usage, and publish the session DTO as an `agentic` event.
4. The loop ends on `finish` (summary, with an optional `<artifacts>` list of created files),
   a budget stop, a loop-guard stop, cancellation or an error. The plan is closed with
   `finish` (running step → done) or `fail` (running step → failed).

User actions between rounds arrive as messages: a plan edit is announced as
"Plan changed by the user" with what was added and removed, an instruction as
"Instruction from the user". The model is told never to re-add a removed step, and the host
enforces it (below).

## Plan algorithms (`plan.rs`)

The plan is a `Vec<AgenticPlanStepDto>` with ids `p1`, `p2`, … (`next_number` takes the
highest existing number + 1, so ids are never reused). Status is `queued`, `running`,
`done` or `failed`; exactly one step is `running` while work is in progress (`current`
promotes the first queued step when none runs).

- **`propose(titles)`** (model's `update_plan`): settled steps (done, failed) stay in place.
  The open steps are replaced by the new list; a new title equal to an open step's title
  reuses that step (keeping its id and linked kernel steps), others get fresh ids. Titles are
  trimmed and capped at `AGENTIC_STEP_TITLE_MAX`, the plan at `AGENTIC_PLAN_MAX`.
- **`edit(edits)`** (user's edit): non-queued steps are fixed. Each edit with a known id keeps
  that step (renamed if the title changed); an edit without one adds a step. Queued steps not
  mentioned are removed. `PlanChange` reports added, removed, and whether order or titles
  changed, which becomes the note shown to the model.
- **`complete(summary)`**: the running step becomes done with a capped summary, and the next
  queued step starts.

## Loop guard

`LoopGuard` stops sessions that spin without progress:

- **Repeats.** Each round's tool calls are reduced to a signature (tool names plus arguments
  normalised as JSON, so whitespace does not matter). The same signature `REPEAT_LIMIT` (3)
  rounds in a row trips the guard.
- **Revisions.** `update_plan` calls without a completed step in between are counted;
  `REVISION_LIMIT` (4) trips the guard. `complete_step` resets the count.
- **Removed steps.** Titles the user removed are remembered (`forbid`) and filtered out of
  later proposals (`allowed`), compared after trimming; the model is told they were dropped
  (`REMOVED_DROPPED`).

## Changed files

`record_changes` reads `changes` (`FileDiffDto`) from file-tool outputs and accumulates
added/removed line counts per path, so the session view lists every file the agent touched.
