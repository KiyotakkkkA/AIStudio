import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import ChatComposer from "../../src/renderer/features/chat/ChatComposer";
import ChatMarkdown from "../../src/renderer/features/chat/ChatMarkdown";
import { provider } from "./providerFixtures.ts";

test("Markdown drops raw HTML and remote images and renders fenced code as escaped text", async () => {
  const content =
    "# Answer\n\n**Safe**\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n![remote](https://example.com/track)\n\n[bad](javascript:alert(1))\n\n```html\n<img src=x onerror=alert(1)>\n```";
  const { container } = render(<ChatMarkdown content={content} />);
  expect(screen.getByRole("heading", { name: "Answer" })).toBeTruthy();
  expect(container.querySelector("strong")?.textContent).toBe("Safe");
  await waitFor(() => expect(container.textContent).toContain("<img src=x onerror=alert(1)>"));
  expect(container.querySelector("script, img, iframe, a[href^='javascript:']")).toBeNull();
});

test("no provider routes to configuration and does not leave a dead composer", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [])
    .handle("providers.list", () => [])
    .handle("vectorStores.list", () => []);
  const events = createEventRouter();
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  await store.mount();
  render(
    <MemoryRouter>
      <ChatComposer store={store} />
    </MemoryRouter>,
  );
  expect(
    screen
      .getByRole("link", { name: /Open AI Providers|Открыть провайдеров ИИ/ })
      .getAttribute("href"),
  ).toBe("/providers");
  expect(screen.queryByRole("textbox", { name: /Message|Сообщение/ })).toBeNull();
  store.dispose();
  events.dispose();
});

test("composer keeps Shift+Enter and IME Enter local, while Enter sends", async () => {
  const saved = provider();
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [])
    .handle("providers.list", () => [saved])
    .handle("providers.get", () => saved)
    .handle("vectorStores.list", () => []);
  const events = createEventRouter();
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  await store.mount();
  render(
    <MemoryRouter>
      <ChatComposer store={store} />
    </MemoryRouter>,
  );
  const input = screen.getByRole("textbox", { name: /Message|Сообщение/ });
  fireEvent.change(input, { target: { value: "hello" } });
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  expect(bridge.calls.filter((call) => call.channel === "chat.conversations.create")).toHaveLength(
    0,
  );
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() =>
    expect(bridge.calls.some((call) => call.channel === "chat.conversations.create")).toBe(true),
  );
  store.dispose();
  events.dispose();
});
