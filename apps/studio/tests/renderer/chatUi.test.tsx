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
import { isFenceOpen } from "../../src/renderer/features/chat/codeFence.ts";
import { ADAPTERS, provider } from "./providerFixtures.ts";
import { account } from "./accountFixtures.ts";

test("Markdown drops raw HTML and remote images and renders fenced code as escaped text", async () => {
  const content =
    "# Answer\n\n**Safe**\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n![remote](https://example.com/track)\n\n[bad](javascript:alert(1))\n\n```html\n<img src=x onerror=alert(1)>\n```";
  const { container } = render(<ChatMarkdown content={content} />);
  expect(screen.getByRole("heading", { name: "Answer" })).toBeTruthy();
  expect(container.querySelector("strong")?.textContent).toBe("Safe");
  await waitFor(() => expect(container.textContent).toContain("<img src=x onerror=alert(1)>"));
  expect(container.querySelector("script, img, iframe, a[href^='javascript:']")).toBeNull();
});

test("a code fence is open until its closing line arrives and nothing follows it", () => {
  const open = "Text\n\n```python\nprint(1)\n";
  expect(isFenceOpen(open, 6, open.length)).toBe(true);
  const closed = "Text\n\n```python\nprint(1)\n```\n";
  expect(isFenceOpen(closed, 6, closed.length - 1)).toBe(false);
  const followed = "```\na\n\nMore text";
  expect(isFenceOpen(followed, 0, 6)).toBe(false);
  expect(isFenceOpen(open, undefined, undefined)).toBe(false);
});

test("a streamed code block keeps its element and is highlighted only once it is closed", async () => {
  const start = "Код:\n\n```python\nprint(1)\n";
  const { container, rerender } = render(<ChatMarkdown content={start} streaming />);
  const block = container.querySelector("pre");
  expect(block?.textContent).toBe("print(1)");
  expect(container.querySelector(".animate-pulse")).toBeNull();
  rerender(<ChatMarkdown content={`${start}print(2)\n`} streaming />);
  expect(container.querySelector("pre")).toBe(block);
  expect(block?.textContent).toBe("print(1)\nprint(2)");
  rerender(<ChatMarkdown content={`${start}print(2)\n\`\`\``} streaming />);
  expect(container.querySelector(".animate-pulse")).toBeNull();
  await waitFor(() => expect(container.textContent).toContain("print(2)"));
});

test("Markdown renders GFM tables", () => {
  const { container } = render(
    <ChatMarkdown content={"| Name | Value |\n| --- | ---: |\n| Model | 20b |"} />,
  );
  expect(container.querySelector("table")).toBeTruthy();
  expect(screen.getByRole("columnheader", { name: "Name" })).toBeTruthy();
  expect(screen.getByRole("cell", { name: "20b" })).toBeTruthy();
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
    screen.getByRole("link", { name: /AI Providers|Провайдеры ИИ/ }).getAttribute("href"),
  ).toBe("/providers");
  expect(screen.queryByRole("textbox", { name: /Message|Сообщение/ })).toBeNull();
  store.dispose();
  events.dispose();
});

test("with no provider yet, a linked Qwen account can be connected straight from the chat", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("chat.conversations.list", () => [])
    .handle("providers.list", () => [])
    .handle("vectorStores.list", () => [])
    .handle("accounts.list", () => [account()])
    .handle("providers.adapters", () => ADAPTERS);
  const events = createEventRouter();
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  await store.mount();
  render(
    <MemoryRouter>
      <ChatComposer store={store} />
    </MemoryRouter>,
  );
  expect(screen.getByRole("button", { name: "Подключить Qwen" })).toBeTruthy();
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
  expect(screen.getAllByText(saved.models[0].displayName).length).toBeGreaterThan(0);
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
