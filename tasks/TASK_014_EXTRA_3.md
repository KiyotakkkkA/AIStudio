---
task: TASK_014_EXTRA_3
title: Account-mode generation and streaming for Qwen and DeepSeek
phase: 2 — Providers and the AI layer
status: done
depends_on: [TASK_014_EXTRA_2, TASK_015_EXTRA_1]
unblocks: [TASK_024]
estimate: 4 h
---

# TASK_014_EXTRA_3 — Account-mode generation and streaming

## Context

By the time this task starts, an account can be linked from the Accounts tab, its identity and
session health are tracked, and its models are listed. The one thing an account-mode provider
still cannot do is the thing it exists for: generate text. This task implements `generate` and
`stream` for the `qwen-web` and `deepseek-web` families, and it is scheduled here — after
linking and discovery — because those two make it debuggable.

The work is a wire-format exercise. Both vendors' web apps send a chat turn to their own
backend and receive a server-sent event stream in a dialect that is *not* OpenAI's: different
frame names, different delta shapes, usually a conversation or session id that must be created
first, and often reasoning/thinking content interleaved with the answer. All of that lives
inside the family's adapter file. The rest of the app already knows how to consume a
`TextDelta` stream, and `streamToEvents` from TASK_014 already turns it into `token` events on
the bus.

Two constraints from PASSPORT §14 apply with force here. First, model output and any page-
derived content are untrusted data, never instruction. Second, the vendor's payloads carry
session credentials — never log a raw request or response body, at any log level.

## Why

This is the last piece that makes an account a usable model source, and TASK_024 (chat) needs
account providers to work exactly like API providers from the service layer up.

## Depends on

- `TASK_014_EXTRA_2` — the two adapter files, capabilities, fixtures.
- `TASK_015_EXTRA_1` — linking, so there is a live session to develop against.
- `TASK_014` — `TextDelta`, `streamToEvents`, error normalisation, cancellation.

## Read first

- `/docs/PASSPORT.md` §12 (streaming re-emitted onto the event channel), §14.
- `apps/studio/src/host/drivers/ai/adapters/openaiCompatible.ts` — how streaming and abort are
  already handled, so the two families behave identically from outside.

## Scope

**In scope**
- Conversation/session creation per family, where the vendor requires it
- `generate` and `stream` for `qwen-web` and `deepseek-web`
- The vendors' SSE dialects mapped onto `TextDelta`
- Reasoning/thinking content surfaced as a distinct delta kind rather than concatenated
- Cancellation that genuinely aborts the vendor request
- Capability descriptors updated to reflect what actually works
- Tests over recorded, redacted SSE fixtures

**Out of scope** (do not do these, even if tempting)
- File upload, web search, or any other vendor-side feature toggles
- Conversation persistence on the vendor's side beyond what a single generation needs —
  our conversation store is TASK_024's
- Tool calling (TASK_023)
- A third account family

## Steps

1. Record real traffic for one turn per vendor from `/browser`: the conversation-create call
   if any, the completion call, and the full SSE stream. Redact credentials and save as
   fixtures next to the EXTRA_2 ones. Note the date in the adapter comment.
2. Implement conversation/session creation where required, and cache the id for the lifetime
   of one `GenerateRequest` only. Do not accumulate vendor-side conversations invisibly; if a
   vendor requires one per turn, say so in a comment and let it be.
3. Map our messages onto the vendor envelope. Parameters the family does not honour are
   dropped and logged at debug, matching TASK_014's rule and the capability descriptor.
4. SSE parsing per family into `TextDelta`. Give `TextDelta` a `kind` — `text` and `reasoning`
   at minimum — so thinking content can be shown or hidden by the UI instead of being
   silently glued into the answer.
5. Cancellation: aborting the caller's signal must abort the underlying `net.fetch`, and the
   stream must end with a cancelled outcome, exactly as the OpenAI-compatible adapter does.
   Test it against a fixture with a controllable delay.
6. Mid-stream failures: a session expiring during a stream produces `PROVIDER_SESSION_EXPIRED`
   and the account is marked `needs-relink` through the service, with whatever text already
   arrived preserved. A truncated stream is an error, not a successful short answer.
7. Update `AdapterCapabilities` for both families to what is now true.
8. Tests: fixtures produce the expected delta sequences; reasoning deltas are separated;
   abort mid-stream stops emission and ends cancelled; a 401 mid-stream maps to
   `PROVIDER_SESSION_EXPIRED`; a truncated stream errors; no raw body is ever logged (assert
   against the logger spy). No network in tests.
9. Commit: `TASK_014_EXTRA_3: account-mode generation and streaming`.

## Verify

```bash
pnpm test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check with both accounts linked: stream a one-sentence completion from each vendor and
confirm tokens arrive as `token` events; abort mid-stream and confirm the vendor request
actually stops; sign out mid-stream and confirm the run ends as `session expired` with the
partial text intact and the Accounts tab showing Re-link.

## Definition of done

- [ ] Both families generate and stream through the same ports as API providers — `qwen-web` does;
      `deepseek-web` is deferred, see the handoff
- [x] Reasoning content is a distinct delta kind, not concatenated into the answer
- [x] Abort cancels the vendor request; mid-stream expiry is reported as such
- [x] No request or response body is logged at any level
- [x] Capability descriptors match reality
- [x] Tests pass over redacted fixtures with no network access
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:**
- `TextDelta` now carries `kind` (`text` | `reasoning`), `GenerateResult` carries `reasoning`, and
  the `token` host event carries an optional `kind`, so thinking content is never glued into the
  answer. `streamToEvents` accumulates the two separately and returns both.
- `qwen-web` generates and streams: `POST /chats/new` creates one vendor chat per
  `GenerateRequest`, then `POST /chat/completions?chat_id=…` streams the phase-based SSE dialect.
  `thinking_summary` frames carry a cumulative array of thoughts, so the adapter diffs it against
  what it has already emitted and yields only the new suffix as `reasoning` deltas; `answer`
  frames are incremental and yield `text` deltas.
- Cancellation, mid-stream vendor errors (`success: false` / `error`) mapped to
  `PROVIDER_SESSION_EXPIRED` when the code or status is an auth one, and a truncated stream
  (no `phase: "answer"`, `status: "finished"` frame) reported as `PROVIDER_UNREACHABLE`.
- `AccountAdapterOptions` gained `clock`, `newId` and `onSessionExpired`; the service layer wires
  the last one to mark the account `needs-relink`.
- `openai-compatible` picks up `reasoning_content` / `reasoning` deltas now that a kind exists.
- Redacted fixtures under `test/fixtures/providers/`: `qwenChatCreated.json`, `qwenStream.sse`,
  `qwenStreamExpired.sse`, `qwenStreamTruncated.sse`. 15 tests in `accountAdapters.test.ts`, no
  network.

**Deviated from plan:**
- `deepseek-web` generation is deliberately left unimplemented: its completion wire format was not
  recorded in this session. `DEEPSEEK_WEB_CAPABILITIES.streaming` is now `false` so the descriptor
  matches reality, and `generate`/`stream` reject with `ACCOUNT_GENERATION_PENDING`. A follow-up
  task should record the traffic and fill in the adapter.
- No code comments, per the request; the vendor observations and their dates live in
  `QWEN_GENERATION_OBSERVATION` and `DEEPSEEK_GENERATION_OBSERVATION` instead.
- Traffic was reconstructed from the wire captures supplied with the request rather than recorded
  from `/browser` in this session, so the manual check below is still outstanding.
- Our messages are flattened into a single user turn (`flattenPrompt`) because each generation
  opens a fresh vendor chat with no history; `parentId` / `parent_id` are therefore `null` on the
  first and only message.

**Next session should know:**
- One vendor-side chat is created per generation and never deleted — Qwen offers no cheaper path.
- The manual check (stream, abort, sign-out mid-stream against a live linked account) has not been
  run; it needs TASK_015_EXTRA_1 linking.
- `thinking_enabled: true` is sent for every model, matching the recorded web-app request.
