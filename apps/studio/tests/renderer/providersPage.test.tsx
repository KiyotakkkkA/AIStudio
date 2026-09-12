import assert from "node:assert/strict";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract, type ProviderCapability } from "@zvs/shared";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import StoreProvider from "../../src/renderer/app/StoreProvider.tsx";
import ProvidersPage from "../../src/renderer/pages/ProvidersPage.tsx";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import {
  ACCOUNTS,
  ADAPTERS,
  EMBEDDER,
  OLLAMA,
  provider,
  SECRETS,
  summary,
} from "./providerFixtures.ts";

function renderPage(selectedProvider = provider()) {
  const bridge = createFakeBridge<Contract>();
  bridge.handle("settings.get", ({ key }) => ({ key }));
  bridge.handle("providers.adapters", () => ADAPTERS.map((entry) => ({ ...entry })));
  bridge.handle("secrets.list", () => SECRETS.map((entry) => ({ ...entry })));
  bridge.handle("accounts.list", () => [...ACCOUNTS]);
  bridge.handle("providers.list", ({ capability }: { capability?: ProviderCapability }) => {
    if (capability === "text") return [summary({ id: OLLAMA })];
    if (capability === "embedding") {
      return [summary({ id: EMBEDDER, name: "Ollama Embeddings", capabilities: ["embedding"] })];
    }
    return [];
  });
  bridge.handle("providers.get", () => selectedProvider);

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

test("model checkboxes allow multiple selections and filtering preserves checked models", async () => {
  renderPage();
  const first = await screen.findByRole("checkbox", { name: "Использовать модель: gpt-oss:120b" });
  const second = screen.getByRole("checkbox", { name: "Использовать модель: qwen3-coder:480b" });
  fireEvent.click(first);
  fireEvent.click(second);
  assert.equal((first as HTMLInputElement).checked, true);
  assert.equal((second as HTMLInputElement).checked, true);
  fireEvent.click(screen.getByRole("checkbox", { name: "Бесплатные модели" }));
  assert.equal(
    screen.queryByRole("checkbox", { name: "Использовать модель: qwen3-coder:480b" }),
    null,
  );
  fireEvent.click(screen.getByRole("checkbox", { name: "Бесплатные модели" }));
  assert.equal(
    (
      screen.getByRole("checkbox", {
        name: "Использовать модель: qwen3-coder:480b",
      }) as HTMLInputElement
    ).checked,
    true,
  );
  assert.equal(screen.queryByRole("checkbox", { name: "Не используют данные для обучения" }), null);
});

test("the page mounts one form and one list, with the token as a picker over secrets", async () => {
  renderPage();

  assert.ok(await screen.findByText("Настройка подключения"));

  const tab = screen
    .getAllByRole("button", { name: /Генерация текста/u })
    .find((button) => button.getAttribute("aria-current") === "page");
  assert.ok(tab !== undefined, "the active capability tab is marked");
  assert.equal(within(tab).getByText("1").textContent, "1");

  // The credential never appears as a typed field: no password input anywhere on the page.
  assert.equal(document.querySelectorAll('input[type="password"]').length, 0);
  const tokenField = screen.getByText("API-токен").parentElement;
  assert.ok(tokenField !== null);
  assert.equal(tokenField.querySelectorAll("input").length, 0, "the token is picked, never typed");
  assert.equal(screen.queryByRole("radiogroup", { name: "Способ авторизации" }), null);

  const models = screen.getAllByText("gpt-oss:120b");
  assert.ok(models.length > 0);
});

test("web adapter hides the vendor field", async () => {
  renderPage(
    provider({
      adapter: "deepseek-web",
      authMode: "account",
      secretId: null,
      accountId: null,
    }),
  );

  assert.ok(await screen.findByText("Семейство адаптера"));
  assert.equal(screen.queryByText("Вендор"), null);
  assert.equal(screen.queryByText("Способ авторизации"), null);
});
