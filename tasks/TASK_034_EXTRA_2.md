---
task: TASK_034_EXTRA_2
title: Agentic knowledge retrieval — the model searches the knowledge base itself
phase: 6 — Tools, MCP and Skills (extension)
status: done
depends_on: [TASK_034_EXTRA_1, TASK_021_EXTRA_6, TASK_076]
unblocks: []
estimate: 4 h
---

# TASK_034_EXTRA_2 — Agentic knowledge retrieval — the model searches the knowledge base itself

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

By this point the retrieval pipeline is complete — hybrid search with rank fusion, reranking,
structure-aware chunks with page and section, neighbour and graph expansion, a relevance floor and
an evaluation set (`TASK_024_EXTRA_1` through `TASK_021_EXTRA_6`) — and chat can call tools in a
bounded, permission-checked loop (`TASK_034_EXTRA_1`).

Chat still retrieves the same way it did in TASK_024: **once, before generation, with the user's
words as the query**, whether or not the turn needs it. That fails in predictable ways: a greeting
still triggers a search; a question that needs two lookups ("compare the leave policy with the
sick-leave policy") gets one; a vague question gets a vague query; and after reading the first
results the model cannot go back for more.

The registry already lists `vector.search` as a kernel tool, but its interface is the raw node:
a store id, a query string, `k`, `minScore`. That is the wrong shape for a model — it does not know
store ids, does not know what each store contains, and should not be tuning `k`.

## Why

Letting the model decide when and what to search is the step from "retrieval bolted onto chat" to
a knowledge instrument: it writes its own queries, searches again when the first results are thin,
reads a whole section when a fragment is not enough, and skips retrieval entirely when the turn
does not need it. Every pipeline improvement from the previous tasks is reused; this task changes
who drives it.

## Depends on

- `TASK_034_EXTRA_1` — the tool-calling loop, adapters and approvals.
- `TASK_021_EXTRA_6` — the full search pipeline and chunk/neighbour fetch the tools call.

## Read first

- `apps/studio/src/host/tools/kernelTools.ts` — how kernel nodes become registry tools
- `apps/studio/src/host/services/ChatService.ts` — the pre-generation retrieval this task makes optional
- `apps/studio/src/host/services/chatHistory.ts` — budgeting and citation numbering
- TASK_034_EXTRA_1 handoff — the loop's message shapes and caps

## ❓ DECISION — default retrieval mode for a conversation with stores attached

- **(a)** `auto` (pre-search as today) stays default; `agentic` is opt-in per conversation.
- **(b)** `agentic` is default when the model supports tools; `auto` otherwise.
- **(c)** `hybrid`: a cheap pre-search always runs and its top results are offered, and the model
  may search further with tools.

Fallback if unavailable: **(c)** for tool-capable models, **(a)** otherwise — it never answers
worse than today and lets the model go further when it needs to.

## Scope

**In scope**
- Model-facing tools (registered as built-in, read-only tier, scoped to the conversation's attached
  stores — never other stores):
  - `knowledge_search(query, store?, limit?)` — runs the full pipeline, returns numbered passages
    with path, section, page and score, already floored and deduplicated.
  - `knowledge_read(documentId, section? | chunkRange?)` — returns a contiguous span of a document
    for when a fragment is not enough, size-capped.
  - `knowledge_list()` — attached stores with name, description and document count, so the model
    knows what it can search.
- Tool descriptions written for models: when to search, to search again with different wording,
  to cite passages as `[n]`, to say when the knowledge base has no answer.
- Retrieval mode per conversation (`auto` / `agentic` / `hybrid`) per the decision, in the composer
  next to the attached stores.
- Citations from tool results join the same `ChatCitationDto` list as pre-search citations, with
  stable numbering across rounds so `[n]` in the answer always resolves on the Chat page.
- Budget: tool results share the context budget from `TASK_024_EXTRA_1`; older tool results are
  compacted (passages dropped, citations kept) when a new round needs room.
- Run detail shows each search the model made, its query, and what came back — the debugging view
  for "why did it answer that".
- Evaluation: the `TASK_021_EXTRA_5` set gains an end-to-end mode that asks each question in a
  scratch conversation and checks whether an expected document was cited. Report `auto` vs
  `agentic` side by side.

**Out of scope** (do not do these, even if tempting)
- Writing to the knowledge base from chat (index, add source) — read-only tools only
- Multi-agent or planner/executor splits — TASK_040
- Web search as a retrieval source

## Steps

1. Three tools, registered as built-ins with schemas and model-facing descriptions; tests on
   scoping (no access to unattached stores) and size caps.
2. Retrieval mode setting, migration, composer control; `ChatService` honours it.
3. Citation numbering across tool rounds; Chat page resolves `[n]` from tool results.
4. Budget compaction of older tool results.
5. Run detail rendering of model-issued searches.
6. End-to-end evaluation mode; record `auto` vs `agentic` numbers in the handoff.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: in `agentic` mode, a greeting triggers no search; "compare A with B" triggers two
searches; a question with no answer in the store gets an honest "not found" instead of an
invented answer. Citations from the second search resolve on the Chat page.

## Definition of done

- [x] The model can list, search and read attached stores, and only those
- [x] Retrieval mode per conversation works per the decision
- [x] Citations stay correct across multiple tool rounds
- [ ] End-to-end evaluation numbers for `auto` vs `agentic` in the handoff — the mode is built and tested; live numbers need a run against a real model (see below)
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Decision: fallback (c). `RetrievalMode` (`auto` / `agentic` / `hybrid`) on the
conversation (`conversation.retrieval_mode`, migration `0004_conversation_retrieval_mode`, default
`hybrid`), `CreateConversationInput.retrievalMode`, `ConversationDto.retrievalMode` and the
`chat.conversations.updateRetrieval` channel. Three built-in read-only kernel nodes in
`kernel/knowledge_nodes.rs`, registered with the core nodes and surfaced as tools with
model-facing descriptions: `knowledge.search` (query, optional store name or id, limit 1–10; runs
the full store search per attached store, then `merge_retrieval` with each store's floor, dedupe
and the per-document cap; passages carry store, documentId, path, section, page, chunk and score,
each capped at 1500 chars), `knowledge.read` (documentId plus a section title or a chunk range;
at most 12 chunks / 6000 chars, joined with the overlap-aware `join_chunks`) and `knowledge.list`.
Wire names are `knowledge_search`, `knowledge_read`, `knowledge_list`. Scoping: every node takes a
`scope` that is hidden from the schema (`#[schemars(skip)]`) and is always overwritten by the chat
loop with the conversation's attached stores, so a model cannot widen it; an empty scope, a store
outside it or a document outside it is refused. `VectorSearch` gained `document()`.
`chat.prepare` decides per turn: knowledge tools are offered in `agentic` and `hybrid` when stores
are attached, the adapter/model can call tools and the tools are not `off`; the system prompt then
carries `KNOWLEDGE_INSTRUCTION` and a catalogue of the attached stores. `hybrid` keeps the
pre-search but injects only its top 3; `agentic` drops the pre-search nodes from the graph, and
when tools turn out to be unusable `chat.prepare` runs the pre-search itself. Citations: one
citation book per turn starts from the pre-search citations; each search passage and read span is
numbered into it (a chunk already cited keeps its number, a read reuses the number of any cited
chunk in its span), the model sees `<source id="n" …>` blocks, and the book is written to the
assistant message after every knowledge call, so `[n]` resolves from `message.citations`. Budget:
each knowledge result also records a compact form (numbers, paths and documentIds, no text); before
a round, if the messages exceed the context minus output, system prompt and tool specs, results
from earlier rounds are replaced by their compact form, oldest first, with a log line. Chat page:
a «Поиск по знаниям» segmented control in the stores dialog with a hint that names the tool-less
fallback, `[n]` markers rendered as chips that open the sources panel (numbers shown there too),
and knowledge calls summarised by query and passage count. Run detail (Tasks) shows a model
search step's query and its passages above the raw JSON. Evaluation: `chat.evaluate`
(`ChatEvalInput` → `ChatEvalRunDto`) asks every eval case of a store in a scratch conversation per
mode (default `auto` then `agentic`), records the cited documents, hit, number of searches and any
error, deletes the conversation, and reports hit rate and searches per question per mode; the
«Проверка качества» section has a «Сквозная проверка в чате» panel with a model picker and the
side-by-side table. Tests: `kernel/knowledge_nodes/tests.rs` (read window, section span, store
scoping), `services/chat/tests/knowledge.rs` (greeting searches nothing; a comparison searches
twice with numbering across rounds; out-of-scope store refused and a forged scope ignored; empty
result tells the model to say there is no answer; hybrid numbering after the up-front sources;
`auto` offers no knowledge tools; tool-less `agentic` falls back to the pre-search; read reuses a
cited number; compaction order; the end-to-end evaluation), `tests/renderer/knowledgeRetrieval.test.ts`.

Also done in this session at the user's request: OAuth for remote MCP servers — see INDEX
decision TASK_032 and the next block.

**Deviated from plan:** No `auto` vs `agentic` numbers are recorded: the evaluation needs a live
text model and an indexed store with eval cases, and none was run in this session. The read tool
scans at most 400 chunks to find a section. The eval runs sequentially inside one IPC call, so a
large case set keeps the panel busy until it finishes. The pre-existing tailwind-canonical
warnings in `ChatToolPicker.tsx` were auto-fixed so `lint:tailwind` passes.

**Next session should know:** Record the evaluation numbers by opening a store with eval cases,
«Проверка качества» → «Сквозная проверка в чате», with a tool-capable model. The manual checks
(greeting, «compare A with B», an unanswerable question) were covered by tests with a scripted
model, not against a live one. MCP OAuth: an http server that answers 401 with a Bearer
`WWW-Authenticate` challenge now fails with `AUTH_REQUIRED` and `oauth: "required"`; «Войти» runs
`mcp.signIn` (probe → protected-resource metadata, upgrading an `http://` metadata link on an
https host → authorization-server metadata → dynamic registration with a `127.0.0.1` loopback
redirect, reusing the client and port on the next sign-in → PKCE S256 in the system browser →
code exchange with `resource`), stores the grant encrypted in `mcp_oauth` (migration
`0003_mcp_oauth`) and reconnects; tokens refresh a minute before expiry and once on a 401.
`mcp.cancelSignIn` and `mcp.signOut` exist. Checked against `testix.zvsd.ru/mcp` → `id.zvsd.ru`
discovery and `api.zvsd.ru/oauth/register` by hand; the browser login itself was not completed in
this session. Gates: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
--workspace` (zvs-host 875 passed), `pnpm typecheck`, `pnpm lint`, `pnpm lint:tailwind`,
`pnpm test` (1311 passed), `pnpm build`, `contract:channels --check` all pass.
