---
task: TASK_025
title: Chat page — thread, composer, retrieval citations
phase: 4 — Execution kernel and Chat
status: done
depends_on: [TASK_024]
unblocks: [TASK_026]
estimate: 4 h
---

# TASK_025 — Chat page — thread, composer, retrieval citations

## Context

`ChatService` works but has no interface. This task builds the page that makes the app usable, following `/design/Chat.dc.html`, which was drawn from the Claude desktop reference the project started with.

Four columns at 1440 px: the 248 px nav rail, a 250 px conversation list, the thread, and a 292 px run-context rail showing the active model and its settings, attached stores, the system prompt and session usage. The thread itself: user messages as right-aligned cards, assistant messages full width with a small avatar, a retrieval step card above the answer reading "Searched **Product knowledge base** — 3 chunks above 0.7", source chips beneath it, and a usage line.

The composer carries mode chips (Chat / Agent), an attached-stores chip, the model and provider chips, temperature, and a send button. Streaming has to feel right — tokens appended smoothly, a stop button while generating, and the view pinned to the bottom unless the user has scrolled up.

## Why

This is the screen that makes everything built so far worth having, and the first place a real user judges the product.

## Depends on

- `TASK_024` — Chat as the first kernel front-end, with token streaming.

## Read first

- `/design/Chat.dc.html` — the whole page
- `/design/DocumentViewer.dc.html` — the same page with the viewer docked, so build the thread to survive being narrowed to 468 px (TASK_044 depends on it)
- `/docs/PASSPORT.md` §13

## Scope

**In scope**
- `features/chat/`: `ChatStore`, `ConversationVm`, organisms
- Conversation list grouped by recency
- Thread with streaming, retrieval step card, citations, usage line
- Composer with model/store/temperature chips and stop-while-generating
- Run-context rail
- Markdown rendering with code blocks via the kit's `./code-view`
- Layout that survives being narrowed to 468 px

**Out of scope** (do not do these, even if tempting)
- Clickable citations opening the viewer (TASK_044) — render them as chips now
- Agent mode — the chip exists, switching it is TASK_041
- Attachments and file drop

## Steps

1. `ChatStore` (MobX): conversations, active conversation, messages, streaming state, subscription to the run's stream via `EventRouter`. Subscribe on send, dispose on unmount — no leaked handlers, per PASSPORT §13.
2. Streaming: append token deltas to the in-progress assistant message. Batch renders with a small buffer rather than a `setState` per token, or a fast model will pin the CPU.
3. Scroll behaviour: pin to bottom while generating **unless** the user has scrolled up, in which case show a 'jump to latest' affordance. Getting this wrong makes the app feel broken during long answers.
4. Conversation list: grouped Today / Earlier this week, active row with the accent dot, new-chat button and search, exactly as drawn.
5. Thread: user bubble and assistant block per the mockup. Retrieval step card is collapsible and names the store and hit count. Source chips show path and score.
6. Markdown: render assistant content as markdown with code blocks through `@kiyotakkkka/zvs-uikit-lib/code-view`. Sanitise — model output is untrusted text and must never be injected as HTML.
7. Composer: textarea with Enter to send and Shift+Enter for newline, mode chips, store chip, model and provider chips, temperature chip, send button that becomes stop while generating.
8. Run-context rail: model card with settings chips, attached stores with their top-k and floor, the system prompt, and a session usage meter.
9. States: empty conversation, generating, cancelled (partial message marked), failed (error with retry), no provider configured (route the user to the Providers page rather than showing a dead composer).
10. Tests: store handles token batching and out-of-order guards; cancel mid-stream leaves a partial message; scroll pinning logic is unit-tested as a pure function.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Hold a real conversation against a live provider with a store attached. Check: tokens stream smoothly, citations appear, stop works mid-answer, scrolling up during generation does not yank you back down, and the thread still reads correctly when the window is narrowed to 1100 px.

## Definition of done

- [x] Page matches the mockup; thread survives narrowing to 468 px
- [x] Streaming is smooth and does not re-render per token
- [x] Retrieval card and citations render from real data
- [x] Stop, cancel-partial, error and no-provider states all handled
- [x] Markdown sanitised; model output never injected as HTML
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Root-owned MobX ChatStore and ConversationVm, recency-grouped searchable conversations, streaming thread, composer, and responsive run-context rail. Tokens flush every 32 ms; duplicate, older, foreign and post-end events are guarded. Subscriptions and timers are disposed on unmount, held events replay on remount, and completed turns refresh persisted messages. Retrieval cards and source chips use actual run results and persisted citations. Added stop/partial/error/retry/no-provider states, explicit allow-once/deny controls for kernel approvals, safe react-markdown rendering and kit code-view blocks. Preserved the workspace's Russian UI localization and added no code comments. Studio suite: 358 passed, 2 existing skips. Desktop regression passed with a local streaming HTTP provider and real native vector data, covering approvals, citations, scroll pinning, cancellation, and 1440/1100/966 px layouts (468 px thread). Development startup, pnpm typecheck, pnpm lint, pnpm build, and git diff --check passed.

**Deviated from plan:** Existing chat retrieval uses top-k 5 and a score floor of 0; the UI reports those values rather than mockup examples. Model, stores, temperature and system instructions are selected before creating a conversation because the current contract has no settings-update endpoint. The context rail becomes an accessible overlay at narrower widths. Added approval controls because existing kernel defaults can pause chat runs. Token usage remains explicitly estimated. Validation used a local HTTP provider fixture, not an external authenticated provider. Added ESNext.Disposable to renderer libs for the kit code-view's Shiki types.

**Next session should know:** TASK_026 can build on the root store's chat feature. TASK_044 can replace source chips using storeId/documentId/chunkIndex/sourcePath; citation clicks and document viewing remain out of scope. Agent mode remains disabled for TASK_041. Retry submits the failed prompt as another turn through the existing chat.send contract. Conversation configuration remains fixed after creation. e2e/chat.spec.ts exercises the production Electron bundle and saves layout screenshots under test-results. Task files remain ignored by Git, as in earlier tasks.
