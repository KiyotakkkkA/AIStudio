import { expect, test } from "vitest";
import type { ComputerAction } from "@zvs/shared";
import {
  STATUS_TEXT,
  VERIFICATION_GENERIC,
  currentAction,
  elapsedText,
  providerName,
  statusLine,
  verificationTitle,
} from "../../src/computer-ui/computerCopy.ts";
import { approvalView } from "../../src/renderer/features/chat/approvalModel.ts";
import { toolPresentation } from "../../src/renderer/features/chat/toolPresentation.ts";
import {
  approvalQuestion,
  toolPresentation as reexported,
} from "../../src/renderer/features/chat/toolCallCopy.ts";

const RUN = "0199aa11-3333-7111-8111-111111111111";
const APPROVAL = "0199aa11-5555-7111-8111-111111111111";

function action(id: number, state: ComputerAction["state"], label: string): ComputerAction {
  return { id, tool: "computer.act", label, state, startedAt: 0 };
}

test("the panel timer counts whole seconds since the turn started", () => {
  expect(elapsedText(undefined, 5_000)).toBeNull();
  expect(elapsedText(1_000, 1_999)).toBe(`${(0).toLocaleString("ru-RU")} с`);
  expect(elapsedText(1_000, 1_390_000)).toBe(`${(1389).toLocaleString("ru-RU")} с`);
  expect(elapsedText(5_000, 1_000)).toBe(`${(0).toLocaleString("ru-RU")} с`);
});

test("a provider verification names the provider that asked for it", () => {
  expect(providerName("https://chat.qwen.ai")).toBe("Qwen");
  expect(providerName("https://chat.z.ai")).toBe("Z.ai");
  expect(providerName("https://chat.example.test")).toBe("chat.example.test");
  expect(verificationTitle([])).toBe(VERIFICATION_GENERIC);
  expect(verificationTitle(["https://chat.z.ai"])).not.toBe(VERIFICATION_GENERIC);
  expect(verificationTitle(["https://chat.z.ai"])).toContain(providerName("https://chat.z.ai"));
});

test("the status line names the action on the screen while the agent works", () => {
  const actions = [
    action(1, "done", "Снимок экрана"),
    action(2, "running", "Нажатие"),
    action(3, "done", "Ввод текста"),
  ];

  expect(currentAction(actions)?.id).toBe(2);
  expect(statusLine("working", actions)).toBe(`${STATUS_TEXT.working}: Нажатие`);
  expect(statusLine("starting", actions)).toBe(STATUS_TEXT.starting);
  expect(statusLine("working", [action(1, "done", "Снимок экрана")])).toBe(STATUS_TEXT.working);
});

test("the panel and the chat read approvals the same way", () => {
  const view = approvalView({
    id: APPROVAL,
    runId: RUN,
    kind: "apps",
    subject: "computer.request_access",
    apps: [{ app: "excel.exe", path: "C:\\excel.exe" }],
    detail: "Заполнить таблицу",
  });

  expect(view).toEqual({
    id: APPROVAL,
    runId: RUN,
    subject: "computer.request_access",
    changes: [],
    apps: [{ app: "excel.exe", path: "C:\\excel.exe" }],
    detail: "Заполнить таблицу",
  });
  expect(approvalView({ id: APPROVAL, subject: "shell.run" })).toBeNull();
});

test("a standalone approval names the tool it asks about", () => {
  expect(approvalQuestion("shell.run")).toBe(
    `Разрешить «${toolPresentation("shell.run").title}» для этого запроса?`,
  );
  expect(approvalQuestion("vendor.custom")).toBe("Разрешить vendor.custom для этого запроса?");
});

test("the panel and the chat share one tool title table", () => {
  expect(toolPresentation).toBe(reexported);
  expect(toolPresentation("shell.run").title).toBe("Выполнить команду в терминале");
  expect(toolPresentation("vendor.custom").title).toBe("vendor.custom");
});
