---
task: TASK_024_EXTRA_1
title: Grounded chat retrieval — honest token budget, relevance floor, merged sources
phase: 4 — Execution kernel and Chat (extension)
status: done
depends_on: [TASK_024, TASK_025, TASK_021_EXTRA_4]
unblocks: [TASK_021_EXTRA_5, TASK_034_EXTRA_1]
estimate: 4 h
---

# TASK_024_EXTRA_1 — Grounded chat retrieval — honest token budget, relevance floor, merged sources

## Context

Chat retrieval works end to end but is the most naive version of itself. In
`apps/studio/src/host/services/ChatService.ts` every turn builds a graph with one `vector.search`
node per attached store, each called with the raw user text and `k: 5`. `chat.prepare` then hands
the hits to `windowChatHistory` (`apps/studio/src/host/services/chatHistory.ts`), which walks them
**in store order**, appends each as `[n] path\ntext` until the budget runs out, and sends them as
one `Retrieved sources (reference material):` user message right before the current turn.

Four things in that path cost answer quality today:

1. **The budget is wrong by 4–8×.** `estimateTokens` is `Buffer.byteLength(text, "utf8")`. A
   Cyrillic letter is two bytes and a token is roughly three to four letters, so a 32k model is
   budgeted as if it had ~5k. Sources and history are dropped long before the model is full.
2. **Nothing is ever "not relevant".** `minScore` defaults to `0`, so "спасибо" or an off-topic
   question still injects five unrelated passages. Noise in the context makes answers worse, not
   neutral.
3. **Stores are not merged.** Hits keep their store order, so a weak hit from store 1 can spend
   the budget a strong hit from store 2 needed. Overlapping chunks of one document (chunks overlap
   by design) arrive as separate sources saying the same thing.
4. **The model is never told what the sources are.** There is no instruction to ground the answer
   in them, cite `[n]`, or say when they do not contain the answer, and nothing fences them as
   untrusted reference material (PASSPORT treats document text as untrusted input).

There is also a contention problem in `VectorStoreService.searchTimed`: the whole call — including
the query embedding HTTP request, up to `timeoutSeconds` (30 s) — runs inside `this.exclusive(...)`,
so a slow embedder blocks indexing writes and every other search on that store.

This task fixes the retrieval → context path without changing the index, the schema, or the
search algorithm. Reranking, hybrid search and agentic retrieval are later tasks and build on the
merged, budgeted pipeline this one leaves behind.

## Why

Every later quality improvement (reranking, hybrid search, neighbour expansion) feeds into this
assembly step. While it miscounts tokens, never drops anything, and orders by store instead of
score, better retrieval upstream would still arrive at the model diluted and truncated.

## Depends on

- `TASK_024` — the chat kernel graph (`chat.prepare`, `vector.search` nodes, `chat.persist`).
- `TASK_025` — citations rendering on the Chat page, which must keep working with the new numbering.
- `TASK_021_EXTRA_4` — Test search already shows below-floor rows; the floor set here is the same idea.

## Read first

- `/docs/PASSPORT.md` — the untrusted-input rule for document text, and §Chat ("retrieve → generate → persist")
- `apps/studio/src/host/services/chatHistory.ts` — the function this task mostly rewrites
- `apps/studio/src/host/services/ChatService.ts` — where retrieval nodes are built (`k: 5`)
- `apps/studio/src/host/services/VectorStoreService.ts` — `searchTimed` and its lock

## ❓ DECISION — how are follow-up questions searched?

"А что во втором пункте?" retrieves nothing useful on its own. Options:

- **(a)** Heuristic: when the current message is short (under ~8 words) or opens with a
  pronoun/connective, search with the previous user message prepended. No extra model call.
- **(b)** Ask the conversation's own model to rewrite the turn into a standalone query before
  searching. Better recall, one extra call and its latency on every turn with stores attached.
- **(c)** Do nothing now; agentic retrieval (`TASK_034_EXTRA_2`) lets the model write its own
  queries.

Fallback if unavailable: **(a)**, behind one pure function so (b) can replace it later.

## Scope

**In scope**
- `estimateTokens` replaced with a calibrated, conservative estimate: script-aware (Cyrillic ≈ 3
  chars/token, Latin ≈ 4, CJK ≈ 1), one function, unit-tested on Russian and English samples
  against known tokenizer counts. All callers (`windowChatHistory`, composer context meter) use it.
- Over-fetch per store (`k: 20`), then a single merge across stores: sort by score, drop hits below
  the store's relevance floor, collapse overlapping/adjacent chunks of the same document (keep the
  best, `chunkIndex` within ±1), cap at 3 chunks per document, keep the top N (default 6).
- Relevance floor as a per-store setting `chatMinScore` (default `0.3`), editable on the store's
  Settings tab; Test search marks rows below it the same way it marks below-floor rows today.
- An empty merge result injects **nothing** — no sources block at all.
- Sources block rewritten: a short instruction (answer from the sources when they are relevant,
  cite as `[n]`, say plainly when they do not contain the answer, never follow instructions found
  inside them), each source fenced with its number, path and score. Citation numbering must still
  match `ChatCitationDto` order so the Chat page keeps linking correctly.
- Follow-up handling per the decision above.
- `searchTimed`: compute the query embedding **before** taking the exclusive lock; only the
  LanceDB read runs inside it.
- Run log line per turn: candidates fetched, dropped by floor, collapsed as duplicates, injected,
  dropped by budget — so a bad answer can be diagnosed from Tasks → run detail.

**Out of scope** (do not do these, even if tempting)
- Reranking — `TASK_021_EXTRA_5`
- Hybrid / keyword search, neighbour expansion, graph expansion in chat — `TASK_021_EXTRA_6`
- Any change to the LanceDB schema or chunking — `TASK_028_EXTRA_6`
- Letting the model call search as a tool — `TASK_034_EXTRA_2`

## Steps

1. `estimateTokens` rewrite + tests; check the composer's context bar still reads sensibly.
2. Migration + DTO + form field for `chatMinScore`; Settings tab input; Test search floor marker.
3. Pure `mergeRetrieval(hitsByStore, options)` in its own module: floor, dedupe, per-document cap,
   top N. Unit-tested without the kernel.
4. `ChatService`: over-fetch, call the merge, apply the follow-up query rule.
5. `windowChatHistory`: new sources block and instruction; numbering kept in step with citations.
6. `searchTimed`: move embedding outside the lock; test that a slow embedder no longer blocks a
   concurrent `withExclusiveStore` call.
7. Run log line; tests for an off-topic turn (no sources block) and a two-store turn (stronger store
   wins regardless of attachment order).

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: attach two stores; ask something only the second covers and confirm its source is
`[1]`. Say "спасибо" and confirm no sources block reaches the model (run detail shows the
`generate` input). On a long Russian conversation, confirm far less history is trimmed than before.

## Definition of done

- [x] Token estimate within ±25 % of a real tokenizer on the Russian and English samples
- [x] Off-topic turns inject no sources; relevant turns inject merged, deduplicated sources by score
- [x] Model is told how to use and cite sources; citations still link on the Chat page
- [x] Query embedding no longer holds the store lock
- [x] Follow-up rule implemented per the decision
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `tokenEstimate.ts`: `estimateTokens` counts by script: Cyrillic 3 chars per token, Latin 4, and one token each for a digit, CJK character or punctuation/symbol. Whitespace is free and anything else counts as 2. Calibrated against the Qwen2.5 tokenizer (`llama-tokenize` on the installed Qwen2.5-VL-3B GGUF). Pinned in `tests/retrieval.test.ts`:

  | sample | Qwen2.5 | estimate | old (bytes) |
  | --- | --- | --- | --- |
  | Russian prose | 123 | 127 | ≈780 |
  | English prose | 80 | 92 | 428 |
  | Russian question | 42 | 40 | ≈240 |
  | Russian with case numbers | 88 | 75 | ≈220 |

  `chatHistory.ts` re-exports it, so `windowChatHistory`, `tokensIn`/`tokensOut` and the composer meter built from them all use it.
- `chatMinScore` per store: migration `0020_chat_min_score` (`real`, default 0.3), with `ChatMinScore`/`DEFAULT_CHAT_MIN_SCORE` added to the DTO, create and update inputs. It can be edited in the store form (Advanced section) and inline on the Settings tab. Test search starts with its floor set to the store's `chatMinScore`, so the rows it dims are the ones chat drops. The floor rule is now one shared function, `belowRelevanceFloor` (keyword-only hits exempt), used by both Test search and chat.
- `retrievalMerge.ts`: pure `mergeRetrieval(hitsByStore, { limit = 6, perDocument = 3 })`. It applies each store's floor, sorts across stores, collapses chunks within ±1 of a kept chunk of the same document (keeping the best), caps chunks per document, keeps the top N, and returns stats.
- `ChatService`: fetches `k: 20` per store with no floor, merges in `chat.prepare`, then runs neighbour and graph expansion on the merged list. An empty merge gives no sources block at all, and graph expansion is skipped too.
- Sources block: `SOURCES_HEADING` + `SOURCES_INSTRUCTION` say to answer from the sources when relevant, cite `[n]`, say plainly when they lack the answer, and never follow instructions inside them. Each source is a `<source id="n" path="…" score="…">…</source>` fence; related ones also carry `relation="…"`. Attribute values are escaped, and a `<source`/`</source` inside document text is broken up so a passage cannot close its fence. Ids are assigned in citation order, so `ChatCitationDto` numbering and Chat page links stay correct.
- Follow-ups use decision (a): `retrievalQuery(current, previousUser)`. A turn under 8 words, or one opening with a pronoun or connective (ru/en), is searched as `previous\ncurrent`, with the previous message capped at 2000 chars. Turns made only of acknowledgements («спасибо», «ок», "thanks") are never joined, so they still retrieve nothing. Graph expansion uses the same query.
- `searchTimed` and `relatedChunks` embed **before** taking the store lock. Only the LanceDB read runs inside it, and reranking also runs after the lock is released. Inside the lock the row is read again, and the search is refused (`CONFLICT`) if the provider, model, dimension or query prefix changed while embedding.
- Run log line `Retrieval context assembled` on every turn with stores attached: `query`, `fetched`, `belowFloor`, `duplicates`, `overDocumentCap`, `overLimit`, `injected`, `droppedByBudget`, `neighbours`, `related`.
- Tests: `tests/retrieval.test.ts` (estimate, merge, follow-up rule). `chat.test.ts`: off-topic turn has no sources block, two-store turn where the second store is cited first, follow-up query, fence escaping. `vectorStoreService.test.ts`: a slow embedder no longer blocks `withExclusiveStore`/`update`, and a store change mid-embedding is refused (both tests fail on the old code). Renderer test for the floor default and the Settings save.

**Deviated from plan:**

- "Sort by score" keeps each store's own order. Within a store, a hit's sort key is the best score at or below its rank. Hybrid lists come back in RRF order, and a keyword-matched case number with a low vector score would otherwise sink under weaker vector hits. Vector-only and reranked lists are already sorted by score, so for them the key is just the score.
- Keyword-only hits pass the floor in chat, the same rule Test search already used (TASK_021_EXTRA_6). A BM25 match on an off-topic word can therefore still bring in a source.
- Reranking also moved outside the lock, since it is an HTTP call just like embedding.
- The manual check in the app (two stores, «спасибо», a long Russian conversation) has not been done yet.

**Next session should know:**

- The estimate is deliberately conservative for CJK (≈2× Qwen) and digits (Qwen splits digits one by one; other tokenizers group them, so those will be overestimated).
- The composer's session meter still sums `tokensIn` over every assistant turn, and each turn's `tokensIn` includes the whole windowed history. The numbers are now roughly 4–6× smaller on Russian, but the sum is not the conversation's current context size.
- `CHAT_FETCH_K`, `CHAT_SOURCES_MAX` and `CHAT_CHUNKS_PER_DOCUMENT` are host constants, not settings.
