import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract, type ExecPolicyDto, type ProviderDto } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ExecPolicyStore from "../../src/renderer/features/execPolicy/ExecPolicyStore.ts";
import { MODEL, OLLAMA, OTHER_MODEL, model, provider, summary } from "./providerFixtures.ts";

function setup(
  providers: ProviderDto[] = [provider()],
  initial: ExecPolicyDto = { mode: "always-ask", judge: null },
) {
  let stored = initial;
  const saved: ExecPolicyDto[] = [];
  let refuse = false;
  const bridge = createFakeBridge<Contract>()
    .handle("providers.list", () => providers.map((item) => summary({ id: item.id })))
    .handle("providers.get", ({ id }) => providers.find((item) => item.id === id) ?? provider())
    .handle("execPolicy.get", () => stored)
    .handle("execPolicy.set", (policy) => {
      if (refuse) throw new Error("refused");
      saved.push(policy);
      stored = policy;
      return policy;
    });
  const store = new ExecPolicyStore(createIpcClient(contract, bridge));
  return {
    store,
    saved,
    refuse: () => {
      refuse = true;
    },
    restore: (policy: ExecPolicyDto) => {
      stored = policy;
    },
  };
}

const JUDGE = { providerId: OLLAMA, modelId: "gpt-oss:120b" };

describe("ExecPolicyStore", () => {
  test("shows the stored mode and judge after loading", async () => {
    const { store } = setup([provider()], { mode: "auto-bypass", judge: JUDGE });
    expect(store.mode).toBe("always-ask");
    await store.load();
    expect(store.mode).toBe("auto-bypass");
    expect(store.judgeProviderId).toBe(OLLAMA);
    expect(store.judgeModelId).toBe("gpt-oss:120b");
    expect(store.judgeReady).toBe(true);
  });

  test("keeps auto-bypass unavailable without a judge and saves other modes", async () => {
    const { store, saved } = setup();
    await store.load();
    expect(store.isAvailable("auto-bypass")).toBe(false);
    await store.setMode("auto-bypass");
    expect(store.mode).toBe("always-ask");
    await store.setMode("safety-ask");
    expect(store.mode).toBe("safety-ask");
    expect(saved).toEqual([{ mode: "safety-ask", judge: null }]);
  });

  test("choosing a judge provider saves its first available model and unlocks auto-bypass", async () => {
    const { store, saved } = setup([
      provider({
        models: [
          model({ id: MODEL, externalId: "gone", available: false }),
          model({ id: OTHER_MODEL, externalId: "qwen3-coder:480b" }),
        ],
      }),
    ]);
    await store.load();
    await store.selectJudgeProvider(OLLAMA);
    expect(store.judgeModelId).toBe("qwen3-coder:480b");
    await store.setMode("auto-bypass");
    expect(store.mode).toBe("auto-bypass");
    expect(saved.at(-1)).toEqual({
      mode: "auto-bypass",
      judge: { providerId: OLLAMA, modelId: "qwen3-coder:480b" },
    });
  });

  test("ignores a judge model the provider does not offer", async () => {
    const { store, saved } = setup();
    await store.load();
    await store.selectJudgeProvider(OLLAMA);
    await store.setJudgeModel("missing");
    expect(store.judgeModelId).toBe("gpt-oss:120b");
    expect(saved).toHaveLength(1);
  });

  test("clearing the judge falls back from auto-bypass to safety-ask on the host too", async () => {
    const { store, saved } = setup([provider()], { mode: "auto-bypass", judge: JUDGE });
    await store.load();
    await store.clearJudge();
    expect(store.judgeReady).toBe(false);
    expect(store.mode).toBe("safety-ask");
    expect(saved.at(-1)).toEqual({ mode: "safety-ask", judge: null });
  });

  test("keeps no-restrictions when the judge is cleared", async () => {
    const { store, saved } = setup([provider()], { mode: "no-restrictions", judge: JUDGE });
    await store.load();
    await store.clearJudge();
    expect(store.mode).toBe("no-restrictions");
    expect(saved.at(-1)).toEqual({ mode: "no-restrictions", judge: null });
  });

  test("shows what the host kept when a judge it lost is dropped", async () => {
    const { store, restore } = setup([provider()], { mode: "auto-bypass", judge: JUDGE });
    await store.load();
    restore({ mode: "safety-ask", judge: null });
    await store.load();
    expect(store.judgeProviderId).toBeNull();
    expect(store.mode).toBe("safety-ask");
  });

  test("a refused save reverts to the stored policy", async () => {
    const { store, refuse } = setup([provider()], { mode: "safety-ask", judge: null });
    await store.load();
    refuse();
    await store.setMode("no-restrictions");
    expect(store.mode).toBe("safety-ask");
  });
});
