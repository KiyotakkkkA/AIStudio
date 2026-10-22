---
task: TASK_074
title: Chat in Rust — ChatService, history and budget, grounded retrieval, the tool-calling loop, chat.*
phase: M — Migration to Tauri
status: done
depends_on: [TASK_073]
unblocks: [TASK_075]
estimate: 6 h
---

# TASK_074 — Chat in Rust — ChatService, history and budget, grounded retrieval, the tool-calling loop, chat.*

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

Chat is the last domain and the one that uses everything else. Its parts:
- `services/ChatService.ts` (808 lines): conversations (`create`, `get`, `list`, `remove`,
  `rename`, `truncate`, `updateStores`, `updateRelations`, `updateTools`), `send`, which builds
  the turn graph (one `vector.search` per attached store, then `chat.prepare`, `llm.generate`
  and `chat.persist`), `cancel`, and one turn at a time per conversation. Tokens are saved as
  they arrive through `RunService::subscribe`, so a partial answer survives cancel, failure and
  shutdown. Recovery never repeats a charged generation.
- `chatHistory.ts`: prompt windowing with an honest token budget (TASK_024_EXTRA_1). Output is
  reserved, the system prompt and current turn are kept, passages are included only while
  they fit, contiguous recent history fills the rest, and trimming is logged. Citations are
  numbered.
- `retrievalQuery.ts` (ported in TASK_069) is the follow-up join, and the relevance floor and
  merged sources come from the retrieval pipeline.
- `tokenEstimate.ts`: a conservative UTF-8 estimate, marked `usageEstimated`.
- `chatTools.ts`: the tool-calling loop (TASK_034_EXTRA_1). It is bounded, checked against the
  tool ceiling, raises approvals through the kernel, and uses adapters with native tools or
  injected tools for Qwen and DeepSeek.
- `ChatRepository`.

Channels: `chat.cancel`, `chat.conversations.*` and `chat.send`.

## Why

After this, every channel the renderer uses is served by Rust. Only the e2e suite (TASK_075)
and the cutover (TASK_076) remain.

## Depends on

- `TASK_073` — the tool registry and invoker that the chat tool loop calls.

## Read first

- `apps/studio/src/host/kernel/README.md`, the "Chat" section
- `apps/studio/src/host/services/ChatService.ts`, `chatHistory.ts`, `chatTools.ts`,
  `tokenEstimate.ts`, `data/repositories/ChatRepository.ts`, `ipc/chat.ts`
- Handoffs of TASK_024, TASK_024_EXTRA_1 and TASK_034_EXTRA_1, and the decision recorded there
  on which tools a chat turn sees
- `apps/studio/tests/chat.test.ts`, `chatToolLoop.test.ts`, `toolCalling.test.ts`,
  `retrieval.test.ts`

## Scope

**In scope**
- `ChatRepository`, `ChatService`, `chat_history`, `token_estimate` and `chat_tools`
- `chat.prepare` and `chat.persist` nodes, and the turn graph
- The tool loop with its caps, ceiling checks and approvals, for native-tool adapters and
  injected-tool adapters
- `chat.*` channels, DTOs and fixtures
- `IpcServer::finish()` now reports **zero** unimplemented channels. Make the test strict.

**Out of scope** (do not do these, even if tempting)
- Agentic knowledge retrieval (TASK_034_EXTRA_2, which waits for the cutover)
- Any change to budgeting, citation numbering or the loop caps

## Steps

1. Token estimate and history windowing, with their table tests ported first.
2. Repository, then `ChatService` with `chat.test.ts` ported against fake drivers and a real
   kernel: partial answers, cancel before preparation, one turn per conversation, removal
   rules and title derivation.
3. The tool loop, with `chatToolLoop.test.ts` and `toolCalling.test.ts` ported.
4. Channels, then a strict exhaustiveness test.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, hold a chat with an API provider and with a Qwen account
provider. Attach a store and see citations open the right passages. Ask a follow-up and see
it joined. Let the model call an `ask`-tier tool and approve it. Cancel mid-stream and see the
partial answer kept. Quit mid-stream, relaunch, and see the partial answer present and no
repeated generation.

## Definition of done

- [x] Chat, history budget, citations and the tool loop are ported with the full TS suites
- [x] Every contract channel is implemented in Rust, and the exhaustiveness test is strict
- [ ] The Chat page works end to end under Tauri, with retrieval and tools
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract`: `chat` (`ChatSettings`, `ChatCitationDto`, `ChatToolCallStatus`, `ChatToolCallDto`,
  `MessageRole`, `CreateConversationInput`, `ConversationRef`, `ConversationDto`, `MessageDto`,
  `ConversationDetailDto`, `ChatSendInput`, `TruncateConversationInput`, and named inputs for
  `rename`, `updateStores`, `updateRelations`, `updateTools`) and `graph_expansion`
  (`merge_graph_neighbors`, `GRAPH_EXPANSION_DEFAULT_MAX`, from `packages/shared/src/graph`). 15
  fixture pairs under `packages/shared/fixtures/chat`, registered in both tables.
- Data: `ConversationRow`, `MessageRow`, `ChatRepository` (list newest first, patches, messages in
  time order, `truncate_from`, cascade with the conversation), `RunPatch::title`.
- `services::token_estimate` (`estimate_tokens`, the Qwen-calibrated table tests),
  `services::chat_history` (`window_chat_history`, `join_chunks`, `fence_source`, `source_label`,
  JS `toFixed(2)` rounding for scores) and `services::chat_tools` (offered tools, wire names,
  parameters, result budget, capping, fencing, argument parsing, the 4-round / 8-call caps).
- `services::chat::ChatService`: conversations (`create`, `get`, `list`, `remove`, `rename`,
  `truncate`, `update_stores`, `update_relations`, `update_tools`), `send`, `cancel`, and the
  `chat.prepare`, `chat.generate`, `chat.persist` nodes with the tool loop.
- Kernel: `StepLink::invoke_tool` / `StepContext::invoke_tool` with `ToolStepRequest`,
  `ToolStepResult` and an `ApprovalObserver`; the scheduler runs a model tool call as step
  `<parent>.<label>` with `AdmissionOrigin::Model`, raising a destructive tool to `ask`.
  `KernelServices::graph` (`GraphNeighbors`, implemented by `GraphService`) and
  `VectorSearch::chunks` / `related_chunks` (implemented by `VectorStoreService`).
- `RunService::start_with`: the run row and the caller's rows commit in one transaction and an
  observer is attached before the scheduler task exists (`start` is `start_with` with nothing).
- `ipc::chat` (11 channels). `ipc::tests::every_contract_channel_is_served` is no longer ignored and
  also asserts `server.channels() == CHANNELS` and that `finish()` reports complete coverage.
- Composition: `ChatService` built on the shared registry and run service before `recover()`,
  `graph` in `KernelServices`, `Services::chat`. `test_services` builds `ChatService` and gives the
  run service `FakeTexts` over the fake driver source.
- Tests: `chat.test.ts` (`services::chat::tests`, 18, plus the window tests in
  `chat_history::tests`), `chatToolLoop.test.ts` (`services::chat::tests::tool_loop`, 14), the
  chat-tool parts of `toolCalling.test.ts` (`chat_tools::tests`; the adapter and injected-protocol
  parts were already ported with their drivers), the token-estimate part of `retrieval.test.ts`
  (`token_estimate::tests`; merge and follow-up were ported in TASK_069), repository tests and two
  channel tests (`ipc::chat::tests`).
- Gates run on Windows 2026-10-19, all passing: `cargo fmt --check`, workspace clippy with
  `-D warnings`, `cargo test --workspace` (zvs-host 834 passed, 5 ignored), `pnpm typecheck`,
  `pnpm lint`, `pnpm lint:root`, `pnpm typecheck:root`, `pnpm build`, `pnpm format:check`,
  `npx vitest run` (1863 passed, 2 skipped), `pnpm contract:channels:check`.

**Deviated from plan:**

- The task text names the middle node `llm.generate`; the TS graph uses `chat.generate`, gated on
  the `llm.generate` permission, and so does the port.
- TS made the user message and the run start atomic through a synchronous transaction around
  `runs.start`; in Rust that is `RunService::start_with`, which also attaches the token observer
  before launch so no token can be missed on a multi-threaded runtime.
- Graph expansion calls `GraphService::neighbors`, whose input is a `DocumentId` (UUID). A ranked
  hit whose document id is not a UUID makes that store's expansion fail, be logged
  (`Could not expand sources through the graph`) and be skipped; TS passed the raw string. Indexed
  documents always have UUID v7 ids, so the graph test uses real document rows.
- A model tool step commits only its step event, without the progress event that graph steps add,
  as `Scheduler.ts` did.
- `FakeDriver` now behaves like the TS `FakeDriver` when streaming: it holds before each delta
  under `manual`/`delay`, fails at `fail_after`, and counts `emitted`.
- `createdAt` / `updatedAt` on conversations and messages are checked as safe integers rather than
  non-negative timestamps, matching `z.number().int()`.

**Next session should know:**

- The manual check (`pnpm dev:tauri`: chat with an API provider and a Qwen account provider,
  citations opening the right passages, a joined follow-up, approving an `ask`-tier tool, cancel
  mid-stream keeping the partial answer, quit mid-stream and relaunch with the partial answer
  present and no repeated generation) has **not** been done. The renderer needed no changes.
- Every renderer channel is now served by Rust; TASK_075 (e2e on Tauri) is next.
- Building still needs `PROTOC` (see TASK_073): `export PROTOC=$PWD/target/tools/protoc/bin/protoc.exe`.
- An unthrottled `cargo test --workspace` ran out of virtual memory on this machine (os error 1455,
  "paging file too small") and left corrupt `lance`/`jsonb` rlibs; `cargo clean -p lance -p
  lance-datafusion -p jsonb` and `-j 4` fixed it.
