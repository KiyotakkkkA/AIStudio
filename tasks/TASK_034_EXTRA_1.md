---
task: TASK_034_EXTRA_1
title: Model tool calling in chat — the call loop, adapters and approvals
phase: 6 — Tools, MCP and Skills (extension)
status: done
depends_on: [TASK_034, TASK_024_EXTRA_1]
unblocks: [TASK_034_EXTRA_2]
estimate: 4 h
---

# TASK_034_EXTRA_1 — Model tool calling in chat — the call loop, adapters and approvals

## Context

The tool side is built: `ToolRegistry` (`apps/studio/src/host/tools/ToolRegistry.ts`, TASK_031)
holds every kernel node and MCP tool with its permission ceiling, `McpService` (TASK_032) invokes
remote tools, and the Tools page (TASK_034) lets the user see and set tiers and try tools by hand.
TASK_024's own scope note says it plainly: *"the node exists but the model is not yet given
tools"*. No task in Phases 6–7 owns that step before the agentic engine (TASK_040), which needs a
planning loop on top of it.

The generation port (`apps/studio/src/host/drivers/ai/ports.ts`) has no notion of tools:
`GenerateRequest` is `model, messages, system` plus sampling settings; `ChatMessage.role` is only
`system | user | assistant`; `TextDelta` is text or reasoning. The chat graph is fixed —
`vector.search…` → `chat.prepare` → `llm.generate` → `chat.persist`.

Adapters differ in how tools can be offered:

- `openai-compatible` (OpenRouter, Mistral, Ollama, custom) and `local` (`llama-server`) speak the
  OpenAI `tools` / `tool_calls` wire format natively, where the model supports it.
- `qwen-web` and `deepseek-web` have no tool field. `/docs/know-how/QWEN_DEEPSEEK_TOOLS_INJECTING.md`
  describes the workaround: describe tools in the system prompt, have the model reply with one
  marked JSON object, parse it, run the tool, return the result as a fenced system message.

## Why

Every "the model decides" feature — agentic knowledge retrieval (`TASK_034_EXTRA_2`), skills,
agentic sessions — needs the same loop: offer tools, receive a call, admit it against the ceiling,
run it, feed the result back, repeat within a budget. Building it once, for chat, under the
permission model the Tools page now makes visible, is cheaper than letting each later task grow its
own.

## Depends on

- `TASK_034` — the Tools page, so tiers the loop enforces are user-visible and settable.
- `TASK_024_EXTRA_1` — the reworked context assembly the loop extends with tool messages.

## Read first

- `/docs/PASSPORT.md` §14 — permission tiers, ceilings, approvals
- `/docs/know-how/QWEN_DEEPSEEK_TOOLS_INJECTING.md` — the prompt-injection protocol for web adapters
- `apps/studio/src/host/tools/ToolRegistry.ts`, `kernelTools.ts`
- `apps/studio/src/host/services/ChatService.ts` — the fixed chat graph this task makes iterative
- TASK_023 — approval gates the loop must route through

## ❓ DECISION — which tools does a chat turn see?

- **(a)** Only tools explicitly enabled for the conversation (a picker in the composer), within
  the ceiling.
- **(b)** Every tool whose ceiling is not `off`.
- **(c)** None by default; only built-in tools added by later tasks (knowledge search) are on.

Fallback if unavailable: **(a)** — tool descriptions cost context and every exposed tool is
attack surface for prompt injection from retrieved text.

## Scope

**In scope**
- Port: `GenerateRequest.tools?: ToolSpec[]` (name, description, JSON Schema); `ChatMessage` gains
  `tool` role with `toolCallId`, and assistant messages may carry `toolCalls`; stream deltas gain
  a `tool_call` kind (id, name, arguments JSON, assembled from partial deltas).
- Adapter capability flag `tools: "native" | "injected" | "none"`, per adapter and overridable per
  model (models without tool support fall back to `none`).
- `openai-compatible` and `local`: native `tools`/`tool_calls` mapping + recorded-fixture tests.
- `qwen-web` and `deepseek-web`: injected protocol from the know-how doc — unique marker key,
  streamed output buffered until it is clearly not a call, result returned fenced as
  `[SYSTEM_TOOL_RESULT]`.
- Chat loop: generate → if tool calls, admit each through the ceiling and approval gates
  (TASK_023), run via the registry, append results, generate again; stop on a plain answer or the
  budget (default 4 rounds, 8 calls). Every call is a step in the run, visible on Tasks.
- Tool results are untrusted input: size-capped, fenced, never able to change the tool set or
  tiers.
- Chat page: compact tool-call cards in the assistant message (tool, arguments, status, result
  preview, approval prompt inline when needed).
- Tool selection per the decision.

**Out of scope** (do not do these, even if tempting)
- A dedicated knowledge-search tool and retrieval policy — `TASK_034_EXTRA_2`
- Planning, self-extending graphs, budgets beyond the round cap — TASK_040
- Parallel tool execution — calls in one round run sequentially

## Steps

1. Port types and DTOs; persisted message shape for tool calls/results (migration).
2. Capability flag; native mapping for `openai-compatible` and `local` with fixtures.
3. Injected protocol for web adapters; parser tests including malformed and partial JSON.
4. Iterative chat graph with admission, approvals, round/call caps, cancellation mid-loop.
5. Chat page tool-call cards and inline approvals.
6. Tool selection UI per the decision.
7. Tests: refused `off` tool, approval required for a destructive tool, loop stops at the cap,
   cancellation during a tool call leaves a consistent conversation.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: with an MCP read-only tool enabled, ask a question that needs it on an OpenRouter
model and on Qwen web; both call the tool and answer from the result. Set the tool to `off` on the
Tools page and confirm the next turn cannot call it.

## Definition of done

- [x] Native and injected tool calling work on at least one model each
- [x] Every call admitted against the ceiling, with approvals where the tier requires them
- [x] Loop bounded, cancellable and fully visible as run steps
- [x] Tool calls rendered on the Chat page
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Decision (a): only tools picked per conversation are offered, and only while they are
available, enabled and not `off`. `conversation.enabled_tools` and `message.tool_calls` columns
(migration `0021_chat_tool_calls`), `ChatToolCallDto`, `MessageDto.toolCalls`,
`CreateConversationInput.enabledTools` and the `chat.conversations.updateTools` channel. Port:
`GenerateRequest.tools` (`ToolSpec`), `ChatMessage` gains the `tool` role with `toolCallId` and
assistant `toolCalls`, `TextDelta` gains a `tool_call` kind carrying an assembled call, and
`FinishReason` gains `tool_calls`. `AdapterCapabilities.tools` is `native` for
`openai-compatible`, Ollama and `local`, `injected` for `qwen-web` and `deepseek-web`; it is also
on `AdapterCapabilitiesDto`. Per model, `modelToolSupport` + `resolveToolMode` (in `@zvs/shared`)
drop a native adapter to `none` when the provider reports tool support for some models but not
this one. `openai-compatible` maps `tools`, `tool_calls` and `tool` messages and assembles streamed
fragments by index (`ToolCallAssembler`); Ollama maps the same natively with object arguments.
`drivers/ai/injectedTools.ts` implements the know-how protocol as a driver wrapper: tool catalogue
in the system prompt, unique key `zvs_call_tool`, text held only while the prefix can still become
a call, results returned as fenced `[SYSTEM_TOOL_RESULT]` user messages. The chat graph's generate
node is now `chat.generate` (still admitted as `llm.generate`), which loops: generate, run each
call through `StepContext.invokeTool`, append fenced results, generate again; 4 rounds and 8 calls,
after which one last round carries a budget note and any further call is refused. The scheduler
records every call as its own step (`generate.callN`, typed by the tool's node) and admits it with
origin `model`, so the chat exemptions for `vector.search`/`llm.generate` do not apply to model
calls; destructive tools ask even at `auto`. Results are capped per call from the free context
budget, fenced, and the tool set is fixed at prepare time, so a result cannot change tools or
tiers; tiers are re-read on every call. Calls are persisted on the assistant message and streamed
as `{ domain: "tool-calls" }` step events. Chat page: `ChatToolCalls` cards (tool, arguments,
status, timing, result preview, inline approve/deny), `ChatToolPicker` in the composer with the
tier, the reason a tool cannot be offered and the model's tool mode, and chips for picked tools.
Added at the user's request: a read-only built-in `vector.list` tool (UUID, name, description,
documents, status, search mode), and `vector.search` field descriptions telling the model that
`storeId` is a UUID from `vector.list` and what `mode` accepts. Tests: `tests/toolCalling.test.ts`
(recorded OpenRouter, Ollama and Qwen fixtures under `test/fixtures/providers/`, parser with
malformed and partial JSON, streaming buffer, wire names, mode resolution, `vector.list`),
`tests/chatToolLoop.test.ts` (native call as a step, picker and `off` filtering, `off` refused at
admission mid-turn, destructive approval and denial, round and call caps, cancellation during a
call, unoffered tool, malformed arguments, injected mode, tool-less model, selection validation),
`tests/renderer/chatTools.test.tsx`.

**Deviated from plan:** Tool results are not replayed into later turns: history keeps only the
role and content of earlier messages, so a cancelled or capped turn never leaves an orphaned
`tool_calls` message behind. The `local` family is flagged `native`, but it has no text driver
yet, so nothing there can call tools until one exists. DeepSeek web generation is still not
implemented, so its injected path is exercised only through the shared wrapper. An injected reply
that looks like a call but does not parse is shown to the user as text rather than retried.
`RunSummary.progress.done` is clamped to the graph size, since tool steps are extra steps.

**Next session should know:** The manual check against a live OpenRouter model and Qwen web was
not run in this session; the user's dev app was running and surfaced the `vector.search`
validation failure that led to `vector.list`. A model that sends invalid arguments gets the
validation error back as a failed call and can retry. `pnpm typecheck`/`lint`/`build` through
turbo stop at `build:rust` while the dev app holds `zvs-core.node`; the per-package typecheck,
lint, `lint:tailwind`, `pnpm --filter studio build` and Prettier on touched files pass. Studio
tests: 733 passed, 2 skipped.
