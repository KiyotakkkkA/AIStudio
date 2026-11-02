import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  ChatGoalDto,
  ConversationDetailDto,
  ConversationDto,
  RunHandleDto,
  contract,
  type ChatGoalStepDto,
  type Contract,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import GoalStore from "../../src/renderer/features/chat/GoalStore.ts";
import {
  GOAL_CONTINUE_FAILED,
  GOAL_DONE,
  GOAL_PLAN_TOOL,
  GOAL_STEP_FIELD,
  GOAL_STEP_SUMMARY,
  GOAL_STEP_TOOL,
  planProgressText,
  GOAL_STOP_FAILED,
  goalMinutesText,
  goalRoundText,
  goalStatusText,
  goalTokensText,
} from "../../src/renderer/features/chat/goalCopy.ts";
import { toolCallView } from "../../src/renderer/features/chat/toolCallView.ts";
import {
  BASE_TOOLS_TITLE,
  COMMAND_TOOLS_TITLE,
  toolSections,
} from "../../src/renderer/features/chat/toolSets.ts";
import { provider } from "./providerFixtures.ts";

const ID = "0199ff11-8888-7111-8111-000000000001";
const OTHER = "0199ff11-8888-7111-8111-000000000002";
const handle = RunHandleDto.parse({
  id: "0199ff11-8888-7111-8111-000000000010",
  streamId: "0199ff11-8888-7111-8111-000000000011",
});
const budget = { rounds: 40, tokens: 400_000, minutes: 30 };

function goal(overrides: Partial<ChatGoalDto> = {}): ChatGoalDto {
  return ChatGoalDto.parse({
    text: "Добавить тесты",
    status: "active",
    reason: null,
    budget,
    usage: { rounds: 0, tokens: 0, elapsedMs: 0 },
    summary: null,
    startedAt: 10,
    endedAt: null,
    ...overrides,
  });
}

function conversation(value: ChatGoalDto | null): ConversationDto {
  return ConversationDto.parse({
    id: ID,
    title: "Цель",
    providerId: "0199ff11-1111-7111-8111-000000000001",
    modelId: "m",
    goal: value,
    createdAt: 1,
    updatedAt: 1,
  });
}

function step(overrides: Partial<ChatGoalStepDto> = {}): Record<string, unknown> {
  return {
    domain: "goal",
    conversationId: ID,
    status: "active",
    reason: null,
    round: 3,
    usage: { rounds: 3, tokens: 1500, elapsedMs: 120_000 },
    budget,
    ...overrides,
  };
}

function setup(initial: ChatGoalDto | null, server: ChatGoalDto | null = initial) {
  let current = conversation(initial);
  let remote = conversation(server);
  const refreshed: string[] = [];
  const bridge = createFakeBridge<Contract>()
    .handle("chat.goal.stop", () => {
      remote = conversation({
        ...(remote.goal ?? goal()),
        status: "stopped",
        reason: "user",
        endedAt: 99,
      });
      return remote;
    })
    .handle("chat.goal.continue", () => {
      remote = conversation({ ...(remote.goal ?? goal()), status: "active", reason: null });
      return handle;
    });
  const clock = createFakeClock(1_000);
  const store = new GoalStore(
    createIpcClient(contract, bridge),
    {
      conversation: () => current,
      refresh: async (id) => {
        refreshed.push(id);
        current = remote;
        await Promise.resolve();
      },
      replace: (value) => {
        current = value;
      },
    },
    clock,
  );
  return {
    store,
    bridge,
    clock,
    refreshed,
    current: () => current,
    setServer(value: ChatGoalDto | null) {
      remote = conversation(value);
    },
  };
}

test("goal steps update the round, tokens and status over the stored goal", () => {
  const { store } = setup(goal());
  expect(store.apply(step())).toBe(true);
  expect(store.goal?.usage).toEqual({ rounds: 3, tokens: 1500, elapsedMs: 120_000 });
  expect(store.active).toBe(true);
  expect(store.apply({ domain: "tool-call" })).toBe(false);
  store.apply(step({ conversationId: OTHER, round: 9 } as Partial<ChatGoalStepDto>));
  expect(store.goal?.usage.rounds).toBe(3);
  store.apply(step({ status: "paused", reason: "rounds", round: 40 }));
  expect(store.paused).toBe(true);
  expect(goalStatusText(store.goal!)).toBe("Приостановлено: исчерпан лимит раундов");
});

test("a step for a goal the dialog does not know yet reloads the dialog once", async () => {
  const { store, refreshed, setServer } = setup(null);
  setServer(goal());
  store.apply(step({ round: 0, usage: { rounds: 0, tokens: 0, elapsedMs: 0 } }));
  store.apply(step({ round: 0, usage: { rounds: 0, tokens: 0, elapsedMs: 0 } }));
  await vi.waitFor(() => expect(store.goal?.text).toBe("Добавить тесты"));
  expect(refreshed).toEqual([ID]);
});

test("elapsed minutes keep counting between rounds while the goal runs", () => {
  const { store, clock } = setup(goal());
  store.apply(step());
  clock.advance(65_000);
  expect(store.elapsedMs).toBe(185_000);
  expect(goalMinutesText(store.elapsedMs)).toBe(`${(3).toLocaleString("ru-RU")} мин`);
  store.apply(step({ status: "paused", reason: "minutes", round: 4 }));
  clock.advance(600_000);
  expect(store.elapsedMs).toBe(120_000);
});

test("stop replaces the goal with the stopped one and drops the live overlay", async () => {
  const { store, bridge, current } = setup(goal());
  store.apply(step());
  await store.stop();
  expect(bridge.calls).toContainEqual({
    channel: "chat.goal.stop",
    payload: { conversationId: ID },
  });
  expect(store.live).toBeNull();
  expect(current().goal?.status).toBe("stopped");
  expect(store.goal?.status).toBe("stopped");
  expect(goalStatusText(store.goal!)).toBe("Остановлено");
});

test("a failed stop or continue shows the copy and leaves the goal as it was", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.goal.stop", () => {
      throw new AppError(AppErrorCode.CONFLICT, "x");
    })
    .handle("chat.goal.continue", () => {
      throw new AppError(AppErrorCode.CONFLICT, "x");
    });
  const store = new GoalStore(createIpcClient(contract, bridge), {
    conversation: () => conversation(goal({ status: "paused", reason: "tokens" })),
    refresh: () => Promise.resolve(),
    replace: () => undefined,
  });
  await store.stop();
  expect(store.error).toBe(GOAL_STOP_FAILED);
  expect(await store.resume()).toBeNull();
  expect(store.error).toBe(GOAL_CONTINUE_FAILED);
  expect(store.goal?.status).toBe("paused");
});

test("continue returns the new turn and reloads the renewed goal", async () => {
  const paused = goal({
    status: "paused",
    reason: "rounds",
    usage: { rounds: 40, tokens: 9, elapsedMs: 0 },
  });
  const { store, refreshed } = setup(paused);
  store.apply(step({ status: "paused", reason: "rounds", round: 40, usage: paused.usage }));
  expect(await store.resume()).toEqual(handle);
  expect(refreshed).toEqual([ID]);
  expect(store.live).toBeNull();
  expect(store.active).toBe(true);
});

test("a reached goal shows once and can be dismissed until a new goal starts", () => {
  const done = goal({ status: "done", summary: "Готово", endedAt: 50 });
  const context = setup(done);
  expect(context.store.visible).toBe(true);
  expect(goalStatusText(context.store.goal!)).toBe(GOAL_DONE);
  context.store.dismiss();
  expect(context.store.visible).toBe(false);
  const next = setup(goal({ startedAt: 60 }));
  expect(next.store.visible).toBe(true);
});

test("progress copy formats counts for ru-RU", () => {
  expect(goalRoundText(12, 40)).toBe(
    `Раунд ${(12).toLocaleString("ru-RU")} из ${(40).toLocaleString("ru-RU")}`,
  );
  expect(goalTokensText(48_200, 400_000)).toContain((400_000).toLocaleString("ru-RU"));
  expect(goalStatusText({ status: "stopped", reason: "repetition" })).toBe(
    "Остановлено: повторяющиеся вызовы",
  );
  expect(goalStatusText({ status: "failed", reason: "error" })).toBe("Ошибка");
});

test("the completion call shows its summary and evidence", () => {
  expect(
    toolCallView({
      key: "goal.complete",
      arguments: { summary: "Готово", evidence: "cargo test: ok" },
      meta: null,
    }),
  ).toEqual({ kind: "goal", summary: "Готово", evidence: "cargo test: ok" });
});

function chat(active: ChatGoalDto | null) {
  const saved = provider();
  const detail = ConversationDetailDto.parse({
    ...conversation(active),
    providerId: saved.id,
    modelId: saved.models[0]!.externalId,
    messages: [],
  });
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [detail])
    .handle("chat.conversations.get", () => detail)
    .handle("chat.attach", () => null)
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => [])
    .handle("chat.send", () => handle);
  const store = new ChatStore(
    createIpcClient(contract, bridge),
    createEventRouter({ logger: { log: vi.fn() } }),
  );
  return { store, bridge, detail };
}

test("the composer refuses an empty goal with a reason and locks while a goal runs", async () => {
  const { store, bridge, detail } = chat(null);
  await store.mount();
  await store.select(detail.id);
  store.composer.addCommand("goal");
  expect(store.goalCommand).toBe(true);
  await store.send();
  expect(store.goalEmpty).toBe(true);
  expect(bridge.calls.some((call) => call.channel === "chat.send")).toBe(false);
  store.typeText("Добавить тесты");
  expect(store.goalEmpty).toBe(false);
  expect(store.canSend).toBe(true);
  store.dispose();

  const running = chat(goal());
  await running.store.mount();
  await running.store.select(running.detail.id);
  running.store.typeText("Ещё вопрос");
  expect(running.store.goals.active).toBe(true);
  expect(running.store.canSend).toBe(false);
  running.store.dispose();
});

const plan = [
  { id: "p1", title: "Найти модули", status: "done", stepIds: [], summary: "4 модуля" },
  { id: "p2", title: "Добавить тесты", status: "running", stepIds: [], summary: null },
] as const;

test("the plan follows the live steps and counts what is done", () => {
  const { store } = setup(goal());
  expect(store.plan).toEqual([]);
  store.apply(step({ plan: [...plan] } as Partial<ChatGoalStepDto>));
  expect(store.plan.map((item) => item.title)).toEqual(["Найти модули", "Добавить тесты"]);
  expect(store.planDone).toBe(1);
  expect(planProgressText(store.planDone, store.plan.length)).toBe(
    `${(1).toLocaleString("ru-RU")} из ${(2).toLocaleString("ru-RU")}`,
  );
  store.apply(
    step({
      round: 4,
      plan: plan.map((item) => ({ ...item, status: "done" as const })),
    } as Partial<ChatGoalStepDto>),
  );
  expect(store.planDone).toBe(2);
});

test("plan tool calls show their steps and the step summary", () => {
  expect(
    toolCallView({ key: GOAL_PLAN_TOOL, arguments: { steps: ["А", "Б"] }, meta: null }),
  ).toEqual({
    kind: "fields",
    fields: [
      { label: `${GOAL_STEP_FIELD} ${(1).toLocaleString("ru-RU")}`, value: "А" },
      { label: `${GOAL_STEP_FIELD} ${(2).toLocaleString("ru-RU")}`, value: "Б" },
    ],
  });
  expect(
    toolCallView({ key: GOAL_STEP_TOOL, arguments: { summary: "готово" }, meta: null }),
  ).toEqual({ kind: "fields", fields: [{ label: GOAL_STEP_SUMMARY, value: "готово" }] });
  expect(
    toolSections([
      { key: GOAL_PLAN_TOOL, tokens: 1, source: "command" },
      { key: "fs.read", tokens: 1, source: "base" },
    ]).map((section) => section.title),
  ).toEqual([BASE_TOOLS_TITLE, COMMAND_TOOLS_TITLE]);
});
