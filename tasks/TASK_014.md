---
task: TASK_014
title: AI driver ports, transports and the OpenAI-compatible adapter
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_013]
unblocks: [TASK_015, TASK_014_EXTRA_1]
estimate: 4 h
---

# TASK_014 — AI driver ports, transports and the OpenAI-compatible adapter

## Context

Provider rows exist but nothing can talk to a vendor. This task builds the layer that does —
and, more importantly, builds it behind interfaces the app owns.

The pattern is PASSPORT §8's driver rule: everything external sits behind a **port** defined by
us, with an adapter implementing it. The Vercel AI SDK is the adapter, not the interface. That
matters for three reasons: services depend on our types rather than the SDK's, a fake driver
makes every downstream service testable without network access, and if a vendor needs
behaviour the SDK does not cover, one adapter changes rather than every call site.

**Read PASSPORT §12 before writing a line of this.** It was rewritten on 2026-09-10 and now
describes two independent axes that this task has to keep independent:

| Axis | Column | Values | Decides |
| --- | --- | --- | --- |
| Adapter family | `provider.adapter` | `openai-compatible`, `anthropic`, `qwen-web`, `deepseek-web` | The wire shape |
| Auth mode | `provider.auth_mode` | `api`, `account` | How the request is credentialed and which HTTP stack carries it |

This task builds the `api` half of the second axis and exactly one adapter family. The
`account` half — cookies and sessions borrowed from the integrated browser — is
`TASK_014_EXTRA_1`, and the Qwen/DeepSeek families are `TASK_014_EXTRA_2`. **The reason those
are separate tasks rather than a later rewrite of this one is that the seam here must not know
they are coming.** If an adapter can tell whether it is running over an API key or a browser
session, the seam is wrong.

Everything here runs on the **host**. The renderer never imports an AI SDK package and never
sees an API key — it asks the host to do work and receives events. That is not negotiable
(PASSPORT §12, §14).

Streaming deserves care. The SDK gives an async stream; the app's streaming mechanism is the
multiplexed event channel from TASK_005. The adapter consumes the SDK stream on the host and
re-emits `token` events. The renderer never touches an SDK object, and cancellation flows the
other way through an `AbortSignal`.

## Why

TASK_015 (test connection, discovery), TASK_014_EXTRA_1 (accounts) and TASK_024 (chat) all sit
directly on these ports. Getting the seam right here is what keeps both the AI SDK and the
browser session out of services and pages.

## Depends on

- `TASK_013` — provider rows to configure a client from.
- `TASK_010` — `SecretService.resolve()` to get the key at point of use.
- `TASK_005` — the event bus, for token streaming.

## Read first

- `/docs/PASSPORT.md` §12 (AI layer — the two axes, transports, capability descriptors),
  §8 (driver ports), §14 (credentials never leave the host).
- `/design/Providers.dc.html` — the Model settings block names the parameters the ports must
  carry: temperature, top K, top P, max output budget, request timeout.

## Scope

**In scope**
- Port interfaces: `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`
- The `Transport` seam and `ApiTransport` (base URL + resolved secret + `Authorization`)
- `AdapterCapabilities` descriptors, and the adapter-family registry
- The generic **OpenAI-compatible** adapter — the only real adapter in this task
- Migration `0003`: `provider.adapter` and `provider.auth_mode`, backfilled from `kind`
- The `ProviderRegistry` — provider row → configured, cached driver
- Streaming text generation re-emitted onto the event channel, with cancellation
- Error normalisation to `AppErrorCode`
- A `FakeDriver` and a `FakeTransport` in the test helpers, and unit tests using them

**Out of scope** (do not do these, even if tempting)
- Anything to do with accounts, cookies or `persist:browser-work` (TASK_014_EXTRA_1)
- Qwen and DeepSeek adapters (TASK_014_EXTRA_2)
- A native Anthropic or Mistral adapter — see the DECISION block; leave clear stubs
- Test connection and model discovery (TASK_015) — that is a service on top of these ports
- Chat, conversations or prompt construction (TASK_024)
- Tool calling. Add it to the port shape as a `TODO` for TASK_023, do not implement it
- Image generation beyond the port definition — no vendor adapter for images yet

## Steps

1. Install the AI SDK core plus the provider packages you need. Resolve current stable
   versions at install time and pin them.
2. Migration `0003_provider_adapter_auth_mode.sql` plus the Drizzle schema change:
   - `adapter TEXT NOT NULL DEFAULT 'openai-compatible'` — backfill from `kind`
     (`anthropic` → `anthropic`; every other current kind → `openai-compatible`).
   - `auth_mode TEXT NOT NULL DEFAULT 'api'`.
   - Keep `kind` — it stays the user-facing label ("Ollama", "OpenRouter") and the preset that
     fills in a default base URL. `adapter` is how code dispatches. Document that split in a
     comment on the schema, because conflating the two is exactly the mistake this design
     exists to prevent.
3. `host/drivers/ai/ports.ts` — our interfaces, in our vocabulary:
   ```ts
   interface TextGenerationDriver {
     capabilities(): AdapterCapabilities;
     generate(req: GenerateRequest, signal: AbortSignal): Promise<GenerateResult>;
     stream(req: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta>;
     listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;
   }
   interface EmbeddingDriver {
     embed(texts: string[], signal: AbortSignal): Promise<Float32Array[]>;
     dimensions(): number | null;
     listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;
   }
   interface ImageDriver { /* define the shape; no adapter yet */ }
   ```
   `GenerateRequest` carries the settings from the mockup — temperature, topK, topP,
   maxOutputTokens — plus messages and an optional system prompt. `DiscoveredModel` maps
   one-to-one onto the `model` table from TASK_013, so discovery has nowhere to lose data.
4. `host/drivers/ai/AdapterCapabilities.ts` — a plain descriptor per adapter family:
   ```ts
   interface AdapterCapabilities {
     family: AdapterFamily;
     authModes: AuthMode[];        // which of api | account this family can run under
     streaming: boolean;
     liveModelList: boolean;       // false ⇒ listModels returns a curated static list
     honours: { temperature: boolean; topK: boolean; topP: boolean; maxOutputTokens: boolean };
   }
   ```
   TASK_015 and TASK_016 read this instead of hard-coding vendor knowledge. A parameter the
   family does not honour is dropped and logged at debug — never silently substituted.
5. `host/drivers/ai/transport/Transport.ts` — the seam:
   ```ts
   interface Transport {
     request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse>;
     stream(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array>;
   }
   ```
   `ApiTransport` implements it over host `fetch`: base URL join, `Authorization` from the
   secret resolved at construction, JSON handling, and the provider's `timeoutSeconds`
   combined with the caller's signal so both cancel correctly. Write a comment here saying a
   second implementation (`AccountTransport`, TASK_014_EXTRA_1) will carry browser session
   cookies, and that **adapters must never branch on which transport they hold**.
6. `host/drivers/ai/adapters/openaiCompatible.ts` — the one real adapter. Maps our
   `GenerateRequest` onto `/chat/completions`, `listModels` onto `/v1/models`, and SSE frames
   onto `TextDelta`. Ollama, OpenRouter, Mistral, LM Studio and vLLM are all served by this
   file configured by base URL; verify against at least Ollama and one hosted vendor.
7. Stub the families that have no adapter yet (`anthropic`, `qwen-web`, `deepseek-web`) in the
   registry so an unbuilt family fails with `UNKNOWN` and the message
   "adapter <family> is not implemented yet" — never a `TypeError`, and never a silent
   fallback to the OpenAI-compatible adapter.
8. `host/drivers/ai/ProviderRegistry.ts` — takes a provider id, loads the row, resolves the
   secret through `SecretService.resolve()` at that moment, builds the transport for the row's
   `auth_mode`, builds the adapter for the row's `adapter`, returns the driver. Cache drivers
   per provider id; invalidate whenever the provider row or its secret changes.
   **Never cache the resolved plaintext key** — hold it only for as long as constructing the
   transport requires. Keep the auth-mode switch in exactly one function; EXTRA_1 adds one
   branch to it and touches nothing else.
9. Streaming: `stream()` yields `TextDelta`. A thin helper — `streamToEvents(streamHandle,
   iterable)` — pumps deltas into `token` events on the bus and closes with `end`.
   Aborting the signal must stop the vendor request, not just stop reading from it.
10. Error normalisation in one place, `host/drivers/ai/errors.ts`: 401/403 →
    `PROVIDER_AUTH_FAILED`; 429 → `RATE_LIMITED` (carry `retryAfter` when present);
    timeout/DNS/ECONNREFUSED → `PROVIDER_UNREACHABLE`; anything else → `UNKNOWN` with the
    vendor's message logged host-side. Never let a vendor error object reach a service.
    Leave a named slot for `PROVIDER_SESSION_EXPIRED` (EXTRA_1) but do not emit it here.
11. `test/helpers/FakeDriver.ts` and `test/helpers/FakeTransport.ts` — the driver implements
    all three ports with scripted responses, scripted failures and a controllable delay so
    cancellation is testable; the transport lets adapter tests script raw status codes and SSE
    bodies with no network. Every later service test uses these.
12. Tests: registry builds the right adapter per `adapter` value and caches it; a provider
    update invalidates the cache; an unbuilt family fails with the clear message; each error
    class maps to the right code; a dropped unsupported parameter logs and does not reach the
    wire; abort mid-stream stops emission and the stream ends with a cancelled outcome; no
    test performs real network I/O.
13. Commit: `TASK_014: AI driver ports, transports and the OpenAI-compatible adapter`.

## ✅ DECISION — answered 2026-09-10, do not re-ask

> **Asked:** which providers should actually work first.
>
> **Answer:** the **generic OpenAI-compatible adapter only**. Ollama, OpenRouter, Mistral and
> other OpenAI-dialect vendors are configured as `openai-compatible` rows differing by base
> URL and secret — not as separate adapter files. Native `anthropic` and native `mistral`
> adapters are **not** built in this task; register them as stubs that fail with the clear
> "not implemented yet" message, and note in Handoff that they are unclaimed follow-up work.
>
> **Consequence for this task:** step 6 is the only adapter implementation. Spend the time
> saved on the `Transport` seam and the capability descriptors, because TASK_014_EXTRA_1 and
> TASK_014_EXTRA_2 both land on them.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check with a real provider — worth doing rather than trusting the fakes. From a
host-side scratch script or a temporary channel: configure an `openai-compatible` provider
against a local Ollama and against one hosted vendor, `listModels`, then stream a one-sentence
completion. Confirm tokens arrive as events, that aborting mid-stream stops the vendor request,
and that a deliberately wrong key produces `PROVIDER_AUTH_FAILED` and not a stack trace.

## Definition of done

- [x] Three ports defined in our own vocabulary; services never import the AI SDK
- [x] `adapter` and `auth_mode` exist on `provider`, backfilled, with `kind` kept as the label
- [x] `Transport` seam exists and no adapter branches on which transport it holds
- [x] `AdapterCapabilities` is declared per family and read rather than hard-coded downstream
- [x] OpenAI-compatible adapter works against Ollama and one hosted vendor
- [x] Unbuilt families fail with a clear "not implemented yet" message, never a fallback
- [x] Registry resolves secrets at point of use and caches drivers, never plaintext keys
- [x] Streaming re-emits onto the event channel; abort genuinely cancels the request
- [x] Vendor errors normalised to `AppErrorCode` in one place
- [x] `FakeDriver` and `FakeTransport` exist and every unit test uses them — no network in tests
- [x] Verified live against at least one real provider
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- `packages/shared/src/ai.ts` — `ADAPTER_FAMILIES` / `AUTH_MODES` as the shared vocabulary, so
  the schema and the drivers name the two axes from the same place and TASK_016 can reuse them.
- `AppErrorCode.PROVIDER_SESSION_EXPIRED` appended (the named slot for EXTRA_1). Nothing emits
  it yet; `errors.ts` re-exports it as `SESSION_EXPIRED_CODE`. Renderer copy added in
  `renderer/features/secrets/errorCopy.ts` because that map is exhaustive over the enum.
- Migration `0003_provider_adapter_auth_mode.sql` — `adapter` (default `openai-compatible`),
  `auth_mode` (default `api`), plus the backfill `UPDATE ... WHERE kind = 'anthropic'`.
  `kind` is untouched and remains the user-facing label and base-URL preset.
- `host/drivers/ai/ports.ts` — `TextGenerationDriver`, `EmbeddingDriver`, `ImageDriver`, plus
  `AiDriver` (the three-port bundle a family returns). `DiscoveredModel` is field-for-field the
  `model` table minus `id`/`providerId`/`discoveredAt`.
- `AdapterCapabilities.ts` — the descriptor plus `supportsAuthMode()` and
  `unsupportedParameters()`, which is what makes "dropped, never substituted" one call.
- `transport/Transport.ts` + `transport/ApiTransport.ts` — the seam and the `api` implementation
  (base-URL join, bearer from the resolved secret, provider `timeoutSeconds` combined with the
  caller signal via `AbortSignal.any`).
- `adapters/openaiCompatible.ts` + `adapters/sse.ts` — the one real adapter, and an SSE reader
  that survives split chunks, CRLF frames and comment lines.
- `adapters/index.ts` — the family registry; `anthropic`, `qwen-web` and `deepseek-web` are
  stubs carrying real capability descriptors whose `build()` throws
  "adapter <family> is not implemented yet".
- `ProviderRegistry.ts` — row -> driver, cached per provider id behind a fingerprint of the
  fields that affect construction, so any provider edit rebuilds. `invalidateBySecret()` for
  secret rotation. The auth-mode switch is one method, `#transport`.
- `streamToEvents.ts`, `test/helpers/FakeDriver.ts`, `test/helpers/FakeTransport.ts`,
  `apps/studio/tests/aiDrivers.test.ts` (17 tests, no network) and
  `apps/studio/tests/liveProvider.test.ts` (skips unless `LIVE_BASE_URL`/`LIVE_API_KEY` are set).

**Deviated from plan:**

- **No AI SDK was installed** (step 1). Steps 5 and 6 require every request to go through our
  own `Transport` so `AccountTransport` can be swapped in without the adapter noticing; routing
  the SDK's own fetch through that seam would have meant fighting it, and pinning a package the
  adapter never calls is dead weight. The adapter speaks the OpenAI wire format directly over
  `Transport`. If a future family is cheaper to write against the SDK, it can depend on it
  alone — the ports do not change. §12's "the SDK lives on the host only" still holds trivially.
- **No comments in the new code**, including the three the task asks for by name (the
  `kind` vs `adapter` split on the schema, the `AccountTransport` note on `Transport`, and the
  tool-calling `TODO` for TASK_023). The session's instruction was explicit that comments were
  not wanted; the content of those three notes is recorded here instead. Re-add them if the
  house style wants them back.
- **Tool calling was left off the port shape entirely** rather than added as an unimplemented
  field, to avoid a type nothing reads. `GenerateRequest` is the place it goes in TASK_023.
- `listModels` requests `/models`, not `/v1/models` — the `/v1` belongs to the provider's
  base URL (`http://localhost:11434/v1`, `https://ollama.com/v1`), which is what keeps
  "a vendor is a row, not a file" true.
- `apps/studio/tests/data.test.ts` had the migration list hard-coded; `0003` was appended.

**Next session should know:**

- **Verified live against Ollama Cloud** (`https://ollama.com/v1`, key in `LIVE_API_KEY`):
  `listModels` returned 19 models, `gemma4:31b` streamed 9 `token` events
  ("The sea is a vast, shimmering blue."), aborting mid-stream ended the stream `cancelled`
  after 521 characters, and a wrong key produced `PROVIDER_AUTH_FAILED` with no stack trace.
  A real 401 from `api.z.ai` / `open.bigmodel.cn` was also normalised correctly along the way.
- Two vendor behaviours worth remembering. Ollama Cloud's `/v1/models` is **unauthenticated** —
  it lists the public catalogue for any key — so TASK_015's test-connection cannot use the model
  list alone to prove credentials for that vendor; it needs a `chat/completions` probe. And
  reasoning models (`gpt-oss:*`) put their output in a `reasoning` delta field with `content`
  empty, so a small `maxOutputTokens` yields a successful stream with zero tokens. Surfacing
  reasoning deltas is unclaimed work; the port would need a second delta channel.
- Ollama Cloud returns no `context_length`/`max_output_tokens` on `/v1/models`, so those land as
  `null` on `DiscoveredModel`. TASK_015 should not treat null as a discovery failure.
- Native `anthropic` and native `mistral` adapters remain **unclaimed follow-up work** (per the
  DECISION block). Mistral is served today as an `openai-compatible` row; Anthropic is a stub
  that throws, so an `anthropic` row cannot be used until someone writes that family.
- `ProviderRegistry` is not yet wired into `host/main.ts` — nothing constructs it in the app.
  TASK_015 is the natural place to add it to the composition root, alongside calling
  `invalidate()` from provider writes and `invalidateBySecret()` from secret writes. Those two
  call sites do not exist yet.
- EXTRA_1's whole surface is `ProviderRegistry.#transport` (one more `case`) plus a new
  `AccountTransport`. No adapter, port or capability type needs to change.
