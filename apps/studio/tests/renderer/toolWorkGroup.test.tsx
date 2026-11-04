import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ChatToolCallDto } from "@zvs/shared";
import { StoreContext } from "../../src/renderer/stores/StoreContext";
import type { RootStore } from "../../src/renderer/stores/RootStore";
import ChatMessage from "../../src/renderer/features/chat/ChatMessage";
import { ToolCallItems } from "../../src/renderer/features/chat/ChatToolCalls";
import { TOOL_CALLS_LABEL } from "../../src/renderer/features/chat/toolCallCopy";
import {
  WORKED_LEAD,
  workedDuration,
  workGroupLabel,
} from "../../src/renderer/features/chat/ToolWorkGroup";

const rootStore = {
  chat: { attachSelection: vi.fn(), openSite: vi.fn() },
  viewer: { openCitation: vi.fn(), openSelection: vi.fn() },
} as unknown as RootStore;

const call = (extra: Partial<ChatToolCallDto> = {}) =>
  ChatToolCallDto.parse({
    id: "call_1",
    key: "fs.read",
    name: "fs_read",
    round: 0,
    arguments: { paths: ["a.md"] },
    status: "succeeded",
    durationMs: 200_000,
    ...extra,
  });

function renderMessage(props: { readonly generating?: boolean }) {
  return render(
    <StoreContext.Provider value={rootStore}>
      <ChatMessage
        content="Готово."
        citations={[]}
        stores={[]}
        toolCalls={[call()]}
        renderToolItems={(calls) => <ToolCallItems calls={calls} />}
        generating={props.generating ?? false}
      />
    </StoreContext.Provider>,
  );
}

test("worked duration formats milliseconds, seconds and minutes", () => {
  expect(workedDuration(500)).toBe("500 мс");
  expect(workedDuration(45_000)).toBe("45 с");
  expect(workedDuration(60_000)).toBe("1 мин");
  expect(workedDuration(200_000)).toBe("3 мин 20 с");
  expect(workGroupLabel(200_000, 3)).toBe(`${WORKED_LEAD} 3 мин 20 с`);
  expect(workGroupLabel(0, 1)).toBe("Вызовы инструментов · 1 вызов");
  expect(workGroupLabel(0, 3)).toBe("Вызовы инструментов · 3 вызова");
});

test("a finished answer folds its tool calls into a collapsed work group", () => {
  renderMessage({});
  const toggle = screen.getByRole("button", { name: `${WORKED_LEAD} 3 мин 20 с` });
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("list", { name: TOOL_CALLS_LABEL })).toBeNull();

  fireEvent.click(toggle);
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByRole("list", { name: TOOL_CALLS_LABEL })).toBeTruthy();
});

test("a streaming answer keeps the timeline expanded", () => {
  renderMessage({ generating: true });
  expect(screen.queryByRole("button", { name: new RegExp(WORKED_LEAD, "u") })).toBeNull();
  expect(screen.getByRole("list", { name: TOOL_CALLS_LABEL })).toBeTruthy();
});
