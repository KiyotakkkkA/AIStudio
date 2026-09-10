import assert from "node:assert/strict";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type AccountDto, type AccountLinkResult, type Contract } from "@zvs/shared";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import StoreProvider from "../../src/renderer/app/StoreProvider.tsx";
import ProvidersPage from "../../src/renderer/pages/ProvidersPage.tsx";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { ADAPTERS, OLLAMA, provider, SECRETS, summary } from "./providerFixtures.ts";
import { account, deferred, QWEN_ACCOUNT } from "./accountFixtures.ts";

beforeEach(() => {
  // jsdom has no layout; Modal filters focus targets by their rendered dimensions.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(100);
});

afterEach(() => vi.restoreAllMocks());

function renderTab(accounts: readonly AccountDto[] = []) {
  const bridge = createFakeBridge<Contract>();
  bridge.handle("settings.get", ({ key }) => ({ key }));
  bridge.handle("providers.adapters", () => ADAPTERS.map((entry) => ({ ...entry })));
  bridge.handle("secrets.list", () => SECRETS.map((entry) => ({ ...entry })));
  bridge.handle("accounts.list", () => [...accounts]);
  bridge.handle("providers.list", ({ capability }) =>
    capability === "text" ? [summary({ id: OLLAMA })] : [],
  );
  bridge.handle("providers.get", () => provider());

  render(
    <MemoryRouter>
      <StoreProvider
        environment={{ ipc: createIpcClient(contract, bridge), events: createEventRouter() }}
      >
        <ProvidersPage />
      </StoreProvider>
    </MemoryRouter>,
  );
  return bridge;
}

async function openAccountsTab(bridge: ReturnType<typeof createFakeBridge<Contract>>) {
  fireEvent.click(await screen.findByRole("button", { name: /Аккаунты/u }));
  return bridge;
}

function loginMenu(): HTMLElement {
  const trigger = screen.getAllByRole("button", { name: /Войти/u })[0];
  assert.ok(trigger !== undefined);
  fireEvent.click(trigger);
  return screen.getByRole("menu", { name: "Выберите вендора" });
}

test("the tab sits beside the capability filters and counts linked accounts", async () => {
  const bridge = renderTab([account()]);
  await openAccountsTab(bridge);

  const tab = screen
    .getAllByRole("button", { name: /Аккаунты/u })
    .find((button) => button.getAttribute("aria-current") === "page");
  assert.ok(tab !== undefined, "the accounts tab is the active one");
  assert.equal(within(tab).getByText("1").textContent, "1");
  assert.ok(screen.getByText("k***@example.com"));
});

test("the empty state explains account mode and offers only the vendors the host reports", async () => {
  const bridge = renderTab();
  await openAccountsTab(bridge);

  assert.ok(await screen.findByText("Аккаунтов пока нет"));
  const menu = loginMenu();
  assert.deepEqual(
    within(menu)
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.split("О")[0]?.trim()),
    ["DeepSeek", "Qwen"],
  );

  // The vendor's own page takes the password; our UI never offers a field for one.
  assert.equal(document.querySelectorAll('input[type="password"]').length, 0);
  assert.equal(document.querySelectorAll("input").length, 0);
});

test("waiting shows the vendor and cancels through the channel", async () => {
  const bridge = renderTab();
  const pending = deferred<AccountLinkResult>();
  bridge.handle("accounts.link", () => pending.promise);
  bridge.handle("accounts.cancelLink", () => ({ cancelled: true }));
  await openAccountsTab(bridge);
  await screen.findByText("Аккаунтов пока нет");

  fireEvent.click(within(loginMenu()).getByRole("menuitem", { name: /Qwen/u }));

  assert.ok(await screen.findByText(/Войдите в Qwen в браузере/u));
  fireEvent.click(screen.getByRole("button", { name: "Отменить" }));

  assert.deepEqual(
    bridge.calls.filter((call) => call.channel === "accounts.cancelLink"),
    [{ channel: "accounts.cancelLink", payload: { adapter: "qwen-web" } }],
  );

  pending.resolve({ status: "cancelled", detail: "No sign-in detected" });
  await screen.findByText("Аккаунтов пока нет");
  assert.equal(screen.queryByText(/Войдите в Qwen в браузере/u), null);
});

test("a timeout says no sign-in was detected and offers another attempt", async () => {
  const bridge = renderTab();
  bridge.handle("accounts.link", () => ({
    status: "timeout" as const,
    detail: "No sign-in detected",
  }));
  await openAccountsTab(bridge);
  await screen.findByText("Аккаунтов пока нет");

  fireEvent.click(within(loginMenu()).getByRole("menuitem", { name: /Qwen/u }));

  assert.ok(await screen.findByText(/Вход не обнаружен/u));
  assert.ok(screen.getByRole("button", { name: "Попробовать снова" }));
});

test("a needs-relink account promotes Re-link and keeps the session detail visible", async () => {
  const bridge = renderTab([
    account({ status: "needs-relink", detail: "Сессия истекла — войдите заново" }),
  ]);
  await openAccountsTab(bridge);

  assert.equal((await screen.findAllByText("Сессия истекла — войдите заново")).length, 2);
  assert.ok(screen.getByRole("button", { name: "Перепривязать" }));
  assert.equal(screen.queryByRole("button", { name: "Проверить сессию" }), null);
});

test("an expiring session reuses the amber expiry line", async () => {
  const bridge = renderTab([account({ expiresAt: Date.now() + 3 * 86_400_000 })]);
  await openAccountsTab(bridge);

  const line = await screen.findByText("Сессия истекает через 3 дня");
  assert.ok(line.className.includes("text-warn"));
});

test("unlink names the consequences and only then calls the channel", async () => {
  const bridge = renderTab([account({ linkedProvidersCount: 2 })]);
  bridge.handle("accounts.unlink", ({ id }) => ({
    id,
    removed: true as const,
    browserSessionPreserved: true as const,
  }));
  await openAccountsTab(bridge);

  fireEvent.click(await screen.findByRole("button", { name: /Отвязать/u }));

  const dialog = screen.getByRole("dialog", { name: "Отвязать аккаунт?" });
  assert.ok(within(dialog).getByText(/2 подключения перестанут работать/u));
  assert.ok(within(dialog).getByText(/Настройки моделей и модель по умолчанию сохранятся/u));
  assert.ok(within(dialog).getByText(/Сессия Qwen в браузере останется активной/u));
  assert.equal(
    bridge.calls.some((call) => call.channel === "accounts.unlink"),
    false,
    "nothing is unlinked before the confirmation",
  );

  fireEvent.click(within(dialog).getByRole("button", { name: "Отвязать" }));

  assert.deepEqual(
    bridge.calls.filter((call) => call.channel === "accounts.unlink"),
    [{ channel: "accounts.unlink", payload: { id: QWEN_ACCOUNT } }],
  );
});

test("signing in again for a linked vendor explains the replacement first", async () => {
  const bridge = renderTab([account()]);
  bridge.handle("accounts.link", () => ({ status: "timeout" as const, detail: "" }));
  await openAccountsTab(bridge);
  await screen.findByText("k***@example.com");

  fireEvent.click(within(loginMenu()).getByRole("menuitem", { name: /Qwen/u }));

  const dialog = screen.getByRole("dialog", { name: "Заменить привязанный аккаунт?" });
  assert.ok(within(dialog).getByText(/только одна сессия на вендора/u));
  assert.equal(
    bridge.calls.some((call) => call.channel === "accounts.link"),
    false,
    "the replacement is explained before it happens",
  );

  fireEvent.click(within(dialog).getByRole("button", { name: "Войти заново" }));

  assert.deepEqual(
    bridge.calls.filter((call) => call.channel === "accounts.link"),
    [{ channel: "accounts.link", payload: { adapter: "qwen-web" } }],
  );
});

test("no part of the tab renders a token or cookie field", async () => {
  const bridge = renderTab([account({ linkedProvidersCount: 1 })]);
  await openAccountsTab(bridge);
  await screen.findByText("k***@example.com");

  const markup = document.body.innerHTML.toLowerCase();
  for (const forbidden of ["cookie", "куки"]) {
    // "Сайты и куки" is a pointer to the browser, not a value we show.
    assert.equal(markup.split(forbidden).length - 1 <= 1, true);
  }
  assert.equal(markup.includes("token"), false);
  assert.equal(markup.includes("bearer"), false);
});
