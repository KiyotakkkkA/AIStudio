import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  BackgroundShellDto,
  BackgroundShellId,
  ConversationId,
  StreamId,
  contract,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import BackgroundShellStore from "../../src/renderer/features/backgroundShells/BackgroundShellStore.ts";
import {
  SHELL_GONE,
  STOP_FAILED,
  durationText,
  runningCount,
  statusLabel,
} from "../../src/renderer/features/backgroundShells/backgroundShellsCopy.ts";
import {
  BACKGROUND_MODE,
  SHELL_BACKGROUND_LEAD,
  SHELL_LEAD,
  processLead,
} from "../../src/renderer/features/chat/processCopy.ts";

const CONVERSATION = ConversationId.parse("0199ee11-7777-7111-8111-000000000001");
const OTHER = ConversationId.parse("0199ee11-7777-7111-8111-000000000002");

const id = (n: number) =>
  BackgroundShellId.parse(`0199ee11-7777-7111-8111-0000000001${String(n).padStart(2, "0")}`);
const stream = (n: number) =>
  StreamId.parse(`0199ee11-7777-7111-8111-0000000002${String(n).padStart(2, "0")}`);

const shell = (n: number, ownerId: string = CONVERSATION, startedAt = 1_000) =>
  BackgroundShellDto.parse({
    id: id(n),
    ownerId,
    program: "C:\\nodejs\\npx.cmd",
    argv: ["npx", "http-server"],
    cwd: "C:\\work",
    status: "running",
    exitCode: null,
    streamId: stream(n),
    startedAt,
    endedAt: null,
  });

function setup(existing: BackgroundShellDto[] = []) {
  const shells = new Map(existing.map((dto) => [dto.id, dto]));
  const outputs = new Map<string, { text: string; eventSeq: number }>();
  const bridge = createFakeBridge<Contract>()
    .handle("shells.list", ({ ownerId }) =>
      [...shells.values()].filter((dto) => dto.ownerId === ownerId),
    )
    .handle("shells.output", ({ id: shellId }) => {
      const dto = shells.get(shellId);
      if (dto === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "missing");
      const saved = outputs.get(shellId) ?? { text: "", eventSeq: 0 };
      return {
        shell: dto,
        text: saved.text,
        nextSeq: saved.text.length,
        truncated: false,
        eventSeq: saved.eventSeq,
      };
    })
    .handle("shells.kill", ({ id: shellId }) => {
      const dto = shells.get(shellId);
      if (dto === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "missing");
      const stopped = { ...dto, status: "stopped" as const, endedAt: 9_000 };
      shells.set(shellId, stopped);
      return stopped;
    });
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const clock = createFakeClock(61_000);
  const store = new BackgroundShellStore(createIpcClient(contract, bridge), events, clock);
  const emit = (streamId: StreamId, seq: number, body: Record<string, unknown>) => {
    events.dispatch({ streamId, seq, ts: 5_000, ...body });
  };
  const status = (n: number, value: string, ownerId: string = CONVERSATION, seq = 9) =>
    emit(stream(n), seq, {
      type: "step",
      step: {
        domain: "shell",
        id: id(n),
        ownerId,
        status: value,
        exitCode: value === "exited" ? 1 : null,
      },
    });
  return { bridge, store, shells, outputs, emit, status, clock };
}

const calls = (bridge: ReturnType<typeof setup>["bridge"], channel: string) =>
  bridge.calls.filter((call) => call.channel === channel).map((call) => call.payload);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("showing a dialog lists its processes and measures their run time", async () => {
  const { store, bridge } = setup([shell(1), shell(2, OTHER)]);
  await store.show(CONVERSATION);
  expect(calls(bridge, "shells.list")).toEqual([{ ownerId: CONVERSATION }]);
  expect(store.shells.map((dto) => dto.id)).toEqual([id(1)]);
  expect(store.running).toHaveLength(1);
  expect(store.elapsed(store.shells[0]!)).toBe(60_000);
});

test("a status step updates a known process and a new one reloads the list", async () => {
  const { store, shells, status, bridge } = setup([shell(1)]);
  await store.show(CONVERSATION);
  status(1, "exited");
  expect(store.find(id(1))?.status).toBe("exited");
  expect(store.find(id(1))?.exitCode).toBe(1);
  expect(store.find(id(1))?.endedAt).toBe(5_000);
  expect(store.running).toHaveLength(0);

  shells.set(id(2), shell(2));
  status(2, "running", CONVERSATION, 0);
  await settle();
  expect(store.find(id(2))?.status).toBe("running");
  expect(calls(bridge, "shells.list")).toHaveLength(2);

  status(3, "running", OTHER, 0);
  await settle();
  expect(calls(bridge, "shells.list")).toHaveLength(2);
});

test("viewing replays the kept output and follows only newer events", async () => {
  const { store, outputs, emit } = setup([shell(1)]);
  outputs.set(id(1), { text: "listening on 8099\n", eventSeq: 3 });
  await store.show(CONVERSATION);
  await store.view(id(1));
  expect(store.opened).toEqual([id(1)]);
  expect(store.viewing).toBe(id(1));

  const screen = { text: "" };
  store.connect(id(1), { write: (text) => (screen.text += text), reset: () => undefined });
  expect(screen.text).toBe("listening on 8099\n");
  emit(stream(1), 2, { type: "token", delta: "old\n" });
  emit(stream(1), 3, { type: "token", delta: "GET /\n" });
  expect(screen.text).toBe("listening on 8099\nGET /\n");
});

test("closing the viewed tab moves to a neighbour and leaving keeps the tabs", async () => {
  const { store } = setup([shell(1), shell(2)]);
  await store.show(CONVERSATION);
  await store.view(id(1));
  await store.view(id(2));
  store.leave();
  expect(store.viewing).toBeNull();
  expect(store.tabs.map((dto) => dto.id)).toEqual([id(1), id(2)]);
  await store.view(id(2));
  store.closeTab(id(2));
  expect(store.viewing).toBe(id(1));
  expect(store.opened).toEqual([id(1)]);
});

test("stopping replaces the process and a vanished one is worded", async () => {
  const { store, shells, bridge } = setup([shell(1)]);
  await store.show(CONVERSATION);
  await store.stop(id(1));
  expect(calls(bridge, "shells.kill")).toEqual([{ id: id(1) }]);
  expect(store.find(id(1))?.status).toBe("stopped");
  expect(store.isStopping(id(1))).toBe(false);

  shells.delete(id(1));
  await store.stop(id(1));
  expect(store.error).toBe(SHELL_GONE);
  bridge.handle("shells.kill", () => {
    throw new AppError(AppErrorCode.UNKNOWN, "boom");
  });
  shells.set(id(1), shell(1));
  await store.stop(id(1));
  expect(store.error).toBe(STOP_FAILED);
});

test("switching dialogs forgets the previous dialog's processes and tabs", async () => {
  const { store } = setup([shell(1), shell(2, OTHER)]);
  await store.show(CONVERSATION);
  await store.view(id(1));
  await store.show(OTHER);
  expect(store.shells.map((dto) => dto.id)).toEqual([id(2)]);
  expect(store.opened).toEqual([]);
  expect(store.viewing).toBeNull();
});

test("copy counts, statuses and durations are worded in Russian", () => {
  expect(runningCount(1)).toBe("1 фоновый процесс");
  expect(runningCount(3)).toBe("3 фоновых процесса");
  expect(runningCount(5)).toBe("5 фоновых процессов");
  expect(runningCount(11)).toBe("11 фоновых процессов");
  expect(statusLabel("exited", 2)).toBe("Завершён, код 2");
  expect(statusLabel("timed-out", null)).toBe("Превышено время");
  expect(durationText(42_000)).toBe("42 с");
  expect(durationText(125_000)).toBe("2 мин 5 с");
  expect(durationText(3_660_000)).toBe("1 ч 1 мин");
  const plan = { program: "npx", argv: ["npx"], cwd: "C:\\work", packages: [] };
  expect(processLead(plan)).toBe(SHELL_LEAD);
  expect(processLead({ ...plan, background: true })).toBe(SHELL_BACKGROUND_LEAD);
  expect(BACKGROUND_MODE.length).toBeGreaterThan(0);
});
