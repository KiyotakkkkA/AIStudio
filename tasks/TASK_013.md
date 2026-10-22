---
task: TASK_013
title: Provider and model schema, repositories, migration
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_012]
unblocks: [TASK_014]
estimate: 2 h
---

# TASK_013 — Provider and model schema, repositories, migration

## Context

Secrets exist and can be managed (Phase 1). A **provider** is the thing that uses one: a
configured connection to an AI vendor — Ollama, OpenRouter, Anthropic, Mistral — carrying a
base URL, a reference to a stored secret, and model settings.

The key structural insight from the design (`/design/Providers.dc.html`) is that the three
tabs — Text Generation, Embeddings, Image Generation — are **one table with a capability
filter**, not three concepts. The sticky note on the canvas says so explicitly. A provider row
declares which capabilities it serves; the tabs filter on that. Building three tables here
would be a mistake that costs three times the code forever.

The other half is **models**. A provider does not hardcode its model list — the user clicks
"Test connection" and the app stores whatever the vendor reported (TASK_015). So models are a
child table, refreshed on each successful probe, carrying what the UI shows on the cards:
family, context window, capability flags, availability.

## Why

TASK_014 and TASK_015 need somewhere to put a connection and its discovered models. Everything
that generates text, embeds a chunk or draws an image resolves a provider row first.

## Depends on

- `TASK_012` — Phase 1 complete; `secret` table exists to reference.

## Read first

- `/design/Providers.dc.html` — the connection form, the model settings block, and the model
  cards. The card fields tell you what the `model` table must store.
- `/docs/PASSPORT.md` §9 (aggregates), §8 (repository rules), §12 (AI layer — the provider
  registry this feeds).

## Scope

**In scope**
- `provider` table, with capabilities as a queryable set
- `model` table as a child of provider
- Migration generated and committed
- `ProviderRepository`, `ModelRepository`
- Usage registration against `secret_usage` from TASK_009
- Repository tests

**Out of scope**
- The AI SDK and any driver code (TASK_014)
- Test-connection and discovery logic (TASK_015)
- UI (TASK_016)

## Steps

1. `host/data/schema/provider.ts`:
   - `id` text pk, `kind` text (`ollama` | `openrouter` | `anthropic` | `mistral` |
     `openai-compatible`), `name` text, `baseUrl` text
   - `secretId` text, nullable, fk → `secret.id` (`ON DELETE RESTRICT`, matching TASK_009's
     rule — nullable because a local Ollama needs no key)
   - `capabilities` text, not null — JSON array of `text` | `embedding` | `image`. Also
     write three derived boolean columns (`capText`, `capEmbedding`, `capImage`) with
     indices, so the tab filter is an index lookup rather than a JSON scan. Keep them in
     sync in the repository, in one place.
   - `enabled` integer (bool), `settings` text (JSON: temperature, topK, topP,
     maxOutputTokens, timeoutSeconds), `defaultModelId` text nullable
   - `status` text (`unknown` | `ok` | `degraded` | `failed`), `statusDetail` text nullable,
     `lastProbeAt` integer nullable, `lastLatencyMs` integer nullable
   - `createdAt`, `updatedAt`
   - Unique index on `(kind, name)`.
2. `host/data/schema/model.ts`:
   - `id` text pk, `providerId` fk (`ON DELETE CASCADE` — a model has no meaning without its
     provider), `externalId` text (the vendor's own id, e.g. `gpt-oss:120b`)
   - `displayName` text, `family` text nullable, `contextWindow` integer nullable,
     `maxOutput` integer nullable, `sizeBytes` integer nullable
   - `capabilities` text — JSON array of flags the cards show: `tools`, `vision`,
     `streaming`, `reasoning`, `code`
   - `available` integer (bool) and `unavailableReason` text nullable — the mockup shows a
     dimmed "Not available on this plan" card, so this is real state, not an omission
   - `discoveredAt` integer
   - Unique index on `(providerId, externalId)`.
3. Generate the migration, read the SQL, commit it.
4. `ProviderRepository`: `list(filter?: { capability?, enabled? })`,
   `findById`, `create`, `update`, `remove`, `setStatus(id, status, detail, latency)`,
   `setDefaultModel(id, modelId)`. Keep `capabilities` JSON and the three boolean columns
   consistent inside `create`/`update` — never leave that to callers.
5. `ModelRepository`: `listByProvider(providerId)`, `replaceForProvider(providerId, models)`
   — a transactional replace, since discovery returns the full list each time and stale rows
   must not linger. `findById`.
6. Wire `secret_usage`: creating or repointing a provider registers usage
   (`kind: 'provider'`); removing or clearing the reference removes it. Do this in the
   repository layer's transaction so a provider can never exist with unregistered usage —
   otherwise TASK_009's delete protection silently stops working.
7. Tests against `tempDb`: capability filtering returns the right providers; unique
   constraints fire; `replaceForProvider` removes stale models atomically; deleting a
   provider cascades its models; deleting a secret in use by a provider is still rejected.
8. Commit: `TASK_013: provider and model schema`.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: launch the app, confirm the migration applies once and the log names it. Inspect
the tables in a SQLite browser; confirm the capability booleans and their indices exist.

## Definition of done

- [x] One `provider` table serving all three capability tabs
- [x] `model` as a child table with the fields the cards need, including unavailability
- [x] Migration committed and applying cleanly
- [x] Capability JSON and boolean columns kept in sync in one place
- [x] `secret_usage` registered and released transactionally with the provider
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** Provider and model schemas, generated `0002_provider_model` migration and
snapshot, and both repositories registered in the database client and unit of work.
Provider writes maintain indexed capability flags and secret usage transactionally.
Model discovery replaces the full list atomically, preserves IDs by vendor external ID,
and clears a default when its model disappears. Default selection rejects another
provider's model. Added eight repository tests and an Electron migration-log check.

**Deviated from plan:** Used the workspace `pnpm exec vitest run` because studio has no
package-level test script (`pnpm --filter studio test` runs no tests). All 132 tests,
`pnpm typecheck`, `pnpm lint`, `pnpm build`, and seven Electron smoke tests passed.
SQLite table/index inspection is automated against tempDb rather than a SQLite browser;
the built app was launched by Playwright and logged the new migration exactly once.

**Next session should know:** Capabilities and settings use Drizzle JSON text mode, so
repository entities expose typed arrays/objects. Supply timestamps on create and discovery;
updates default to the current time. `setStatus` and `setDefaultModel` accept an optional
timestamp. Select defaults through `setDefaultModel`, not create/update drafts. Discovery
accepts model fields without internal ID or provider ID and assigns those itself. Nested
repository transactions compose with service-owned unit-of-work transactions. No AI drivers
or UI were added; TASK_014 can consume `repositories.providers` and `repositories.models`.
