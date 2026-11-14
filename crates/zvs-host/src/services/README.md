# services

The business logic. `Services` (`mod.rs`) is the single object every IPC handler receives; it
holds one instance of each service, built once by the composition root. Services own their
rules and talk to the database through repositories (`../data`), to the outside through
drivers (`../drivers`), and to each other through explicit handles, never through globals.

## Map

| Area                        | Files                                                                                                                                                                                                                                                     |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chat (see `chat/README.md`) | `chat.rs`, `chat/`, `chat_prompt.rs`, `chat_history.rs`, `chat_carry.rs`, `chat_reply.rs`, `chat_tools.rs`, `tool_loop.rs`, `tool_arguments.rs`, `chat_ask.rs`, `chat_goal.rs`, `chat_memory.rs`, `chat_notes.rs`, `chat_plugins.rs`, `token_estimate.rs` |
| Runs and permissions        | `runs.rs` + `runs/`, `permissions.rs`, `approvals.rs`, `questions.rs`, `retention.rs`, `subagents.rs`                                                                                                                                                     |
| Providers and accounts      | `providers.rs`, `probe_outcome.rs`, `provider_status.rs`, `health_check.rs`, `accounts.rs`                                                                                                                                                                |
| Secrets                     | `crypto.rs` (`CryptoService`, `Plaintext`), `secrets.rs`, `secret_policy.rs`                                                                                                                                                                              |
| Vector stores               | `vector_stores.rs`, `vector_sources.rs`, `vector_eval.rs`, `vector_health.rs`, `rerank.rs`, `retrieval_merge.rs`, `retrieval_query.rs`, `retrieval_metrics.rs`, `graph.rs`                                                                                |
| MCP                         | `mcp.rs` + `mcp/` (supervision, discovery, OAuth grants; protocol is `../drivers/mcp`)                                                                                                                                                                    |
| Memory and profile          | `memory.rs`, `profile.rs`                                                                                                                                                                                                                                 |
| Settings, storage, system   | `settings.rs`, `storage.rs`, `stored.rs`, `system.rs`                                                                                                                                                                                                     |

The crate README documents most of these contracts (Secrets, Providers, Accounts, Vector
stores and graph, Kernel, Tools and MCP, Chat); the algorithms below are the ones worth
reading before changing the code.

## Retrieval merge (`retrieval_merge.rs`)

`merge_retrieval(hits_by_store, { per_document, limit })` combines ranked hits from several
stores into one list:

1. Drop hits below each store's relevance floor (`below_relevance_floor`). Hits found only by
   keyword search are exempt: their scores are on a different scale from vector similarity.
2. Give every hit a sort key equal to the best score at or after its rank in its own store
   (a running max from the bottom), so a store's own ranking is never inverted by the merge.
3. Sort all candidates by key, then by rank within the store, then by store order.
4. Walk the list and skip a hit if a chunk of the same document within ±1 of it was already
   taken (neighbours will be expanded later anyway), if that document already has
   `per_document` hits, or once `limit` hits are taken.

`MergeStats` counts fetched, below floor, duplicates, over cap and over limit for the
retrieval panel.

## Follow-up queries (`retrieval_query.rs`)

A short question (fewer than 8 words) or one opening with a pronoun or connective
(`а`, `и`, `но`, `или`, …) is a follow-up, unless it is only an acknowledgement ("спасибо", "ок").
For a follow-up the search query is the previous user question (first 2000 characters)
plus the new text, so "а во втором пункте?" retrieves the same document.

## Token counting (`token_estimate.rs`)

`estimate_tokens` uses the `o200k_base` BPE (tiktoken) for every model: close enough for
budgeting across vendors and deterministic. `truncate_tokens(text, limit)` binary-searches
the longest prefix (on char boundaries) that fits.

## Retrieval metrics (`retrieval_metrics.rs`, `vector_eval.rs`)

Evaluation cases (query + expected documents) are run in every mode the store supports;
documents are deduplicated in rank order and each case is scored for hit@1, hit@5,
recall@k and reciprocal rank; the run stores the averages (hit@1, hit@5, recall@k, MRR) as
the store's `last_eval`. A failed case is reported, not fatal.

## Secrets (`secrets.rs`, `crypto.rs`)

Secret values are sealed by `CryptoService` over the platform `CryptoBackend` (DPAPI on
Windows) and never leave the host in a DTO; DTOs carry a masked hint (`secret_policy`).
`Plaintext` zeroes its buffer on drop. Consumers (providers, MCP headers, plugins) hold a
secret **id** and resolve it at the moment of use.
