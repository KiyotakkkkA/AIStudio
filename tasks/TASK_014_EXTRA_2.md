---
task: TASK_014_EXTRA_2
title: Qwen and DeepSeek adapter families — model listing
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_014_EXTRA_1]
unblocks: [TASK_015_EXTRA_1, TASK_014_EXTRA_3]
estimate: 3 h
---

# TASK_014_EXTRA_2 — Qwen and DeepSeek adapter families: model listing

## Context

TASK_014 defined the adapter families and left `qwen-web` and `deepseek-web` as stubs that
fail with "not implemented yet". TASK_014_EXTRA_1 gave them a way to be credentialed —
`AccountTransport` over the `persist:browser-work` session. This task fills the two stubs in,
to the depth Phase 2 needs: **they can say which models are available.** Generation over an
account session is TASK_014_EXTRA_3, deliberately after the Providers page can show what a
linked account offers.

That scope split was a decision on 2026-09-10, and it is worth understanding rather than
working around. Listing models is a single request per vendor and makes the Accounts tab real.
Generation means conversation creation, per-vendor message envelopes, and SSE dialects that
differ from OpenAI's — a task of its own, and one that is far easier to get right once linking
and discovery are already trustworthy.

Both vendors are web apps, not documented APIs. Two consequences shape the code:

- **Endpoints will change without notice.** Every vendor-shaped detail lives in one file per
  family, behind our `DiscoveredModel` type, so a break is a small edit rather than an
  archaeology session.
- **Responses are untrusted input** (PASSPORT §14). Parse with zod, take only mapped fields,
  and treat a shape mismatch as a normal failure state — `UNKNOWN` with a host-side log — not
  a crash.

If a vendor turns out to expose no usable model-list endpoint, that is an expected outcome,
not a blocker: return a curated static list with `live: false` on the result, which is
precisely what the flag added in TASK_014 exists for. TASK_015 and TASK_016 will then label it
honestly instead of implying a probe listed them.

## Why

Without this, a linked account is an avatar and nothing else — no model can be selected, so
nothing downstream can run against it.

## Depends on

- `TASK_014_EXTRA_1` — `AccountTransport`, identity probes, `account` rows.
- `TASK_014` — ports, `AdapterCapabilities`, error normalisation, `FakeTransport`.

## Read first

- `/docs/PASSPORT.md` §12 (adapter families; capability descriptors), §14 (untrusted input).
- `apps/studio/src/host/drivers/ai/adapters/openaiCompatible.ts` — the shape to mirror.
- `apps/studio/src/host/drivers/ai/identity/` — the vendor conventions already established.

## Scope

**In scope**
- `adapters/qwenWeb.ts` and `adapters/deepseekWeb.ts` implementing `TextGenerationDriver`
- `listModels` per family, live where possible, curated static list with `live: false` otherwise
- `AdapterCapabilities` for both families, honestly declared
- `generate`/`stream` present but failing with a clear "not implemented yet — TASK_014_EXTRA_3"
- Registry wiring replacing the two stubs
- Tests over `FakeTransport` with recorded payload fixtures

**Out of scope** (do not do these, even if tempting)
- Generation or streaming for either family (TASK_014_EXTRA_3)
- Conversation/chat-session creation
- Embeddings or image generation for these families — declare them unsupported
- Driving the vendor's page via CDP — the decision was explicitly against that
- Any change to `AccountTransport` beyond passing extra headers a family needs

## Steps

1. Discover each vendor's model list **by observation, not guesswork**: open the vendor in
   `/browser`, watch what the page requests when the model picker opens, and record the exact
   request and response. Save a redacted response as a test fixture under
   `test/fixtures/providers/` with every credential-bearing field replaced. Note the endpoint
   and date in a comment at the top of the adapter, so the next person knows how fresh it is.
2. `adapters/qwenWeb.ts`: `listModels` maps the vendor's entries onto `DiscoveredModel`
   (`externalId`, `displayName`, `family`, `contextWindow`, `maxOutput` where present). Where a
   field is absent, leave it null — do not invent context windows.
3. `adapters/deepseekWeb.ts`: same, and remember the DeepSeek envelope convention from
   EXTRA_1 — a 200 with a non-zero `code`/`biz_code` is a failure.
4. If either vendor has no usable endpoint: a curated static list in that adapter file, with a
   comment naming the source and date, and `live: false` on the result. Do not fabricate model
   ids — take them from the vendor's own picker.
5. `AdapterCapabilities` for both: `authModes: ['account']`, `streaming: true` (the family will
   support it; the method lands in EXTRA_3), `liveModelList` per what step 1 found, and
   `honours` set to what the vendor actually accepts — for a web chat backend that is usually
   very little, and declaring it honestly is what stops TASK_016 from rendering sliders that
   do nothing.
6. `generate` and `stream` throw `UNKNOWN` with
   "account-mode generation is not implemented yet (TASK_014_EXTRA_3)". A clear message is the
   deliverable here; a silent empty stream is not.
7. Registry: replace the two stub registrations. Adding a third account-family later must be
   one file plus one registration — if it is more, the registry is doing too much.
8. Tests: each fixture maps to the expected `DiscoveredModel[]`; a malformed payload produces
   `UNKNOWN` and logs host-side without throwing raw; a 401 through `AccountTransport` produces
   `PROVIDER_SESSION_EXPIRED`; the static-list path sets `live: false`; `generate` fails with
   the documented message. No network in tests.
9. Commit: `TASK_014_EXTRA_2: Qwen and DeepSeek adapter families`.

## Verify

```bash
pnpm test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check with both accounts signed in inside `/browser`: from a host-side scratch script,
call `listModels` for each family and confirm the returned ids match what the vendor's own
model picker shows. Then sign out of one and confirm the call reports
`PROVIDER_SESSION_EXPIRED`.

## Definition of done

- [x] Both families list models, live where an endpoint exists, `live: false` where curated
- [x] Every vendor-specific detail is confined to that family's adapter file
- [x] Capabilities are declared honestly, including which sampling parameters are ignored
- [x] Malformed or hostile payloads fail as `UNKNOWN` without leaking the payload
- [x] `generate`/`stream` fail with the documented not-implemented message
- [x] Fixtures are redacted; no credential is committed
- [x] Tests pass with no network access
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**

- `adapters/accountFamily.ts` — the shared spine for account-mode families:
  `AccountFamilyAdapter` (transport wrapper, JSON decode, `generate`/`stream` rejecting with
  `ACCOUNT_GENERATION_PENDING`), `malformedPayload` (logs `warn`/`ai` with family + endpoint
  only, returns `UNKNOWN`) and the `VendorObservation` provenance record.
- `adapters/qwenWeb.ts` — live listing over `GET {baseUrl}/models/`
  (`https://chat.qwen.ai/api/v2/models/`, observed 2026-09-10). Zod parses the envelope,
  accepting both `data: [...]` and the nested `data.data: [...]` the vendor actually returns.
  Maps `id`/`name`/`owned_by` and `info.meta.max_context_length` /
  `max_generation_length`; `sizeBytes` stays null. `vision` and `reasoning` come from the
  union of `meta.capabilities` (booleans), `meta.abilities` (numbers) and `meta.modality`.
  Entries with `info.is_active === false`, a blank id, or a duplicate id are dropped.
- `adapters/deepseekWeb.ts` — `chat.deepseek.com` exposes no model-list endpoint, so a
  curated static list (`deepseek-chat`, `deepseek-reasoner`) with `liveModelList: false`.
  `listModels` makes no request at all.
- Registry: both stubs replaced; `stub()` now serves `anthropic` only. Each family is one
  file plus one registration.
- `test/fixtures/providers/qwenModels.json` — redacted capture (`user_id` and `request_id`
  zeroed, no cookies or tokens), carrying the inactive / blank-id / duplicate-id cases.
- `apps/studio/tests/accountAdapters.test.ts` — fixture → expected `DiscoveredModel[]`;
  three malformed shapes each produce `UNKNOWN`, log once host-side and never carry the
  payload; a 401 through `AccountTransport` produces `PROVIDER_SESSION_EXPIRED`; the curated
  path touches no transport; `generate`/`stream` reject with the documented message.

**Deviated from plan:**

- The endpoint and date live in exported `VendorObservation` constants
  (`QWEN_MODELS_OBSERVATION`, `DEEPSEEK_MODELS_OBSERVATION`) rather than a header comment —
  same provenance, and the tests assert it instead of it silently rotting.
- The Qwen response was supplied from an observed session rather than re-captured through
  `/browser`; the fixture is that payload, redacted.
- DeepSeek's two curated ids mirror the one model plus the DeepThink toggle its picker
  exposes, named after DeepSeek's own `deepseek-chat` / `deepseek-reasoner`. The web app
  sends no `model` field — EXTRA_3 must map these onto `thinking_enabled`.
- `apps/studio/tests/accounts.test.ts` — "an account-mode provider without a live account
  never reaches the network" asserted the `deepseek-web` stub threw `UNKNOWN`; it now asserts
  a driver is built. It still proves no request is made.

**Next session should know:**

- `DeepSeekWebAdapter.listModels()` takes no `AbortSignal` (it makes no request), so a caller
  holding the concrete class must call it with no argument; through `TextGenerationDriver` it
  is unchanged.
- `AccountFamilyAdapter.stream()` returns an iterable that rejects on the first `next()`
  rather than throwing synchronously, because `streamToEvents` calls `stream()` outside its
  `try`.
- Qwen's `meta.capabilities.thinking` and `meta.abilities.thinking` disagree in the wild
  (`false` next to `2`); the union is deliberate. Revisit if EXTRA_3 finds one authoritative.
- `chat_type` (`t2t` / `t2i` / `search`) is parsed but not used to filter — no observed entry
  lacked `t2t`. If an image-only entry appears, filter there.
- Nothing yet persists these models; that is TASK_015's discovery service.
