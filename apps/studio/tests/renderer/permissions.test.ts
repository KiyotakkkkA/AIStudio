import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type AccessListDto,
  type AppGrantDto,
  type Contract,
  type ConversationId,
  type FolderGrantDto,
  type FolderGrantId,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import PermissionsStore from "../../src/renderer/features/permissions/PermissionsStore.ts";
import {
  ALREADY_REVOKED,
  LOAD_ERROR,
  boundLabel,
  lastUsedLabel,
  revokeConsequence,
} from "../../src/renderer/features/permissions/permissionsPresentation.ts";

const CHAT = "0199dd11-2222-7111-8111-000000000001" as ConversationId;

function id(n: number): FolderGrantId {
  return `0199dd11-6666-7111-8111-${String(n).padStart(12, "0")}` as FolderGrantId;
}

function grant(n: number, overrides: Partial<FolderGrantDto> = {}): FolderGrantDto {
  return {
    id: id(n),
    path: `D:\\work\\project-${String(n)}`,
    scope: "always",
    conversation: null,
    boundConversations: [],
    grantedAt: 1_000,
    lastUsedAt: null,
    ...overrides,
  };
}

const APP: AppGrantDto = {
  id: "0199dd11-7777-7111-8111-000000000001",
  app: "notepad.exe",
  path: "C:/Windows/notepad.exe",
  icon: null,
  scope: "always",
  conversation: null,
  grantedAt: 1_000,
  lastUsedAt: null,
};

function listed(folders: FolderGrantDto[], apps: AppGrantDto[] = []): AccessListDto {
  return {
    policy: { mode: "always-ask", judge: null },
    folders,
    sites: {
      defaults: { browse: "allow", click: "ask", type: "ask", submit: "ask", download: "ask" },
      sites: [{ origin: "https://shop.example", rules: { click: "allow" }, updatedAt: 1 }],
    },
    apps,
    tools: [
      {
        id: "t1",
        tool: "mail.send",
        displayName: "mail.send",
        scope: "skill:notes",
        tier: "auto",
        grantedBy: "user",
        grantedAt: 1,
      },
    ],
    rules: { rules: [], unconfirmed: [] },
  };
}

function setup(grants: FolderGrantDto[], apps: AppGrantDto[] = []) {
  const bridge = createFakeBridge<Contract>();
  let stored = [...grants];
  let storedApps = [...apps];
  bridge
    .handle("access.list", () => listed(stored, storedApps))
    .handle("folders.revoke", ({ id: revoked }) => {
      stored = stored.filter((item) => item.id !== revoked);
    })
    .handle("computer.revoke", ({ id: revoked }) => {
      storedApps = storedApps.filter((item) => item.id !== revoked);
    })
    .handle("browserSites.remove", () => listed(stored).sites);
  const clock = createFakeClock();
  const store = new PermissionsStore(createIpcClient(contract, bridge), { clock });
  return { bridge, clock, store };
}

describe("PermissionsStore", () => {
  test("splits grants into lasting and dialog sections and filters by path or chat", async () => {
    const { store } = setup([
      grant(1),
      grant(2, {
        scope: "conversation",
        conversation: { id: CHAT, title: "Отчёт" },
        path: "D:\\docs\\report",
      }),
    ]);
    await store.load();
    expect(store.always.map((item) => item.id)).toEqual([id(1)]);
    expect(store.dialog.map((item) => item.id)).toEqual([id(2)]);
    expect(store.selectedId).toBe(id(1));
    store.setQuery("отчёт");
    expect(store.visible.map((item) => item.id)).toEqual([id(2)]);
    store.setQuery("PROJECT");
    expect(store.visible.map((item) => item.id)).toEqual([id(1)]);
  });

  test("revoking reloads the list and moves the selection", async () => {
    const { bridge, store } = setup([grant(1), grant(2)]);
    await store.load();
    await store.revoke(id(1));
    expect(bridge.calls.filter((call) => call.channel === "folders.revoke")).toEqual([
      { channel: "folders.revoke", payload: { id: id(1) } },
    ]);
    expect(store.grants.map((item) => item.id)).toEqual([id(2)]);
    expect(store.selectedId).toBe(id(2));
    expect(store.revoking).toBe(false);
    expect(store.error).toBeNull();
  });

  test("an already revoked grant is reported by code and dropped on reload", async () => {
    const { bridge, store } = setup([grant(1)]);
    await store.load();
    bridge
      .fail("folders.revoke", AppErrorCode.NOT_FOUND, "Разрешение не найдено")
      .handle("access.list", () => listed([]));
    await store.revoke(id(1));
    expect(store.error).toBe(ALREADY_REVOKED);
    expect(store.isEmpty).toBe(true);
    expect(store.selectedId).toBeNull();
  });

  test("a failed load keeps the previous list and reports it", async () => {
    const { bridge, store } = setup([grant(1)]);
    await store.load();
    bridge.fail("access.list", AppErrorCode.DB_ERROR, "db");
    await store.load();
    expect(store.error).toBe(LOAD_ERROR);
    expect(store.grants).toHaveLength(1);
  });

  test("one list feeds every tab and counts each kind", async () => {
    const { store } = setup([grant(1)], [APP]);
    expect(store.counts.folders).toBeNull();
    await store.load();
    expect(store.counts).toEqual({ folders: 1, sites: 1, apps: 1, tools: 1, rules: 0 });
    store.setTab("apps");
    expect(store.tab).toBe("apps");
    expect(store.access?.policy.mode).toBe("always-ask");
  });

  test("revoking an app or a site goes to its own channel and reloads", async () => {
    const { bridge, store } = setup([grant(1)], [APP]);
    await store.load();
    await store.revokeApp(APP.id);
    expect(store.apps).toEqual([]);
    await store.removeSite("https://shop.example");
    expect(bridge.calls.filter((call) => call.channel === "browserSites.remove")).toEqual([
      { channel: "browserSites.remove", payload: { origin: "https://shop.example" } },
    ]);
    expect(bridge.calls.filter((call) => call.channel === "access.list")).toHaveLength(3);
    expect(store.error).toBeNull();
  });

  test("labels read the clock the store was given", async () => {
    const { clock, store } = setup([grant(1, { lastUsedAt: 0 })]);
    clock.set(5 * 60_000);
    await store.load();
    expect(lastUsedLabel(store.grants[0]!, store.now)).toBe("Использовано 5 м назад");
    expect(lastUsedLabel(grant(2), store.now)).toBe("Не использовалось");
  });
});

test("the revoke confirmation names the folder and what happens to bound chats", () => {
  const bound = grant(1, {
    boundConversations: [
      { id: CHAT, title: "A" },
      { id: "0199dd11-2222-7111-8111-000000000002" as ConversationId, title: "B" },
    ],
  });
  expect(boundLabel(bound)).toBe("Рабочая папка: 2 диалога");
  expect(revokeConsequence(bound)).toContain(bound.path);
  expect(revokeConsequence(bound)).toContain("Привязанные диалоги сохранят");
  expect(revokeConsequence(grant(3))).toBe(`Доступ к папке ${grant(3).path} будет закрыт.`);
});
