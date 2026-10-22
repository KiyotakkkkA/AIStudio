---
task: TASK_024
title: Chat as the first kernel front-end, with token streaming
phase: 4 — Execution kernel and Chat
status: done
depends_on: [TASK_023]
unblocks: [TASK_025]
estimate: 4 h
---

# TASK_024 — Chat as the first kernel front-end, with token streaming

## Context

The kernel runs graphs and enforces permissions. This task makes chat the first thing built on it — deliberately, because chat is the simplest possible front-end and proving the kernel with it de-risks scenarios and agentic sessions later.

A chat turn is a short graph: optionally retrieve from attached vector stores, generate with the selected model, persist the message. Tokens stream out as `token` events on the run's stream (TASK_005), which the renderer routes into a MobX store. Nothing about this is chat-specific except the graph shape — and that is the point.

The conversation model is small: a conversation carries a title, the selected provider and model, its settings, attached vector stores and a system prompt; messages carry role, content, token counts, timing and the retrieval citations that produced them. The citations matter — `/design/Chat.dc.html` shows them as source chips under the answer, and TASK_044 makes them clickable into the document viewer.

## Why

It turns the app from a configuration tool into something you can use, and it validates the kernel's shape before three harder front-ends are built on it.

## Depends on

- `TASK_023` — Node registry, permission admission, approval gates.

## Read first

- `/design/Chat.dc.html` — the retrieval step card, the answer, the source chips, the usage line
- `/docs/PASSPORT.md` §11 (chat is a trivial graph), §12 (streaming re-emitted as events)

## Scope

**In scope**
- `conversation` and `message` tables plus migration
- `ChatService`: start a turn, build the graph, persist messages, manage history
- Retrieval step producing citations that are persisted with the message
- Token streaming through the event channel with proper cancellation
- History windowing so a long conversation does not blow the context budget
- Channels and DTOs
- Tests with `FakeDriver`

**Out of scope** (do not do these, even if tempting)
- The chat UI (TASK_025)
- Tool calling by the model — the node exists but the model is not yet given tools (TASK_035 onward)
- Attachments and the document viewer (TASK_044)

## Steps

1. `conversation` table: id, title, providerId, modelId, settings JSON, systemPrompt, attached store ids JSON, createdAt, updatedAt. `message` table: id, conversationId, role, content, citations JSON, tokensIn, tokensOut, durationMs, runId, createdAt.
2. `ChatService.sendMessage(conversationId, text)`: persist the user message, build the graph, start a kernel run, return the run handle and stream id immediately so the UI can subscribe before tokens arrive.
3. Graph: `vector.search` per attached store (in parallel, since they are independent) → `llm.generate` → persist. Skip retrieval entirely when no store is attached rather than running an empty step.
4. Citations: map search hits to `{ storeId, documentId, sourcePath, chunkIndex, score }` and persist them on the assistant message. This is what TASK_044 turns into a clickable viewer link, so store enough to locate the passage.
5. Streaming: `llm.generate` emits `token` deltas; the final message is persisted on completion. If the run is cancelled mid-stream, persist what was generated so far and mark the message partial — losing a half-written answer is worse than showing it truncated.
6. History windowing: build the prompt from recent messages within the model's context budget, always keeping the system prompt and the current turn. Log when history is trimmed so a confusing answer can be explained later.
7. Auto-title a conversation from its first exchange, cheaply — a short prompt to the same model, or a heuristic; do not spend a full generation on it.
8. Channels: `chat.conversations.list|get|create|remove|rename`, `chat.send`, `chat.cancel`.
9. Tests with `FakeDriver`: a turn with no stores skips retrieval; with stores, citations are persisted; cancellation mid-stream persists a partial message; history windowing respects the budget and always keeps the system prompt; token events carry monotonic seq.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Send a message through the channel from DevTools against a real provider, subscribe to the stream, and watch tokens arrive. Cancel mid-generation and confirm a partial message is persisted and the run lands as cancelled.

## Definition of done

- [x] Conversations and messages persisted with citations and usage
- [x] A chat turn is a kernel run, visible in the run tables
- [x] Tokens stream as events; cancellation persists a partial message
- [x] Retrieval skipped cleanly when no store is attached
- [x] History windowing keeps the system prompt and logs trimming
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Migration 0009_chat adds conversations and messages with citations, usage, duration, run references and partial state. ChatService builds parallel vector.search nodes followed by chat.prepare, llm.generate and chat.persist; storeless turns omit retrieval. Added all seven chat channels and DTOs, host wiring, durable per-token partial persistence, cancellation propagation into retrieval embeddings, recent-history windowing with trim logs, and heuristic titles. Added 11 chat regressions plus retrieval cancellation coverage. Studio: 344 passed, 2 existing skips. Packages: 46 passed. pnpm typecheck, pnpm lint and pnpm build pass; git diff --check passes.
**Deviated from plan:** Added chat.prepare to assemble bound retrieval results and apply the context budget. Streaming ports expose neither provider usage nor a tokenizer, so token counts use a conservative UTF-8 byte estimate plus prompt overhead and are explicitly marked usageEstimated. Oversized mandatory system/current prompts fail before persistence. Native vector search cannot be interrupted; embedding is aborted and late native results are discarded. Partial answers also survive provider errors and shutdown. The real-provider DevTools manual check was not performed; channel and event behavior was exercised with FakeDriver.
**Next session should know:** TASK_025 should subscribe immediately to chat.send's { id, streamId }, distinguish text/reasoning deltas, and refresh chat.conversations.get on end. Get includes ordered messages; partial and usageEstimated must be rendered honestly. Existing permission defaults remain ask, with runs.approve/runs.deny handling approvals. One active turn per conversation; removal rejects active runs. Create accepts a discovered model row ID or external ID, and settings/systemPrompt/attachedStoreIds. Citation sourcePath plus storeId/documentId/chunkIndex locates a passage. See apps/studio/src/host/kernel/README.md for the complete handoff. Task files are ignored by the repository as before.
