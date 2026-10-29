import { describe, expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract, type ProviderDto } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ExecPolicyStore from "../../src/renderer/features/execPolicy/ExecPolicyStore.ts";
import { MODEL, OLLAMA, OTHER_MODEL, model, provider, summary } from "./providerFixtures.ts";

function setup(providers: ProviderDto[] = [provider()]) {
  let listed = providers;
  const bridge = createFakeBridge<Contract>()
    .handle("providers.list", () => listed.map((item) => summary({ id: item.id })))
    .handle("providers.get", ({ id }) => listed.find((item) => item.id === id) ?? provider());
  const store = new ExecPolicyStore(createIpcClient(contract, bridge));
  return {
    store,
    relist: (next: ProviderDto[]) => {
      listed = next;
    },
  };
}

describe("ExecPolicyStore", () => {
  test("starts in always-ask and keeps auto-bypass unavailable without a judge", () => {
    const { store } = setup();
    expect(store.mode).toBe("always-ask");
    expect(store.isAvailable("auto-bypass")).toBe(false);
    store.setMode("auto-bypass");
    expect(store.mode).toBe("always-ask");
    store.setMode("safety-ask");
    expect(store.mode).toBe("safety-ask");
  });

  test("choosing a judge provider picks its first available model and unlocks auto-bypass", async () => {
    const { store } = setup([
      provider({
        models: [
          model({ id: MODEL, externalId: "gone", available: false }),
          model({ id: OTHER_MODEL, externalId: "qwen3-coder:480b" }),
        ],
      }),
    ]);
    await store.load();
    store.selectJudgeProvider(OLLAMA);
    expect(store.judgeModelId).toBe("qwen3-coder:480b");
    store.setMode("auto-bypass");
    expect(store.mode).toBe("auto-bypass");
  });

  test("ignores a judge model the provider does not offer", async () => {
    const { store } = setup();
    await store.load();
    store.selectJudgeProvider(OLLAMA);
    store.setJudgeModel("missing");
    expect(store.judgeModelId).toBe("gpt-oss:120b");
  });

  test("clearing the judge falls back from auto-bypass to safety-ask", async () => {
    const { store } = setup();
    await store.load();
    store.selectJudgeProvider(OLLAMA);
    store.setMode("auto-bypass");
    store.clearJudge();
    expect(store.judgeReady).toBe(false);
    expect(store.mode).toBe("safety-ask");
  });

  test("drops the judge when its provider is no longer listed", async () => {
    const { store, relist } = setup();
    await store.load();
    store.selectJudgeProvider(OLLAMA);
    store.setMode("auto-bypass");
    relist([]);
    await store.load();
    expect(store.judgeProviderId).toBeNull();
    expect(store.mode).toBe("safety-ask");
  });

  test("keeps no-restrictions when the judge is cleared", async () => {
    const { store } = setup();
    await store.load();
    store.selectJudgeProvider(OLLAMA);
    store.setMode("no-restrictions");
    store.clearJudge();
    expect(store.mode).toBe("no-restrictions");
  });
});
