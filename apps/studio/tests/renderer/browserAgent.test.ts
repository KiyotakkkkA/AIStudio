import { describe, expect, it } from "vitest";
import type { BrowserAgentAction, BrowserAgentApproval } from "@zvs/shared";
import {
  ALWAYS_ASK_TOOLS,
  IRREVERSIBLE,
  STATUS_LINES,
  actionMeta,
  actionTitle,
  approvalDetail,
  approvalTitle,
  authorisationLabel,
  canResume,
  canStop,
  canTakeControl,
  offersAlways,
  paceLabel,
  siteRights,
  timeLabel,
} from "../../src/browser-ui/agentCopy";

const AT = Date.UTC(2026, 9, 27, 17, 58);
const RATE_LIMIT_PAUSE = "Превышен предел действий в минуту";

function action(overrides: Partial<BrowserAgentAction>): BrowserAgentAction {
  return {
    id: 1,
    tool: "browser.click",
    target: "button «Отправить»",
    state: "done",
    startedAt: AT,
    ...overrides,
  };
}

function approval(overrides: Partial<BrowserAgentApproval>): BrowserAgentApproval {
  return {
    id: "0199aa22-2222-7222-8222-222222222222",
    tool: "browser.click",
    origin: "https://portal.acme.io",
    target: "button «В корзину»",
    canAlways: true,
    ...overrides,
  };
}

describe("browser agent panel", () => {
  it("names the authorisation of every logged action", () => {
    expect(authorisationLabel({ kind: "auto" })).toBe("авто");
    expect(authorisationLabel({ kind: "granted", at: AT })).toBe(
      `разрешено для сайта · ${timeLabel(AT)}`,
    );
    expect(authorisationLabel({ kind: "approved", at: AT })).toBe(
      `подтверждено в ${timeLabel(AT)}`,
    );
    expect(actionMeta(action({ authorisation: { kind: "auto" }, durationMs: 412 }))).toBe(
      "авто · 412 мс",
    );
    expect(
      actionMeta(
        action({
          state: "failed",
          authorisation: { kind: "granted", at: AT },
          durationMs: 1500,
          error: "Элемент перекрыт",
        }),
      ),
    ).toBe(`разрешено для сайта · ${timeLabel(AT)} · 1,5 с · Элемент перекрыт`);
    expect(actionMeta(action({ state: "waiting" }))).toBe("ожидает подтверждения");
    expect(actionMeta(action({ state: "denied" }))).toBe("отклонено");
    expect(actionMeta(action({ state: "paused", error: RATE_LIMIT_PAUSE }))).toBe(RATE_LIMIT_PAUSE);
    expect(actionTitle(action({}))).toBe("нажатие · button «Отправить»");
    expect(actionTitle(action({ tool: "browser.read_page", target: "" }))).toBe("чтение страницы");
  });

  it("never offers 'always' for the always-ask tiers", () => {
    expect(offersAlways(approval({}))).toBe(true);
    expect(offersAlways(approval({ canAlways: false }))).toBe(false);
    for (const tool of ALWAYS_ASK_TOOLS) {
      expect(offersAlways(approval({ tool, canAlways: true }))).toBe(false);
    }
    expect(offersAlways(approval({ tool: "browser.type" }))).toBe(true);
  });

  it("names the action, the site and the irreversibility in the approval bar", () => {
    const submit = approval({ tool: "browser.submit", target: "form «Заказ»", canAlways: false });
    expect(approvalTitle(submit)).toBe("Подтверждение: отправка формы на portal.acme.io");
    expect(approvalDetail(submit)).toBe(`form «Заказ» · ${IRREVERSIBLE}`);
    expect(approvalDetail(approval({ tool: "browser.type", text: "СПб" }))).toBe(
      "button «В корзину» · «СПб»",
    );
    expect(approvalTitle(approval({ tool: "browser.read_page", origin: "" }))).toBe(
      "Подтверждение: чтение страницы",
    );
  });

  it("derives the site rights from the grants", () => {
    const grant = { id: "g", tool: "browser.click", grantedAt: AT };
    const rights = siteRights([grant]);
    expect(rights.map((right) => right.tier)).toEqual(["auto", "granted", "ask", "ask", "never"]);
    expect(rights[1].grant).toEqual(grant);
    expect(siteRights([]).some((right) => right.tier === "granted")).toBe(false);
  });

  it("offers each control only in the states it applies to", () => {
    expect(canTakeControl("running")).toBe(true);
    expect(canTakeControl("approval")).toBe(true);
    expect(canTakeControl("control")).toBe(false);
    expect(canTakeControl("idle")).toBe(false);
    expect(canResume("paused")).toBe(true);
    expect(canResume("control")).toBe(true);
    expect(canResume("running")).toBe(false);
    expect(canStop("paused")).toBe(true);
    expect(canStop("finished")).toBe(false);
    expect(canStop("stopped")).toBe(false);
    expect(STATUS_LINES.idle).toBe("");
  });

  it("counts the pace with Russian plurals", () => {
    expect(paceLabel(1, 20)).toBe("1 действие за минуту из 20");
    expect(paceLabel(4, 20)).toBe("4 действия за минуту из 20");
    expect(paceLabel(11, 20)).toBe("11 действий за минуту из 20");
  });
});
