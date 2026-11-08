# services · chat

`ChatService` (`../chat.rs`) and its helpers: one user message becomes one kernel run whose
graph retrieves, prepares, generates (with the tool loop) and persists. This folder holds the
pieces split out of `chat.rs`: the message queue, ended-process wake-ups, and subagents.

## Files

| File (in `services/`)                                              | Contents                                                                                     |
| ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| `chat.rs`                                                          | `ChatService`: send, cancel, conversations, the `chat.prepare/generate/persist` nodes, goals |
| `chat/queue.rs`                                                    | Messages queued while a turn runs: add, edit, remove, flush                                  |
| `chat/wake.rs`                                                     | Continuing the conversation when a `notify` background process ends                          |
| `chat/subagent.rs`                                                 | Delegated runs (`agent.delegate`): presets, budgets, report back                             |
| `chat_prompt.rs`                                                   | `PromptParts`: the system prompt assembled from dialog, profile, memory, knowledge, tools, … |
| `chat_history.rs`                                                  | `window_chat_history`: fits sources and history into the context window                      |
| `chat_carry.rs`                                                    | Carrying earlier answers' tool results, sources and reasoning into later turns               |
| `chat_tools.rs`                                                    | `OfferedTool`: which tools a turn offers, wire-safe names, tiers                             |
| `tool_loop.rs`                                                     | The model ↔ tool rounds, budgets, shedding old results                                       |
| `tool_arguments.rs`                                                | Repairing and validating tool-call arguments against the schema                              |
| `chat_reply.rs`                                                    | Buffering streamed text, reasoning and tool-call deltas into persisted rounds                |
| `chat_ask.rs`, `chat_goal.rs`, `chat_memory.rs`, `chat_plugins.rs` | The ask-user tool, `/goal`, memory tools, plugin tools                                       |

## Pipeline: `send`

```
send ─► store user message + run row (one transaction)
          │
          ▼
   vector.search ×N ──► chat.prepare ──► chat.generate ──► chat.persist
   (one per attached      (merge hits,      (tool loop,       (final message,
    store, skipped in      neighbours,       streamed          title, usage,
    agentic retrieval)     graph, history)   deltas saved)     citations)
```

1. **Admission.** One turn per conversation at a time: `send` on a busy conversation fails
   with `CONFLICT` (`ACTIVE_TURN`); the renderer and CLI put such messages in the
   conversation's queue instead (see "Queue" below). The system prompt plus the current message must fit the
   model's context minus its output budget, or the send is refused up front.
2. **Retrieval.** The query is `retrieval_query(text, previous user message)` (follow-ups
   are searched together with the previous question). Each attached store gets a
   `vector.search` node, unless the conversation uses agentic retrieval, where the model
   searches through tools instead.
3. **Prepare.** `merge_retrieval` combines the hits; ±1 neighbour chunks are fetched and
   joined; related documents are followed through the knowledge graph when enabled;
   `window_chat_history` fits everything (below); `PromptParts` builds the system prompt;
   the partial assistant message row is inserted.
4. **Generate.** The tool loop (below) streams the answer. Every delta is written as it
   arrives, so cancellation, a provider failure or shutdown keeps the partial answer;
   recovery marks the turn `interrupted` and never repeats it.
5. **Persist.** Final content, reasoning, tool calls, citations and usage; the conversation
   title is derived after the first answer.

## Context window (`window_chat_history`)

Budget = context window − max output tokens; tokens are counted with `o200k_base`.

1. System prompt + current message are mandatory (otherwise `BUDGET_EXCEEDED`).
2. Sources in merged order: each chunk is costed as its fenced form; chunks that do not fit
   are counted as dropped. Selected chunks of the same document are grouped, consecutive
   chunk indices are joined into one passage, and passages are ordered by their best rank,
   each with a numbered citation.
3. Related (graph) documents not already present are added while they fit.
4. History is taken newest-first while it fits, then restored to chronological order; the
   number of trimmed messages is reported in the context stats.

`chat_carry::fit` then shrinks carried context from earlier answers in stages — shorten,
drop tool calls, drop sources — only as far as needed.

## Tool loop (`tool_loop.rs`)

- Offered tools: the conversation's picked tools that are enabled, available and not `off`,
  under wire-safe names (`chat_tools.rs`).
- Each round: generate; if the reply has tool calls, run them as kernel steps through
  `StepContext::invoke_tool` (admitted as model calls, so the ceiling, the execution policy and
  approvals apply), fence every result as untrusted data, append, repeat.
- Budget: `LoopBudget::Calls(n)` from the profile (1–200) ends the loop when rounds or executed
  calls reach `n`, then asks for an answer; `/goal` turns and "unlimited" use `Open`.
- When the transcript outgrows the context, tool results are replaced by a short placeholder
  (`SHED_RESULT`) oldest first until it fits, never touching results of the tool named as
  kept; the model is told it may call the tool again.
- Arguments are mended against the tool's schema (`repair`: JSON passed as a string is
  decoded, `anyOf` / `oneOf` / `allOf` branches are followed, up to 24 levels deep). Remaining
  violations go back to the model as a Russian problem list (at most 8) with the expected shape
  (`arguments_failure`), so it can retry.

## Queue (`chat/queue.rs`)

Messages typed during a turn are kept per conversation in the host (`queue_add`, `edit`,
`remove`). When the turn's run ends, the queued messages are sent together as the next turn.
`queue_flush` marks the queue _urgent_: the running tool loop checks `yields()` between rounds
and stops early so the queued messages are answered without waiting for the whole loop.

## Wake-ups (`chat/wake.rs`)

When a background process started with `notify` ends, the conversation continues on its own
with an event message holding the process tail and artifacts. At most `WAKE_STREAK_MAX` (3)
automatic continuations in a row without a user message, so two processes cannot keep a
conversation talking to itself.
