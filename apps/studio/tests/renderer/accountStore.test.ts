import assert from "node:assert/strict";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppError,
  AppErrorCode,
  contract,
  type AccountLinkResult,
  type Contract,
} from "@zvs/shared";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import { AccountStore } from "../../src/renderer/features/providers/AccountStore.ts";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { ADAPTERS } from "./providerFixtures.ts";
import { account, deferred, LINK_STREAM, QWEN_ACCOUNT } from "./accountFixtures.ts";

function storeWith(accounts: readonly ReturnType<typeof account>[] = []) {
  const bridge = createFakeBridge<Contract>();
  const events = createEventRouter();
  let listed = [...accounts];
  bridge.handle("providers.adapters", () => ADAPTERS.map((entry) => ({ ...entry })));
  bridge.handle("accounts.list", () => [...listed]);
  return {
    bridge,
    events,
    setListed(next: readonly ReturnType<typeof account>[]) {
      listed = [...next];
    },
    store: new AccountStore({ ipc: createIpcClient(contract, bridge), events }),
  };
}

function channels(bridge: ReturnType<typeof createFakeBridge<Contract>>): string[] {
  return bridge.calls.map((call) => call.channel);
}

test("the vendor list comes from the host's account families, not from the renderer", async () => {
  const { store } = storeWith();

  await store.load();

  assert.deepEqual(store.families, ["deepseek-web", "qwen-web"]);
  assert.equal(store.isEmpty, true);
});

test("linking waits, then renders the account the vendor returned", async () => {
  const { bridge, store, setListed } = storeWith();
  const linked = account();
  const pending = deferred<AccountLinkResult>();
  bridge.handle("accounts.link", () => pending.promise);
  await store.load();

  const running = store.link("qwen-web");
  assert.deepEqual(store.linking, {
    adapter: "qwen-web",
    phase: "waiting",
    detail: null,
    elapsedMs: 0,
    totalMs: 0,
  });
  assert.equal(store.busy, true);

  setListed([linked]);
  pending.resolve({
    status: "linked",
    account: linked,
    replacedPreviousIdentity: false,
    detail: null,
  });
  await running;

  assert.equal(store.linking?.phase, "success");
  assert.equal(store.busy, false);
  assert.deepEqual(
    store.accounts.map((entry) => entry.id),
    [QWEN_ACCOUNT],
  );
});

test("a link that finds no sign-in ends in timeout and can be retried", async () => {
  const { bridge, store } = storeWith();
  const outcomes: AccountLinkResult[] = [
    { status: "timeout", detail: "No sign-in detected" },
    { status: "linked", account: account(), replacedPreviousIdentity: false, detail: null },
  ];
  bridge.handle("accounts.link", () => outcomes.shift() ?? assert.fail("too many link calls"));
  await store.load();

  await store.link("qwen-web");
  assert.equal(store.linking?.phase, "timeout");
  assert.equal(store.accounts.length, 0);

  await store.link("qwen-web");
  assert.equal(store.linking?.phase, "success");
});

test("cancel goes through the channel and leaves the list untouched", async () => {
  const { bridge, store } = storeWith();
  const pending = deferred<AccountLinkResult>();
  bridge.handle("accounts.link", () => pending.promise);
  bridge.handle("accounts.cancelLink", () => ({ cancelled: true }));
  await store.load();

  const running = store.link("qwen-web");
  await store.cancelLink();

  assert.deepEqual(
    bridge.calls.filter((call) => call.channel === "accounts.cancelLink"),
    [{ channel: "accounts.cancelLink", payload: { adapter: "qwen-web" } }],
  );

  pending.resolve({ status: "cancelled", detail: "No sign-in detected" });
  await running;

  assert.equal(store.linking?.phase, "cancelled");
  assert.equal(store.accounts.length, 0);
});

test("cancelling when nothing is waiting never reaches the vendor", async () => {
  const { bridge, store } = storeWith();
  await store.load();

  await store.cancelLink();

  assert.equal(channels(bridge).includes("accounts.cancelLink"), false);
});

test("the waiting panel follows the host's progress events instead of polling", async () => {
  const { bridge, store, events } = storeWith();
  const pending = deferred<AccountLinkResult>();
  bridge.handle("accounts.link", () => pending.promise);
  await store.load();

  const running = store.link("qwen-web");
  events.dispatch({
    streamId: LINK_STREAM,
    seq: 0,
    ts: 1_700_000_000_000,
    type: "step",
    step: { domain: "accounts", adapter: "qwen-web", phase: "waiting-for-sign-in" },
  });
  events.dispatch({
    streamId: LINK_STREAM,
    seq: 1,
    ts: 1_700_000_000_000,
    type: "progress",
    done: 4000,
    total: 180_000,
  });

  assert.equal(store.linking?.elapsedMs, 4000);
  assert.equal(store.linking?.totalMs, 180_000);
  assert.equal(channels(bridge).filter((name) => name === "accounts.list").length, 1);

  pending.resolve({ status: "cancelled", detail: "No sign-in detected" });
  await running;
});

test("progress from another domain's stream never moves the waiting panel", async () => {
  const { bridge, store, events } = storeWith();
  const pending = deferred<AccountLinkResult>();
  bridge.handle("accounts.link", () => pending.promise);
  await store.load();

  const running = store.link("qwen-web");
  events.dispatch({
    streamId: LINK_STREAM,
    seq: 0,
    ts: 1_700_000_000_000,
    type: "progress",
    done: 9000,
    total: 9000,
  });

  assert.equal(store.linking?.elapsedMs, 0);

  pending.resolve({ status: "cancelled", detail: "No sign-in detected" });
  await running;
});

test("unlink detaches the account and reports nothing about the browser session", async () => {
  const { bridge, store } = storeWith([account()]);
  bridge.handle("accounts.unlink", ({ id }) => ({
    id,
    removed: true as const,
    browserSessionPreserved: true as const,
  }));
  await store.load();

  await store.unlink(QWEN_ACCOUNT);

  assert.equal(store.accounts.length, 0);
  assert.deepEqual(
    bridge.calls.filter((call) => call.channel === "accounts.unlink"),
    [{ channel: "accounts.unlink", payload: { id: QWEN_ACCOUNT } }],
  );
});

test("re-checking a session replaces the card in place", async () => {
  const stale = account({ status: "needs-relink", detail: "Сессия истекла — войдите заново" });
  const { bridge, store } = storeWith([stale]);
  bridge.handle("accounts.refresh", () => account({ status: "linked", detail: null }));
  await store.load();

  await store.refresh(QWEN_ACCOUNT);

  assert.equal(store.accounts.length, 1);
  assert.equal(store.accounts[0]?.status, "linked");
  assert.equal(store.refreshingId, null);
});

test("a channel failure surfaces as copy, never as a stack", async () => {
  const { bridge, store } = storeWith([account()]);
  bridge.fail("accounts.refresh", AppErrorCode.PROVIDER_UNREACHABLE, "boom");
  await store.load();

  await store.refresh(QWEN_ACCOUNT);

  assert.equal(store.error, "Провайдер недоступен.");
  assert.equal(store.error?.includes("at "), false);
  store.dismissError();
  assert.equal(store.error, null);
});

test("a rejected link clears the waiting panel and keeps the error readable", async () => {
  const { bridge, store } = storeWith();
  bridge.handle("accounts.link", () => {
    throw new AppError(AppErrorCode.CONFLICT, "busy");
  });
  await store.load();

  await store.link("qwen-web");

  assert.equal(store.linking, null);
  assert.equal(store.error, "Провайдер занят другой операцией — дождитесь её окончания.");
});

test("nothing the store keeps about an account carries a token, cookie or partition", async () => {
  const { store } = storeWith([account()]);
  await store.load();

  const keys = Object.keys(store.accounts[0] ?? {});
  for (const forbidden of ["token", "cookie", "partition", "secret", "value"]) {
    assert.equal(
      keys.some((key) => key.toLowerCase().includes(forbidden)),
      false,
      `the account DTO must not expose ${forbidden}`,
    );
  }
});
