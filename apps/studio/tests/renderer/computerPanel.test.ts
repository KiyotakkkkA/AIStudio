import { expect, test } from "vitest";
import {
  ACTION_TITLE,
  REQUEST_TITLE,
  elapsedText,
  requestTitle,
} from "../../src/computer-ui/computerCopy.ts";
import { toolPresentation } from "../../src/renderer/features/chat/toolPresentation.ts";
import { toolPresentation as reexported } from "../../src/renderer/features/chat/toolCallCopy.ts";

test("the panel timer counts whole seconds since the turn started", () => {
  expect(elapsedText(undefined, 5_000)).toBeNull();
  expect(elapsedText(1_000, 1_999)).toBe(`${(0).toLocaleString("ru-RU")} с`);
  expect(elapsedText(1_000, 1_390_000)).toBe(`${(1389).toLocaleString("ru-RU")} с`);
  expect(elapsedText(5_000, 1_000)).toBe(`${(0).toLocaleString("ru-RU")} с`);
});

test("each confirmation kind has its own title and unknown kinds fall back", () => {
  expect(requestTitle("process")).toBe(REQUEST_TITLE.process);
  expect(requestTitle("folder")).toBe(REQUEST_TITLE.folder);
  expect(requestTitle("plugin")).toBe(ACTION_TITLE);
});

test("the panel and the chat share one tool title table", () => {
  expect(toolPresentation).toBe(reexported);
  expect(toolPresentation("shell.run").title).toBe("Выполнить команду в терминале");
  expect(toolPresentation("vendor.custom").title).toBe("vendor.custom");
});
