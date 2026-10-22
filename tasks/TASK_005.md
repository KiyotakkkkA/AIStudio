---
task: TASK_005
title: Multiplexed event channel, renderer EventRouter, session recorder
phase: 0 — Foundation
status: done
depends_on: [TASK_004]
unblocks: [TASK_007, TASK_022]
estimate: 3 h
---

# TASK_005 — Multiplexed event channel, renderer EventRouter, session recorder

## Context

TASK_004 gave the app request/response IPC. That covers "fetch the provider list" but not the
things this app is actually made of: LLM tokens arriving one at a time, an indexing job
crawling from 0 to 31 402 chunks, a scenario emitting a step event per node, a browser agent
asking permission to submit a form.

PASSPORT decision D4 settles the mechanism: **one multiplexed event channel**, not a
MessagePort per stream and not async iterators. The host pushes typed events shaped
`{ type, streamId, seq, … }` on a single channel; the renderer routes them by `streamId` to
whichever store subscribed. One mechanism covers token deltas, progress, step events, log
lines and approval requests.

The single-channel choice buys something specific that is easy to underrate: because every
event in the entire application passes through one function, recording a session to disk and
replaying it into the UI is a few lines of code. Build that recorder now, in this task. Once
scenarios and agentic runs exist, being able to replay a failing run into a fresh UI is the
difference between a ten-minute debugging session and a two-hour one.

## Why

Everything streaming depends on this: chat (TASK_024), tasks (TASK_026), indexing (TASK_028),
downloads (TASK_029), scenarios (TASK_039), agentic (TASK_040), browser (TASK_047). Building
it once, generically, avoids six bespoke half-solutions.

## Depends on

- `TASK_004` — contract registry, preload bridge, error model.

## Read first

- `/docs/PASSPORT.md` §7, specifically "The event channel (D4)" — the `HostEvent`
  discriminated union is specified there; implement it as written.
- `/docs/PASSPORT.md` §13 (MobX rules — stores subscribe on start, unsubscribe on dispose).

## Scope

**In scope**
- `HostEvent` discriminated union in `packages/shared`
- Host-side `EventBus`: create a stream, emit, close, with monotonic `seq` per stream
- The `zvs:events` channel and the preload's second function, `subscribe`
- Renderer `EventRouter`: dispatch by `streamId`, gap detection, subscribe/unsubscribe
- Session recorder (write) and replayer (read) behind a dev-only flag
- Tests including an out-of-order/gap case

**Out of scope**
- Any actual producer of events — no runs exist yet. Ship a `system.demoStream` channel
  that emits a counting sequence so the machinery is exercised and testable.
- Backpressure tuning. Note it as a known gap; revisit if a real stream floods the UI.

## Steps

1. In `packages/shared/src/events/`, define `HostEvent` as a `z.discriminatedUnion('type', …)`
   with the variants from PASSPORT §7: `token`, `progress`, `step`, `log`, `approval`, `end`.
   For now, `step`, `log` and `approval` may carry loosely-typed payloads with a `TODO`
   naming the task that will tighten them (TASK_022, TASK_023). Every variant carries
   `streamId`, `seq` and `ts`.
2. Host `EventBus` in `apps/studio/src/host/platform/events.ts`:
   - `openStream(): StreamHandle` — allocates a `StreamId`, seq starts at 0.
   - `handle.emit(event)` — stamps `seq` and `ts`, sends on `zvs:events` to every live
     window, appends to the recorder if enabled.
   - `handle.end(outcome)` — emits the terminal `end` event and releases the stream.
   - Emitting on an ended stream is a logged programming error, not a crash.
   - If no window is open, events are dropped (with a debug log) — the host must never
     block on the UI being present.
3. Add `subscribe(handler)` to the preload bridge. It returns an unsubscribe function. The
   preload still exposes **exactly two** functions and nothing more; keep the comment there
   accurate.
4. Renderer `EventRouter` in `src/renderer/app/EventRouter.ts`:
   - `subscribe(streamId, handler)` → returns dispose.
   - Routes each event to the handlers registered for its `streamId`.
   - **Gap detection:** track the last `seq` per stream; if an event arrives with
     `seq > last + 1`, log a warning with the gap size and surface it on the event so a
     store can decide to refetch rather than render a hole.
   - Events for an unknown `streamId` are buffered briefly (a small ring, a few hundred
     events) then dropped — a subscriber can attach a tick after starting a run.
5. Recorder: when `ZVS_RECORD_EVENTS=1`, append every event as JSONL to
   `logsDir/streams/<date>.jsonl`. Replayer: a dev-only script that reads a file and feeds
   it into the `EventRouter`, so a recorded run can be replayed into a running UI with no
   host work at all.
6. Add `system.demoStream` to the contract — starts a stream that emits ten `progress`
   events then `end`. Call it from the placeholder renderer and render the counter. This
   stays as a smoke test alongside `system.ping`.
7. Tests:
   - `seq` is monotonic and per-stream, not global.
   - The router delivers to the right subscriber and to no other.
   - A forced gap raises the warning path.
   - Dispose actually stops delivery (no leaked handler).
   - Recorder round-trip: record N events, replay, assert the same sequence arrives.
8. Commit: `TASK_005: multiplexed event channel, router and recorder`.

## Verify

```bash
pnpm --filter ipc test
pnpm --filter studio dev
ZVS_RECORD_EVENTS=1 pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual checks:
- The demo stream counts 1→10 in the window and terminates cleanly.
- With recording on, a JSONL file appears and contains exactly the events you saw.
- `Object.keys(window.zvs)` returns exactly `['call', 'subscribe']`.
- Starting two demo streams at once keeps their counters independent.

## Definition of done

- [x] Events flow host → renderer on one channel, routed by `streamId`
- [x] Two concurrent streams do not interfere
- [x] Gap detection warns rather than silently rendering a hole
- [x] Recorder writes and the replayer feeds the UI with the host idle
- [x] Preload still exposes exactly two functions
- [x] Tests and gates pass
- [x] Handoff filled in, status `done`, INDEX.md ticked

## Handoff

**Built:** `HostEvent` in `packages/shared/src/events/HostEvent.ts` — the PASSPORT §7
discriminated union (`token`, `progress`, `step`, `log`, `approval`, `end`) over a shared
`{ streamId, seq, ts }` envelope, plus `EVENT_CHANNEL`, `RunOutcomeDto` and `HostEventDraft`
(the union minus the envelope, which is what producers pass to `emit`). Host `EventBus` in
`apps/studio/src/host/platform/events.ts`: `openStream()` allocates a `StreamId` and returns a
handle whose `emit` stamps `seq` (per stream, from 0) and `ts` and whose `end(outcome)` sends
the terminal event and releases the stream; emitting after `end` logs at `error` and drops,
no window means a `debug` log and a drop, and the recorder is offered every event before the
windows are. `createEventRecorder` (`platform/eventRecorder.ts`) appends JSONL to
`<userData>/logs/streams/<date>.jsonl` when `ZVS_RECORD_EVENTS=1`. Preload gained `subscribe`
and is now exactly `{ call, subscribe }`. Renderer `EventRouter`
(`src/renderer/app/EventRouter.ts`) validates each payload against `HostEvent`, routes by
`streamId`, returns a dispose from `subscribe`, tracks the last `seq` per stream and attaches
`gap` to an event that skipped ahead, and holds events for an unsubscribed stream in a 256-slot
ring before dropping the oldest with a warning. `src/renderer/app/replay.ts` feeds a recording
back into a router with the host idle; in dev it is on the window as `zvsReplay`.
`system.demoStream` was added to the contract and to `src/host/ipc/system.ts`, and the
placeholder renderer starts streams on a button and renders each counter. 11 new Node tests in
`apps/studio/tests/events.test.ts` and 4 Vitest tests in `packages/shared/tests/events.test.ts`;
all gates plus `pnpm test` and `pnpm format:check` pass. A Playwright run against the
production build confirmed `Object.keys(window.zvs)` is `['call', 'subscribe']` (both
functions), two concurrent demo streams each counting 1→10 and terminating with distinct
`streamId`s, a recording holding exactly the 22 events shown on screen with per-stream `seq`
0…10, no console errors or warnings, and no `zvsReplay` in a production build.

**Deviated from plan:** The user asked for no code comments, so the `TODO`s the task calls for
are prose instead: `packages/shared/README.md` records that `StepEventDto` and `LogLineDto`
wait on TASK_022 and `ApprovalRequestDto` on TASK_023, and a new `apps/studio/README.md`
documents the bus, the router, the recorder, the replayer, the two-function preload surface
and the backpressure gap. `RunOutcomeDto` was tightened now rather than left loose, since
`end` is the one variant this task actually produces. The router also warns on a `seq` that
goes *backwards* — it still delivers the event and keeps the high-water mark, since IPC
ordering makes that a host bug rather than a lost message. The replayer is a renderer module
plus a dev window hook rather than a standalone script: the renderer cannot read the disk, so
`zvsReplay.fromText(text)` / `fromUrl(url)` is the honest shape. `createEventRecorder` swallows
a write failure, logs it and stops recording — the smoke run proved that an unwritable
directory otherwise threw out of a timer callback and killed the host process. The demo stream
label shows `id.slice(-8)`, not the first 8 characters: two UUID v7 ids minted in the same
millisecond share their whole timestamp prefix and looked identical on screen.

**Next session should know:** A producer needs three things — `bus.openStream()`, `emit` with
a `HostEventDraft`, and exactly one `end`. Never stamp `seq` or `ts` yourself. On the renderer
side a store subscribes on start and calls the returned dispose on teardown (PASSPORT §13),
and must handle `event.gap` — it means events were lost and the store should refetch rather
than render a hole. The 256-event hold buffer is the only thing making "start a run, subscribe
a tick later" safe; a slower attach loses the head of the stream. There is no backpressure:
every event goes straight to every window uncoalesced, which is fine for a demo counter and
untested against a token firehose. `EventBus` and `EventRouter` both take their collaborators
as options (`senders`, `newId`, `clock`, `logger`, `buffer`) so both are testable under plain
Node with no Electron. `createHandlers()` with no arguments still works and builds a bus that
sends nowhere, which is what keeps the contract-coverage test cheap. The TASK_005 smoke lives
in the ignored `node_modules/.task002/smoke005.mjs`.
