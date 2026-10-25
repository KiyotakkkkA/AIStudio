import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  AppError,
  contract,
  SKILL_FILE_MAX_BYTES,
  type Contract,
  McpServerDto,
  type SkillDetailDto,
  type ToolDto,
} from "@zvs/shared";
import serverFixtures from "../../../../packages/shared/fixtures/mcp/McpServerDto.valid.json";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { SkillFormVm } from "../../src/renderer/features/skills/SkillFormVm.ts";
import SkillStore, {
  MANIFEST_PATH,
  modelOf,
  modelValue,
} from "../../src/renderer/features/skills/SkillStore.ts";
import {
  issueCopy,
  skillErrorCopy,
  slugify,
  SKILL_INVALID_BANNER,
  UPDATE_GUARD,
  updatedAllNotice,
  updatedNotice,
} from "../../src/renderer/features/skills/skillPresentation.ts";
import { markdownLines } from "../../src/renderer/features/skills/markdownTokens.ts";

const SKILL_ID = "0199aa11-1111-7111-8111-111111111111" as SkillDetailDto["id"];
const SERVER_ID = "0199aa11-1111-7111-8111-222222222222";

function detail(overrides: Partial<SkillDetailDto> = {}): SkillDetailDto {
  return {
    id: SKILL_ID,
    key: "release-notes",
    name: "Заметки о выпуске",
    description: "Составляет заметки о выпуске",
    allowedTools: ["demo.lookup"],
    modelOverride: null,
    enabled: true,
    source: "local",
    status: "ready",
    issues: [],
    fileCount: 1,
    version: null,
    author: null,
    update: null,
    createdAt: 1_000,
    updatedAt: 1_000,
    instructions: "# Заметки",
    files: [{ path: "references/style.md", size: 120 }],
    ...overrides,
  };
}

function tool(n: number, key: string, overrides: Partial<ToolDto> = {}): ToolDto {
  return {
    id: `0199aa11-1111-7111-8111-00000000000${String(n)}` as ToolDto["id"],
    key,
    source: "builtin",
    sourceRef: null,
    displayName: key,
    description: `${key} description`,
    inputSchema: null,
    readOnly: true,
    destructive: false,
    tier: "auto",
    enabled: true,
    available: true,
    lastUsedAt: null,
    callCount: 0,
    registeredAt: 1_000,
    updatedAt: 1_000,
    ...overrides,
  };
}

function server(): McpServerDto {
  const [fixture] = serverFixtures as { value: unknown }[];
  return McpServerDto.parse({
    ...(fixture?.value as object),
    id: SERVER_ID,
    name: "qdrant",
    enabled: true,
    status: "failed",
  });
}

function setup(tools: ToolDto[] = []) {
  const bridge = createFakeBridge<Contract>()
    .handle("skills.list", () => [detail()])
    .handle("skills.get", () => detail())
    .handle("tools.list", () => tools)
    .handle("mcp.list", () => [server()])
    .handle("providers.list", () => []);
  const store = new SkillStore(createIpcClient(contract, bridge));
  return { store, bridge };
}

test("the form validates the fields the host validates, with the same rules", () => {
  const vm = new SkillFormVm();
  expect(vm.validate()).toBe(false);
  expect(vm.errors).toEqual({
    key: issueCopy("required"),
    name: issueCopy("required"),
    description: issueCopy("required"),
    instructions: issueCopy("required"),
  });

  vm.setName("Отчёт о провайдерах");
  expect(vm.key).toBe("otchet-o-provayderah");
  vm.setKey("Bad Key");
  vm.setDescription("Проверяет провайдеров");
  vm.setInstructions("Шаги.");
  vm.addFile({ path: "notes.md", size: 10, content: "" });
  vm.addFile({ path: "assets/big.bin", size: SKILL_FILE_MAX_BYTES + 1, content: "" });
  expect(vm.validate()).toBe(false);
  expect(vm.errors).toEqual({
    key: issueCopy("pattern"),
    "files.0": issueCopy("path"),
    "files.1": issueCopy("fileTooLarge"),
  });

  vm.setKey("provider-report");
  vm.removeFile("notes.md");
  vm.removeFile("assets/big.bin");
  expect(vm.validate()).toBe(true);
});

test("host issues land on the fields they name and a draft tracks its changes", () => {
  const vm = new SkillFormVm({ skill: detail() });
  expect(vm.dirty).toBe(false);
  vm.applyIssues([
    { field: "allowedTools.0", rule: "toolOff" },
    { field: "key", rule: "duplicate" },
  ]);
  expect(vm.toolErrorOf("demo.lookup")).toBe(issueCopy("toolOff"));
  expect(vm.errorOf("key")).toBe(issueCopy("duplicate"));

  vm.removeFile("references/style.md");
  vm.setToolAllowed("demo.lookup", false);
  expect(vm.dirty).toBe(true);
  expect(vm.toolErrorOf("demo.lookup")).toBeUndefined();
  expect(vm.toUpdateInput()).toEqual({
    id: SKILL_ID,
    key: "release-notes",
    name: "Заметки о выпуске",
    description: "Составляет заметки о выпуске",
    instructions: "# Заметки",
    allowedTools: [],
    removeFiles: ["references/style.md"],
  });
});

test("tools above the ceiling are listed disabled with the reason, never hidden", async () => {
  const { store } = setup([
    tool(1, "demo.lookup"),
    tool(2, "demo.off", { tier: "off" }),
    tool(3, "demo.disabled", { enabled: false }),
    tool(4, "qdrant.find", { source: "mcp", sourceRef: SERVER_ID, available: false }),
    tool(5, "skill.read"),
  ]);
  await store.mount();
  const rows = store.toolGroups.flatMap((group) => group.rows);
  const byKey = Object.fromEntries(rows.map((row) => [row.key, row.ceiling]));
  expect(Object.keys(byKey).sort()).toEqual([
    "demo.disabled",
    "demo.lookup",
    "demo.off",
    "qdrant.find",
  ]);
  expect(byKey["demo.lookup"]?.block).toBeNull();
  expect(byKey["demo.off"]?.block).toBe("off");
  expect(byKey["demo.disabled"]?.block).toBe("disabled");
  expect(byKey["qdrant.find"]?.block).toBe("offline");
  expect(byKey["qdrant.find"]?.reason).toContain("qdrant");
});

test("an allowed tool missing from the registry is shown so it can be unchecked", async () => {
  const { store } = setup([]);
  await store.mount();
  const unknown = store.toolGroups.find((group) => group.key === "unknown");
  expect(unknown?.rows.map((row) => row.key)).toEqual(["demo.lookup"]);
});

test("a remount while the first load is in flight still finishes loading", async () => {
  const { store } = setup();
  const first = store.mount();
  store.dispose();
  const second = store.mount();
  await Promise.all([first, second]);
  expect(store.loading).toBe(false);
  expect(store.loaded).toBe(true);
  expect(store.skills).toHaveLength(1);
  expect(store.form?.key).toBe("release-notes");
});

test("a rejected save maps host issues onto the form", async () => {
  const { store, bridge } = setup();
  await store.mount();
  bridge.handle("skills.update", () => {
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Навык не прошёл проверку", {
      details: {
        field: "key",
        rule: "duplicate",
        issues: [{ field: "key", rule: "duplicate" }],
      },
    });
  });
  store.form?.setName("Другое имя");
  expect(await store.submit()).toBe(false);
  expect(store.form?.errorOf("key")).toBe(issueCopy("duplicate"));
  expect(store.form?.banner).toBe(SKILL_INVALID_BANNER);
});

test("helpers: model values round-trip, keys slugify, errors map by code", () => {
  const model = { providerId: SERVER_ID as never, modelId: "gpt-oss:120b" };
  expect(modelOf(modelValue(model))).toEqual(model);
  expect(modelOf("")).toBeNull();
  expect(slugify("  Проверка 2 провайдеров!  ")).toBe("proverka-2-provayderov");
  expect(skillErrorCopy(new Error("x"))).not.toBe("");
  expect(
    markdownLines("# Заголовок\n- **жирный** `код`").map((line) => line.map((t) => t.kind)),
  ).toEqual([["heading"], ["marker", "plain", "strong", "plain", "code"]]);
});

test("opening a file in the tree reads it from the host and removing it stages the removal", async () => {
  const { store, bridge } = setup();
  bridge.handle("skills.readFile", ({ path }) => ({ path, size: 14, text: "Пишите кратко." }));
  await store.mount();
  expect(store.openPath).toBe(MANIFEST_PATH);
  await store.open("references/style.md");
  expect(store.openFile?.text).toBe("Пишите кратко.");
  expect(bridge.calls.at(-1)).toEqual({
    channel: "skills.readFile",
    payload: { id: SKILL_ID, path: "references/style.md" },
  });
  store.removeOpenFile();
  expect(store.openPath).toBe(MANIFEST_PATH);
  expect(store.form?.removed).toEqual(["references/style.md"]);
  expect(store.form?.dirty).toBe(true);
});

function outdated(modified = false): SkillDetailDto {
  return detail({
    source: "bundled",
    version: "1.0.0",
    author: "ZVS AI Studio",
    update: { version: "1.1.0", modified },
  });
}

function current(): SkillDetailDto {
  return detail({
    source: "bundled",
    version: "1.1.0",
    author: "ZVS AI Studio",
    instructions: "# Заметки по разделам",
  });
}

test("an outdated bundled skill updates in place and the open form follows the new version", async () => {
  const { store, bridge } = setup();
  bridge
    .handle("skills.list", () => [outdated()])
    .handle("skills.get", () => outdated())
    .handle("skills.updateBundled", () => current());
  await store.mount();
  expect(store.updatable.map((skill) => skill.id)).toEqual([SKILL_ID]);
  expect(store.modifiedUpdates).toBe(0);
  await store.updateSkill(SKILL_ID);
  expect(bridge.calls.at(-1)).toEqual({
    channel: "skills.updateBundled",
    payload: { id: SKILL_ID },
  });
  expect(store.updatable).toEqual([]);
  expect(store.selectedSkill?.version).toBe("1.1.0");
  expect(store.form?.instructions).toBe("# Заметки по разделам");
  expect(store.notice).toBe(updatedNotice("Заметки о выпуске", "1.1.0"));
  expect(store.isUpdating(SKILL_ID)).toBe(false);
});

test("unsaved changes in a skill block its update", async () => {
  const { store, bridge } = setup();
  bridge.handle("skills.list", () => [outdated(true)]).handle("skills.get", () => outdated(true));
  await store.mount();
  store.form?.setName("Другое имя");
  await store.updateSkill(SKILL_ID);
  await store.updateAll();
  expect(store.error).toBe(UPDATE_GUARD);
  expect(bridge.calls.some((call) => call.channel.startsWith("skills.update"))).toBe(false);
});

test("updating everything replaces the list with the host's", async () => {
  const { store, bridge } = setup();
  bridge
    .handle("skills.list", () => [outdated(true)])
    .handle("skills.get", () => outdated(true))
    .handle("skills.updateAllBundled", () => [current()]);
  await store.mount();
  expect(store.modifiedUpdates).toBe(1);
  bridge.handle("skills.get", () => current());
  await store.updateAll();
  expect(store.updatable).toEqual([]);
  expect(store.skills[0]?.version).toBe("1.1.0");
  expect(store.form?.instructions).toBe("# Заметки по разделам");
  expect(store.notice).toBe(updatedAllNotice(1));
  expect(store.updatingAll).toBe(false);
});
