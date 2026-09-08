# Studio app

The Electron shell: host process, preload bridge and renderer. Architecture and the decisions
behind it live in `/docs/PASSPORT.md`; this file records what the runtime pieces do.

## The event channel

Request/response goes through `@zvs/ipc`. Everything that streams — tokens, progress, step
events, log lines, approval requests — goes through the one `zvs:events` channel instead
(PASSPORT §7, D4), typed by `HostEvent` in `@zvs/shared`.

`src/host/platform/events.ts` is the host end. `createEventBus({ logger, senders, recorder })`
returns a bus; `openStream()` allocates a `StreamId` and hands back a handle whose `emit(draft)`
stamps `seq` and `ts` and whose `end(outcome)` sends the terminal `end` event and releases the
stream. `seq` starts at 0 and is per stream, never global. Emitting on an ended stream is a
programming error: it is logged at `error` and dropped, never thrown, because a producer must
not be able to crash the host. If no window is open the event is logged at `debug` and dropped
— the host never blocks on the UI being present. Everything reaching the bus is offered to the
recorder first, so a recording is complete even when nothing is listening.

`src/renderer/app/EventRouter.ts` is the renderer end. `createEventRouter()` validates each
incoming payload against `HostEvent` and routes it by `streamId` to whatever subscribed;
`connect(source)` attaches it to the preload's `subscribe` and returns a detach function.
`subscribe(streamId, handler)` returns a dispose function — stores subscribe on start and
dispose on teardown (PASSPORT §13). The router tracks the last `seq` per stream: an event
arriving beyond `last + 1` logs a warning and carries `gap` (the number of missing events) so
a store can refetch instead of rendering a hole; a `seq` that goes backwards warns as well and
is still delivered. Events for a `streamId` nobody has subscribed to yet are held in a small
ring (256 by default) and replayed to the first subscriber, which is what makes it safe to
start a run and attach a tick later; older held events are dropped with a warning.

Backpressure is a known gap. The bus sends every event straight to every window with no
coalescing; revisit only if a real producer floods the UI.

## Recording and replay

Set `ZVS_RECORD_EVENTS=1` and the host appends every event as JSONL to
`<userData>/logs/streams/<date>.jsonl`. `src/renderer/app/replay.ts` reads such a file back:
`replayRecording(router, text)` feeds it into an `EventRouter` with the host completely idle,
so a failing run can be reproduced in a fresh UI. In development the hook is on the window as
`zvsReplay.fromText(text)` / `zvsReplay.fromUrl(url)`.

## Preload

`src/preload/index.ts` exposes exactly two functions and nothing else: `call(channel, payload)`
and `subscribe(handler)`, which returns its own unsubscribe. No `ipcRenderer`, no `require`, no
filesystem, no shell. Adding a third is a security decision, not a convenience.

## Smoke channels

`system.ping` is the request/response round trip. `system.demoStream` is the streaming one: it
opens a stream, emits ten `progress` events and ends. Both are called from the placeholder
renderer and stay until real features replace them.
