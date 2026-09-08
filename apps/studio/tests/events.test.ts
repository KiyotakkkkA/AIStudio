import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test, { after } from "node:test";
import { HostEvent, type StreamId } from "@zvs/shared";
import { createEventBus, type WindowSender } from "../src/host/platform/events.ts";
import { createEventRecorder, recordingEnabled } from "../src/host/platform/eventRecorder.ts";
import { createHandlers } from "../src/host/ipc/index.ts";
import { SettingService } from "../src/host/services/SettingService.ts";
import { temporaryDatabase } from "./database.ts";
import { createEventRouter, type RoutedEvent } from "../src/renderer/app/EventRouter.ts";
import { replayRecording } from "../src/renderer/app/replay.ts";

interface Recorded {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

function recordingLogger() {
  const entries: Recorded[] = [];
  return {
    entries,
    log(level: string, _scope: string, message: string, fields: Record<string, unknown> = {}) {
      entries.push({ level, message, fields });
    },
    close() {},
  };
}

function fakeSender() {
  const sent: HostEvent[] = [];
  const sender: WindowSender = {
    isDestroyed: () => false,
    send(_channel, payload) {
      sent.push(payload as HostEvent);
    },
  };
  return { sent, sender };
}

const uuids = (count: number): string[] =>
  Array.from(
    { length: count },
    (_value, index) => `0199aa11-1111-7111-8111-${String(index + 1).padStart(12, "0")}`,
  );

const found = (entries: Recorded[], pattern: RegExp): Recorded[] =>
  entries.filter((entry) => pattern.test(entry.message));

test("seq is monotonic per stream and never shared between streams", () => {
  const { sent, sender } = fakeSender();
  const pool = uuids(2);
  let next = 0;
  const bus = createEventBus({ senders: () => [sender], newId: () => pool[next++]! });
  const first = bus.openStream();
  const second = bus.openStream();
  first.emit({ type: "token", delta: "a" });
  second.emit({ type: "token", delta: "b" });
  first.emit({ type: "token", delta: "c" });
  second.end({ status: "ok" });
  first.end({ status: "cancelled" });

  const seqOf = (id: StreamId) => sent.filter((event) => event.streamId === id).map((e) => e.seq);
  assert.deepEqual(seqOf(first.id), [0, 1, 2]);
  assert.deepEqual(seqOf(second.id), [0, 1]);
  assert.equal(bus.open, 0);
  for (const event of sent) assert.deepEqual(HostEvent.parse(event), event);
});

test("emitting on an ended stream is logged, not thrown", () => {
  const { sent, sender } = fakeSender();
  const logger = recordingLogger();
  const bus = createEventBus({ senders: () => [sender], logger });
  const stream = bus.openStream();
  stream.end({ status: "ok" });
  stream.emit({ type: "token", delta: "late" });
  stream.end({ status: "ok" });

  assert.equal(sent.length, 1);
  assert.equal(stream.closed, true);
  const errors = logger.entries.filter((entry) => entry.level === "error");
  assert.equal(errors.length, 2);
  assert.match(errors[0]!.message, /already ended/);
});

test("with no window open events are dropped at debug level but still recorded", () => {
  const logger = recordingLogger();
  const recorded: HostEvent[] = [];
  const bus = createEventBus({
    senders: () => [{ isDestroyed: () => true, send: () => assert.fail("destroyed window") }],
    logger,
    recorder: {
      append: (event) => {
        recorded.push(event);
      },
      close() {},
    },
  });
  const stream = bus.openStream();
  stream.emit({ type: "progress", done: 1, total: 2 });
  stream.end({ status: "ok" });

  assert.equal(recorded.length, 2);
  assert.equal(found(logger.entries, /no window/).length, 2);
});

test("the router delivers to the matching subscriber and to no other", () => {
  const router = createEventRouter({ logger: recordingLogger() });
  const [a, b] = uuids(2) as [string, string];
  const mine: RoutedEvent[] = [];
  const theirs: RoutedEvent[] = [];
  router.subscribe(a as StreamId, (event) => mine.push(event));
  router.subscribe(b as StreamId, (event) => theirs.push(event));

  router.dispatch({ type: "token", streamId: a, seq: 0, ts: 1, delta: "x" });
  router.dispatch({ type: "token", streamId: b, seq: 0, ts: 1, delta: "y" });
  router.dispatch({ type: "token", streamId: a, seq: 1, ts: 1, delta: "z" });

  assert.deepEqual(
    mine.map((event) => (event.type === "token" ? event.delta : "")),
    ["x", "z"],
  );
  assert.equal(theirs.length, 1);
});

test("a gap warns and is surfaced on the event, an out of order event warns too", () => {
  const logger = recordingLogger();
  const router = createEventRouter({ logger });
  const [a] = uuids(1) as [string];
  const seen: RoutedEvent[] = [];
  router.subscribe(a as StreamId, (event) => seen.push(event));

  router.dispatch({ type: "token", streamId: a, seq: 0, ts: 1, delta: "0" });
  router.dispatch({ type: "token", streamId: a, seq: 4, ts: 1, delta: "4" });
  router.dispatch({ type: "token", streamId: a, seq: 2, ts: 1, delta: "2" });

  assert.deepEqual(
    seen.map((event) => event.gap),
    [undefined, 3, undefined],
  );
  const missed = found(logger.entries, /Missed events/);
  assert.equal(missed.length, 1);
  assert.equal(missed[0]!.fields.gap, 3);
  assert.equal(found(logger.entries, /out of order/).length, 1);
});

test("dispose stops delivery and leaves no handler behind", () => {
  const router = createEventRouter({ logger: recordingLogger() });
  const [a] = uuids(1) as [string];
  const seen: RoutedEvent[] = [];
  const dispose = router.subscribe(a as StreamId, (event) => seen.push(event));

  router.dispatch({ type: "token", streamId: a, seq: 0, ts: 1, delta: "0" });
  dispose();
  dispose();
  router.dispatch({ type: "token", streamId: a, seq: 1, ts: 1, delta: "1" });

  assert.equal(seen.length, 1);
});

test("events for an unknown stream are held until a subscriber attaches, then dropped", () => {
  const logger = recordingLogger();
  const router = createEventRouter({ logger, buffer: 2 });
  const [a] = uuids(1) as [string];
  for (let seq = 0; seq < 4; seq++)
    router.dispatch({ type: "token", streamId: a, seq, ts: 1, delta: String(seq) });

  const seen: RoutedEvent[] = [];
  router.subscribe(a as StreamId, (event) => seen.push(event));

  assert.deepEqual(
    seen.map((event) => event.seq),
    [2, 3],
  );
  assert.equal(found(logger.entries, /Dropped a held event/).length, 2);
});

test("payloads that do not match the contract never reach a subscriber", () => {
  const logger = recordingLogger();
  const router = createEventRouter({ logger });
  const [a] = uuids(1) as [string];
  const seen: RoutedEvent[] = [];
  router.subscribe(a as StreamId, (event) => seen.push(event));

  router.dispatch({ type: "token", streamId: a, seq: -1, ts: 1, delta: "x" });
  router.dispatch("nonsense");

  assert.equal(seen.length, 0);
  assert.equal(found(logger.entries, /does not match the contract/).length, 2);
});

test("a recorded session replays into the router as the same sequence", async () => {
  const directory = mkdtempSync(join(tmpdir(), "studio-streams-"));
  try {
    const recorder = createEventRecorder({ directory, clock: () => 1_700_000_000_000 });
    const pool = uuids(1);
    const bus = createEventBus({ recorder, newId: () => pool[0]! });
    const stream = bus.openStream();
    const emitted: number[] = [];
    for (let index = 1; index <= 5; index++) {
      stream.emit({ type: "progress", done: index, total: 5 });
      emitted.push(index);
    }
    stream.end({ status: "ok" });
    bus.dispose();

    const files = readdirSync(directory);
    assert.deepEqual(files, ["2023-11-14.jsonl"]);
    const text = readFileSync(join(directory, files[0]!), "utf8");

    const router = createEventRouter({ logger: recordingLogger() });
    const seen: RoutedEvent[] = [];
    router.subscribe(stream.id, (event) => seen.push(event));
    const count = await replayRecording(router, text);

    assert.equal(count, 6);
    assert.deepEqual(
      seen.map((event) => event.seq),
      [0, 1, 2, 3, 4, 5],
    );
    assert.deepEqual(
      seen.flatMap((event) => (event.type === "progress" ? [event.done] : [])),
      emitted,
    );
    assert.equal(seen.at(-1)?.type, "end");
    assert.equal(
      seen.some((event) => event.gap !== undefined),
      false,
    );
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("recording is opt in through the environment flag", () => {
  assert.equal(recordingEnabled({}), false);
  assert.equal(recordingEnabled({ ZVS_RECORD_EVENTS: "0" }), false);
  assert.equal(recordingEnabled({ ZVS_RECORD_EVENTS: "1" }), true);
});

const database = temporaryDatabase();
const settings = new SettingService({ data: database.client });
after(() => database.dispose());

test("system.demoStream counts to the requested total and terminates", async () => {
  const { sent, sender } = fakeSender();
  const bus = createEventBus({ senders: () => [sender] });
  const handlers = createHandlers({ events: bus, intervalMs: 0, settings });
  const { streamId } = await handlers["system.demoStream"]({ steps: 10 });

  while (bus.open > 0) await new Promise((resolve) => setTimeout(resolve, 5));

  assert.equal(bus.open, 0);
  assert.deepEqual(
    sent.flatMap((event) => (event.type === "progress" ? [event.done] : [])),
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
  );
  assert.equal(sent.at(-1)?.type, "end");
  assert.equal(
    sent.every((event) => event.streamId === streamId),
    true,
  );
});
