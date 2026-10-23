import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  ConversationId,
  StreamId,
  TerminalId,
  TerminalSessionDto,
  contract,
  type Contract,
  type TerminalShell,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import TerminalStore, {
  DEFAULT_COLS,
  DEFAULT_ROWS,
  trimBuffer,
} from "../../src/renderer/features/terminal/TerminalStore.ts";
import {
  NO_SHELL,
  SESSIONS_LIMIT,
  SESSION_GONE,
} from "../../src/renderer/features/terminal/terminalCopy.ts";

const CONVERSATION = ConversationId.parse("0199ee11-6666-7111-8111-000000000001");
const OTHER = ConversationId.parse("0199ee11-6666-7111-8111-000000000002");

const id = (n: number) =>
  TerminalId.parse(`0199ee11-6666-7111-8111-0000000001${String(n).padStart(2, "0")}`);
const stream = (n: number) =>
  StreamId.parse(`0199ee11-6666-7111-8111-0000000002${String(n).padStart(2, "0")}`);

const session = (n: number, conversationId = CONVERSATION, shell: TerminalShell = "pwsh") =>
  TerminalSessionDto.parse({
    id: id(n),
    conversationId,
    shell,
    title: `PowerShell ${String(n)}`,
    cwd: "C:\\work",
    status: "running",
    exitCode: null,
    streamId: stream(n),
    cols: DEFAULT_COLS,
    rows: DEFAULT_ROWS,
    createdAt: n,
  });

function setup(existing: TerminalSessionDto[] = []) {
  const sessions = new Map(existing.map((dto) => [dto.id, dto]));
  const scrollback = new Map<string, { text: string; nextSeq: number }>();
  let next = 10;
  const bridge = createFakeBridge<Contract>()
    .handle("terminal.list", ({ id: conversationId }) =>
      [...sessions.values()].filter((dto) => dto.conversationId === conversationId),
    )
    .handle("terminal.shells", () => ["pwsh", "cmd"])
    .handle("terminal.create", ({ conversationId, shell }) => {
      const dto = session(next++, conversationId, shell ?? "pwsh");
      sessions.set(dto.id, dto);
      return dto;
    })
    .handle("terminal.attach", ({ id: terminalId }) => {
      const dto = sessions.get(terminalId);
      if (dto === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "missing");
      const saved = scrollback.get(terminalId) ?? { text: "", nextSeq: 0 };
      return { session: dto, scrollback: saved.text, nextSeq: saved.nextSeq };
    })
    .handle("terminal.write", () => undefined)
    .handle("terminal.resize", () => undefined)
    .handle("terminal.clear", () => undefined)
    .handle("terminal.close", ({ id: terminalId }) => {
      sessions.delete(terminalId);
    })
    .handle("terminal.restart", ({ id: terminalId }) => {
      const dto = sessions.get(terminalId);
      if (dto === undefined) throw new AppError(AppErrorCode.NOT_FOUND, "missing");
      return dto;
    })
    .handle("terminal.rename", ({ id: terminalId, title }) => {
      const dto = { ...sessions.get(terminalId)!, title };
      sessions.set(terminalId, dto);
      return dto;
    });
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new TerminalStore(createIpcClient(contract, bridge), events);
  const seqs = new Map<string, number>();
  const emit = (streamId: StreamId, body: Record<string, unknown>, seq?: number) => {
    const at = seq ?? seqs.get(streamId) ?? 0;
    seqs.set(streamId, at + 1);
    events.dispatch({ streamId, seq: at, ts: 1, ...body });
  };
  const screen = () => {
    const screenState = { text: "", resets: 0 };
    return {
      state: screenState,
      write: (text: string) => {
        screenState.text += text;
      },
      reset: () => {
        screenState.text = "";
        screenState.resets += 1;
      },
    };
  };
  return { bridge, store, emit, scrollback, screen, sessions };
}

const calls = (bridge: ReturnType<typeof setup>["bridge"], channel: string) =>
  bridge.calls.filter((call) => call.channel === channel).map((call) => call.payload);

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("the buffer keeps the newest whole lines", () => {
  expect(trimBuffer("first\nsecond\nthird", 9)).toBe("third");
  expect(trimBuffer("short", 9)).toBe("short");
});

test("showing a dialog attaches its sessions and replays their scrollback", async () => {
  const { store, scrollback, screen, emit } = setup([session(1), session(2, OTHER)]);
  scrollback.set(id(1), { text: "PS C:\\work> ", nextSeq: 3 });
  await store.show(CONVERSATION);
  expect(store.sessions.map((dto) => dto.id)).toEqual([id(1)]);
  expect(store.activeId).toBe(id(1));

  const view = screen();
  store.connect(id(1), view);
  expect(view.state.text).toBe("PS C:\\work> ");

  emit(stream(1), { type: "token", delta: "stale" }, 2);
  emit(stream(1), { type: "token", delta: "dir\r\n" }, 3);
  expect(view.state.text).toBe("PS C:\\work> dir\r\n");
});

test("opening the dock starts a session only when the dialog has none", async () => {
  const empty = setup();
  await empty.store.show(CONVERSATION);
  await empty.store.toggle();
  expect(calls(empty.bridge, "terminal.create")).toEqual([
    { conversationId: CONVERSATION, shell: null, cols: DEFAULT_COLS, rows: DEFAULT_ROWS },
  ]);
  expect(empty.store.active?.id).toBe(id(10));

  const busy = setup([session(1)]);
  await busy.store.show(CONVERSATION);
  await busy.store.toggle();
  expect(calls(busy.bridge, "terminal.create")).toEqual([]);
  await busy.store.toggle();
  expect(busy.store.open).toBe(false);
});

test("opening the dock while the list loads does not start a second session", async () => {
  const { store, bridge } = setup([session(1)]);
  const showing = store.show(CONVERSATION);
  await store.toggle();
  await showing;
  expect(calls(bridge, "terminal.create")).toEqual([]);
  expect(store.sessions).toHaveLength(1);
});

test("a new session can use a chosen shell", async () => {
  const { store, bridge } = setup();
  await store.show(CONVERSATION);
  await store.loadShells();
  expect(store.shells).toEqual(["pwsh", "cmd"]);
  await store.create("cmd");
  expect(calls(bridge, "terminal.create")).toEqual([
    { conversationId: CONVERSATION, shell: "cmd", cols: DEFAULT_COLS, rows: DEFAULT_ROWS },
  ]);
  expect(store.active?.shell).toBe("cmd");
});

test("create failures map to copy by code", async () => {
  const missing = setup();
  missing.bridge.fail("terminal.create", AppErrorCode.NOT_FOUND, "Командная оболочка не найдена");
  await missing.store.show(CONVERSATION);
  await missing.store.create(null);
  expect(missing.store.error).toBe(NO_SHELL);

  const full = setup();
  full.bridge.fail("terminal.create", AppErrorCode.CONFLICT, "limit");
  await full.store.show(CONVERSATION);
  await full.store.create(null);
  expect(full.store.error).toBe(SESSIONS_LIMIT);
});

test("keys reach a running session only and an interrupt is Ctrl+C", async () => {
  const { store, bridge, emit } = setup([session(1)]);
  await store.show(CONVERSATION);
  store.write(id(1), "ls\r");
  store.interrupt(id(1));
  emit(stream(1), {
    type: "step",
    step: { domain: "terminal", id: id(1), status: "exited", exitCode: 1 },
  });
  store.write(id(1), "ignored");
  expect(calls(bridge, "terminal.write")).toEqual([
    { id: id(1), data: "ls\r" },
    { id: id(1), data: "\u0003" },
  ]);
  expect(store.active?.status).toBe("exited");
  expect(store.active?.exitCode).toBe(1);
});

test("a write to a finished shell shows that the session ended", async () => {
  const { store, bridge } = setup([session(1)]);
  bridge.fail("terminal.write", AppErrorCode.CONFLICT, "Сеанс терминала завершён");
  await store.show(CONVERSATION);
  store.write(id(1), "x");
  await settle();
  expect(store.error).toBe(SESSION_GONE);
});

test("resizes are sent once per size", async () => {
  const { store, bridge } = setup([session(1)]);
  await store.show(CONVERSATION);
  store.resize(id(1), 100, 20);
  store.resize(id(1), 100, 20);
  store.resize(id(1), 101, 20);
  expect(calls(bridge, "terminal.resize")).toEqual([
    { id: id(1), cols: 100, rows: 20 },
    { id: id(1), cols: 101, rows: 20 },
  ]);
});

test("a restart resets the screen and the buffer", async () => {
  const { store, emit, screen } = setup([session(1)]);
  await store.show(CONVERSATION);
  const view = screen();
  store.connect(id(1), view);
  emit(stream(1), { type: "token", delta: "old output" });
  emit(stream(1), {
    type: "step",
    step: { domain: "terminal", id: id(1), status: "running", exitCode: null },
  });
  emit(stream(1), { type: "token", delta: "PS> " });
  expect(view.state).toEqual({ text: "PS> ", resets: 1 });
  const again = screen();
  store.connect(id(1), again);
  expect(again.state.text).toBe("PS> ");
});

test("clearing empties the buffer and the screen", async () => {
  const { store, bridge, emit, screen } = setup([session(1)]);
  await store.show(CONVERSATION);
  const view = screen();
  store.connect(id(1), view);
  emit(stream(1), { type: "token", delta: "noise" });
  await store.clear(id(1));
  expect(view.state).toEqual({ text: "", resets: 1 });
  expect(calls(bridge, "terminal.clear")).toEqual([{ id: id(1) }]);
});

test("an ended stream removes the session and selects a neighbour", async () => {
  const { store, emit } = setup([session(1), session(2)]);
  await store.show(CONVERSATION);
  store.select(id(1));
  emit(stream(1), { type: "end", outcome: { status: "ok" } });
  expect(store.sessions.map((dto) => dto.id)).toEqual([id(2)]);
  expect(store.activeId).toBe(id(2));
});

test("closing a session asks the host and forgets it", async () => {
  const { store, bridge } = setup([session(1), session(2)]);
  await store.show(CONVERSATION);
  store.select(id(2));
  await store.close(id(2));
  expect(calls(bridge, "terminal.close")).toEqual([{ id: id(2) }]);
  expect(store.activeId).toBe(id(1));
});

test("switching dialogs stops listening to the previous sessions", async () => {
  const { store, emit, screen } = setup([session(1), session(2, OTHER)]);
  await store.show(CONVERSATION);
  const view = screen();
  store.connect(id(1), view);
  await store.show(OTHER);
  emit(stream(1), { type: "token", delta: "late" });
  expect(view.state.text).toBe("");
  expect(store.sessions.map((dto) => dto.id)).toEqual([id(2)]);
});

test("renaming trims the title and ignores blanks", async () => {
  const { store, bridge } = setup([session(1)]);
  await store.show(CONVERSATION);
  await store.rename(id(1), "   ");
  await store.rename(id(1), "  сервер ");
  expect(calls(bridge, "terminal.rename")).toEqual([{ id: id(1), title: "сервер" }]);
  expect(store.active?.title).toBe("сервер");
});
