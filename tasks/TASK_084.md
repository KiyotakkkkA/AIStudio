---
task: TASK_084
title: Unified answer sources — one contract and pipeline for sources of any origin
phase: B — Dialog notes
status: todo
depends_on: []
unblocks: [TASK_085]
estimate: 6 h
---

# TASK_084 — Unified answer sources

## Context

Answer sources are modelled once per origin:

- vector retrieval: `MessageDto.citations: Vec<ChatCitationDto>`, column `citations`;
- web search: `MessageDto.web_sources: Vec<WebSourceDto>`, column `web_sources`
  (migration `0024_message_web_sources`).

Each origin has its own field, column, validation, carry/compaction handling, export path,
renderer props (`ChatMarkdown`, `ChatSources`, `WebSourceCard`, `citationMarkers`) and CLI
rendering. TASK_085 would add a third copy for notes. Replace this with one source model
that every origin plugs into.

## Why

A new source origin should cost a variant, a collector call and a card — not a new DTO
field, DB column, migration, compaction branch and renderer prop.

## Read first

- `docs/PASSPORT.md` (contract, chat, persistence), `CONTRIBUTING.md`, `CONVENTIONS.md`.
- `crates/zvs-contract/src/chat.rs`, `packages/shared/src/chat.ts`, golden fixtures.
- `crates/zvs-host/src/data/{schema,repositories}/chat.rs`, `data/migrate.rs`.
- `services/{chat,tool_loop,chat_carry}.rs`, `services/chat/compact.rs`, `kernel/nodes.rs`,
  `chat_export.rs` — every place that produces or copies sources.
- Renderer `features/chat/{ChatMarkdown,ChatSources,ChatMessage,ChatStore,WebSourceCard,
  webSources,citationMarkers}`; CLI source rendering in `crates/zvs-cli/src/`.

## Scope

### 1. Contract

- One tagged union, `ChatSourceDto`, serialized with a `kind` discriminator:
  `{ "kind": "web", ... }`, `{ "kind": "vector", ... }`. Variant payloads keep today's
  `WebSourceDto` and `ChatCitationDto` fields unchanged.
- `MessageDto.sources: Vec<ChatSourceDto>` replaces `citations` and `webSources`.
  One shared limit for the whole list; per-variant validation stays in the variant.
- Rust type, Zod discriminated union, golden fixtures for every variant.
- Inline markers keep their current meaning: a web marker `n` is the n-th `web` source and a
  vector marker `n` is the n-th `vector` source, counted within the kind. Answers stored
  before this task must render the same markers on the same sources.

### 2. Persistence

- One `sources` JSON column. A migration folds existing `citations` and `web_sources` into
  it, preserving order within each kind, then drops the old columns. Test the migration on
  rows with vector only, web only, both and neither.

### 3. Host pipeline

- A per-turn `SourceSink` that origins push into (`push(ChatSourceDto)`), with dedup by a
  per-kind key (web: URL; vector: store, document and chunk). Web search and vector
  retrieval write through it; the assistant message takes its `sources` from it.
- Carry, compaction, branching, export and the kernel node path copy `sources` as one list
  instead of per-origin fields.
- Adding an origin means adding a variant, its validation and a dedup key; nothing else in
  the pipeline changes.

### 4. Renderer and CLI

- `ChatSources` renders one list from `message.sources`, dispatching by `kind` through a
  registry of `{ icon, label, card, open }` entries. Web and vector entries reproduce
  today's cards and actions.
- `ChatMarkdown` receives `sources` and resolves markers through a per-kind index; no
  regex parsing of prose beyond today's marker syntax.
- CLI renders the same list by `kind`.
- No visible change to existing answers.

## Out of scope

- New source origins (notes arrive in TASK_085).
- Changing web or vector marker syntax, retrieval, ranking or card design.

## Steps

1. PASSPORT: describe the unified source model and the extension recipe.
2. Contract, Zod, fixtures.
3. Migration and repository.
4. `SourceSink`; move web and vector producers and all copy paths onto `sources`.
5. Renderer registry, marker resolution, CLI.
6. Tests and gates.

## Verify

- Fixtures byte-equal for each variant; validation per variant and for the list limit.
- Migration keeps order and content for all row shapes.
- Dedup per kind; carry, compaction, branch and export keep `sources` intact.
- Markers in pre-migration answers resolve to the same sources (store test with fakes).
- Existing `chatWebSources` tests pass after moving to `sources`.

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm build
pnpm test:e2e
```

Manual checks: open an old dialog with web and vector sources — cards and inline markers are
unchanged; a new answer with both kinds shows one source list; export it; branch it.

## Definition of done

- [ ] `ChatSourceDto` union replaces `citations` and `webSources` in Rust, Zod and fixtures
- [ ] Migration folds old columns into `sources` without loss
- [ ] All producers and copy paths use `SourceSink` / `sources`
- [ ] Renderer and CLI render sources through one kind registry
- [ ] Existing answers look and behave the same
- [ ] PASSPORT documents how to add an origin
- [ ] Gates pass; handoff filled, status `done`, INDEX.md ticked; one `TASK_084: ...` commit

## Handoff

**Built:**
- _Fill after implementation._

**Deviated from plan:**
- _Fill after implementation._

**Next session should know:**
- _Record verification results and platform exclusions._
