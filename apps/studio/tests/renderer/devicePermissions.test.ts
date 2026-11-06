import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppGrantDto, contract, ConversationId, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createFakeClock } from "../../../../test/helpers/fakeClock.ts";
import DevicePermissionsStore, {
  groupAppPermissions,
} from "../../src/renderer/features/device/DevicePermissionsStore.ts";
import { APP_ADD_FAILED } from "../../src/renderer/features/device/deviceCopy.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import { createRootStore } from "../../src/renderer/stores/RootStore.ts";

test("command lists refresh after saving a feature switch, including enabling an absent command", async () => {
  let enabled = true;
  const reads: boolean[] = [];
  const bridge = createFakeBridge<Contract>()
    .handle("settings.set", ({ key, value }) => {
      if (key === "computer.enabled") enabled = value === true;
      return { key, value, updatedAt: 1 };
    })
    .handle("chat.commands", () => {
      reads.push(enabled);
      return enabled ? [{ command: "computer-use", description: "device" }] : [];
    });
  const store = createRootStore({
    ipc: createIpcClient(contract, bridge),
    events: createEventRouter(),
  });
  await store.appSettings.set("computer.enabled", false);
  for (let tick = 0; tick < 5; tick += 1) await Promise.resolve();
  expect(reads.at(-1)).toBe(false);
  expect(store.chat.availableCommands).toEqual([]);
  await store.appSettings.set("computer.enabled", true);
  for (let tick = 0; tick < 5; tick += 1) await Promise.resolve();
  expect(reads.at(-1)).toBe(true);
  expect(store.chat.availableCommands.map((command) => command.command)).toEqual(["computer-use"]);
  store.dispose();
});

test("composer commands follow feature switches immediately", () => {
  const bridge = createFakeBridge<Contract>();
  const store = new ChatStore(createIpcClient(contract, bridge), createEventRouter());
  let device = true;
  let browser = true;
  store.useCommandSwitch((command) =>
    command === "computer-use" ? device : command !== "browser-use" || browser,
  );
  store.commands = [
    { command: "browser-use", description: "browser" },
    { command: "computer-use", description: "device" },
    { command: "goal", description: "goal" },
  ];
  expect(store.availableCommands.map((command) => command.command)).toEqual([
    "browser-use",
    "computer-use",
    "goal",
  ]);
  device = false;
  browser = false;
  expect(store.availableCommands.map((command) => command.command)).toEqual(["goal"]);
  device = true;
  expect(store.availableCommands.map((command) => command.command)).toEqual([
    "computer-use",
    "goal",
  ]);
  store.dispose();
});

const first = ConversationId.parse("0199aa11-1111-7111-8111-111111111111");
const second = ConversationId.parse("0199aa11-2222-7111-8111-111111111111");
function grant(id: string, conversation: typeof first | null) {
  return AppGrantDto.parse({
    id,
    app: "notepad.exe",
    path: "C:\\Windows\\notepad.exe",
    icon: null,
    scope: conversation === null ? "always" : "conversation",
    conversation: conversation === null ? null : { id: conversation, title: conversation },
    grantedAt: 1,
    lastUsedAt: null,
  });
}
const global = grant("0199aa11-3333-7111-8111-111111111111", null);
const local = grant("0199aa11-4444-7111-8111-111111111111", first);
const other = grant("0199aa11-5555-7111-8111-111111111111", second);

test("groups one application across dialogs without merging its permanent permission", () => {
  const groups = groupAppPermissions([global, local, other]);
  expect(groups).toHaveLength(2);
  expect(groups.find((group) => group.scope === "conversation")?.grants).toEqual([local, other]);
  expect(groups.find((group) => group.scope === "always")?.grants).toEqual([global]);
});

test("saves selected dialogs and excludes them when switching to permanent access", async () => {
  const saved: unknown[] = [];
  const bridge = createFakeBridge<Contract>().handle("computer.addGrant", (input) => {
    saved.push(input);
    return [global, local, other];
  });
  const store = new DevicePermissionsStore(createIpcClient(contract, bridge), createFakeClock());
  store.draftPath = global.path;
  store.setScope("conversation");
  expect(store.canSave).toBe(false);
  store.toggleConversation(first);
  store.toggleConversation(second);
  await store.save();
  expect(saved[0]).toEqual({
    path: global.path,
    scope: "conversation",
    conversations: [first, second],
  });
  expect(store.draftPath).toBeNull();
  store.draftPath = global.path;
  store.toggleConversation(first);
  store.setScope("always");
  await store.save();
  expect(saved[1]).toEqual({ path: global.path, scope: "always", conversations: [] });
});

test("revoking the permanent grant keeps both dialog permissions and revoking one dialog keeps the other", async () => {
  let grants = [global, local, other];
  const bridge = createFakeBridge<Contract>()
    .handle("computer.grants", () => grants)
    .handle("computer.revoke", ({ id }) => {
      grants = grants.filter((grant) => grant.id !== id);
    });
  const store = new DevicePermissionsStore(createIpcClient(contract, bridge), createFakeClock());
  await store.load();
  store.select(store.groups.find((group) => group.scope === "conversation")!);
  await store.revoke(global.id);
  expect(store.grants).toEqual([local, other]);
  await store.revoke(local.id);
  expect(store.selectedGroup?.grants).toEqual([other]);
  await store.revoke(other.id);
  expect(store.selected).toBeNull();
});

test("picker cancellation leaves no draft; a failed save preserves the draft for retry", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("computer.pickApp", () => null)
    .handle("computer.addGrant", () => {
      throw Error("refused");
    });
  const store = new DevicePermissionsStore(createIpcClient(contract, bridge), createFakeClock());
  await store.pickApp();
  expect(store.draftPath).toBeNull();
  store.draftPath = global.path;
  await store.save();
  expect(store.error).toBe(APP_ADD_FAILED);
  expect(store.draftPath).toBe(global.path);
  expect(store.busy).toBe(false);
});
