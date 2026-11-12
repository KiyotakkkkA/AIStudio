import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  contract,
  type Contract,
  type ProcessPlanDto,
  type UserRuleDto,
  type UserRulesDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ExecPolicyStore from "../../src/renderer/features/execPolicy/ExecPolicyStore.ts";
import { alwaysAllowable } from "../../src/renderer/features/chat/approvalModel.ts";

const RULE: UserRuleDto = {
  id: "user.0199aa11",
  text: "cargo build",
  effect: "allow",
  pack: "user.json",
  createdAt: 1_700_000_000_000,
  active: true,
};

const HAND: UserRuleDto = {
  id: "mine.a",
  text: "deploytool",
  effect: "allow",
  pack: "mine.json",
  createdAt: null,
  active: false,
};

function setup(initial: UserRulesDto) {
  let stored = initial;
  const bridge = createFakeBridge<Contract>()
    .handle("execPolicy.rules", () => stored)
    .handle("execPolicy.removeRule", ({ id }) => {
      stored = { ...stored, rules: stored.rules.filter((rule) => rule.id !== id) };
      return stored;
    })
    .handle("execPolicy.confirmPack", ({ pack }) => {
      stored = {
        rules: stored.rules.map((rule) => (rule.pack === pack ? { ...rule, active: true } : rule)),
        unconfirmed: stored.unconfirmed.filter((name) => name !== pack),
      };
      return stored;
    });
  return { store: new ExecPolicyStore(createIpcClient(contract, bridge)), bridge };
}

function plan(change: Partial<ProcessPlanDto> = {}): ProcessPlanDto {
  return { program: "cargo", argv: ["cargo", "build"], cwd: "D:\\work", packages: [], ...change };
}

describe("user rules", () => {
  test("lists, removes and confirms through the host", async () => {
    const { store } = setup({ rules: [RULE, HAND], unconfirmed: ["mine.json"] });
    expect(store.rulesState).toBe("idle");
    await store.loadRules();
    expect(store.rulesState).toBe("ready");
    expect(store.inactiveRules("mine.json")).toBe(1);
    await store.removeRule(RULE.id);
    expect(store.rules).toEqual([HAND]);
    await store.confirmPack("mine.json");
    expect(store.unconfirmed).toEqual([]);
    expect(store.inactiveRules("mine.json")).toBe(0);
    expect(store.rulesBusy).toBeNull();
  });

  test("a failed removal keeps the list and records the error", async () => {
    const { store, bridge } = setup({ rules: [RULE], unconfirmed: [] });
    await store.loadRules();
    bridge.handle("execPolicy.removeRule", () => {
      throw new Error("gone");
    });
    await store.removeRule(RULE.id);
    expect(store.rules).toEqual([RULE]);
    expect(store.rulesError).not.toBeNull();
  });

  test("always-allow is offered under the same conditions as the console", () => {
    expect(alwaysAllowable({ process: plan() }, "always-ask")).toBe(false);
    expect(alwaysAllowable({ process: plan() }, "safety-ask")).toBe(true);
    expect(alwaysAllowable({ process: plan() }, "auto-bypass")).toBe(true);
    expect(
      alwaysAllowable(
        { process: plan({ argv: ["powershell", "-Command", "x"], scriptString: true }) },
        "auto-bypass",
      ),
    ).toBe(false);
    expect(
      alwaysAllowable({ process: plan({ language: "python", source: "print(1)" }) }, "auto-bypass"),
    ).toBe(false);
    const changes = [
      {
        path: "D:\\work\\a.txt",
        kind: "delete" as const,
        added: 0,
        removed: 1,
        diff: "",
        truncated: false,
      },
    ];
    expect(alwaysAllowable({ changes }, "safety-ask")).toBe(false);
    expect(alwaysAllowable({ changes }, "auto-bypass")).toBe(true);
    expect(alwaysAllowable({ changes: [] }, "auto-bypass")).toBe(false);
  });
});
