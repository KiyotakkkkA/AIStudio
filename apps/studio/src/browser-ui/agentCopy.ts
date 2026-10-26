import type {
  BrowserAgentAction,
  BrowserAgentApproval,
  BrowserAgentStatus,
  BrowserAuthorisation,
  BrowserSiteGrant,
} from "@zvs/shared";
import { plural } from "./browserCopy";

export const AGENT_TITLE = "Агент";
export const AGENT_OPEN = "Показать панель агента";
export const AGENT_CLOSE = "Скрыть панель агента";
export const TASK_TITLE = "Задача";
export const ACTIONS_TITLE = "Действия";
export const RIGHTS_TITLE = "Права на этом сайте";
export const NO_ACTIONS = "Действий нет.";
export const NO_SITE = "Нет открытого сайта.";
export const NO_TASK = "Задача не указана.";
export const TAKE_CONTROL = "Взять управление";
export const RESUME = "Продолжить";
export const STOP = "Остановить";
export const REVOKE = "Отозвать разрешение";
export const DENY = "Отклонить";
export const ALWAYS = "Всегда на этом сайте";
export const APPROVE_ONCE = "Разрешить один раз";
export const IRREVERSIBLE = "действие необратимо";
export const TARGET_LABEL = "Цель";
export const AGENT_ACTION_FAILED =
  "Действие не выполнено: состояние сеанса агента изменилось. Повторите действие.";

export const ALWAYS_ASK_TOOLS: ReadonlySet<string> = new Set([
  "browser.submit",
  "browser.download",
]);

export const STATUS_LABELS: Readonly<Record<BrowserAgentStatus, string>> = {
  idle: "Сеанс не запущен",
  running: "Выполнение",
  approval: "Ожидает подтверждения",
  paused: "Приостановлено: превышен предел действий",
  control: "Управление у пользователя",
  stopped: "Остановлено",
  finished: "Завершено",
};

export const STATUS_LINES: Readonly<Record<BrowserAgentStatus, string>> = {
  idle: "",
  running: "Сеанс агента: выполнение",
  approval: "Сеанс агента приостановлен — требуется подтверждение",
  paused: "Сеанс агента приостановлен — превышен предел действий в минуту",
  control: "Управление у пользователя — сеанс агента приостановлен",
  stopped: "Сеанс агента остановлен",
  finished: "Сеанс агента завершён",
};

export type StatusTone = "ok" | "busy" | "warn" | "err" | "idle";

export const STATUS_TONES: Readonly<Record<BrowserAgentStatus, StatusTone>> = {
  idle: "idle",
  running: "busy",
  approval: "warn",
  paused: "warn",
  control: "warn",
  stopped: "err",
  finished: "ok",
};

export const TOOL_ACTIONS: Readonly<Record<string, string>> = {
  "browser.navigate": "переход",
  "browser.read_page": "чтение страницы",
  "browser.screenshot": "снимок страницы",
  "browser.find": "поиск на странице",
  "browser.click": "нажатие",
  "browser.type": "ввод текста",
  "browser.submit": "отправка формы",
  "browser.download": "загрузка файла",
};

export function toolAction(tool: string): string {
  return TOOL_ACTIONS[tool] ?? tool;
}

export function timeLabel(ms: number): string {
  return new Date(ms).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
}

export function durationLabel(ms: number): string {
  return ms < 1000
    ? `${ms.toLocaleString("ru-RU")} мс`
    : `${(ms / 1000).toLocaleString("ru-RU", { maximumFractionDigits: 1 })} с`;
}

export function authorisationLabel(authorisation: BrowserAuthorisation): string {
  switch (authorisation.kind) {
    case "auto":
      return "авто";
    case "granted":
      return `разрешено для сайта · ${timeLabel(authorisation.at)}`;
    case "approved":
      return `подтверждено в ${timeLabel(authorisation.at)}`;
  }
}

export function actionMeta(action: BrowserAgentAction): string {
  switch (action.state) {
    case "waiting":
      return "ожидает подтверждения";
    case "denied":
      return "отклонено";
    case "paused":
      return action.error ?? STATUS_LABELS.paused;
    case "running":
      return action.authorisation
        ? `${authorisationLabel(action.authorisation)} · выполнение…`
        : "выполнение…";
    case "failed":
    case "done": {
      const parts = [
        action.authorisation ? authorisationLabel(action.authorisation) : null,
        action.durationMs === undefined ? null : durationLabel(action.durationMs),
        action.state === "failed" ? (action.error ?? "ошибка") : null,
      ];
      return parts.filter((part): part is string => part !== null).join(" · ");
    }
  }
}

export function actionTitle(action: Pick<BrowserAgentAction, "tool" | "target">): string {
  return action.target ? `${toolAction(action.tool)} · ${action.target}` : toolAction(action.tool);
}

export function actionsWord(count: number): string {
  return plural(count, "действие", "действия", "действий");
}

export function paceLabel(pace: number, limit: number): string {
  return `${actionsWord(pace)} за минуту из ${limit.toLocaleString("ru-RU")}`;
}

export function stepLabel(steps: number): string {
  return `шаг ${steps.toLocaleString("ru-RU")}`;
}

export function siteName(origin: string): string {
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}

export function offersAlways(approval: BrowserAgentApproval): boolean {
  return approval.canAlways && !ALWAYS_ASK_TOOLS.has(approval.tool);
}

export function approvalTitle(approval: BrowserAgentApproval): string {
  const site = approval.origin ? ` на ${siteName(approval.origin)}` : "";
  return `Подтверждение: ${toolAction(approval.tool)}${site}`;
}

export function approvalDetail(approval: BrowserAgentApproval): string {
  const parts = [approval.target];
  if (approval.text !== undefined) parts.push(`«${approval.text}»`);
  if (ALWAYS_ASK_TOOLS.has(approval.tool)) parts.push(IRREVERSIBLE);
  return parts.join(" · ");
}

export type RightTier = "auto" | "granted" | "ask" | "never";

export const RIGHT_TIER_LABELS: Readonly<Record<RightTier, string>> = {
  auto: "авто",
  granted: "разрешено",
  ask: "спрашивать",
  never: "никогда",
};

export interface SiteRight {
  readonly label: string;
  readonly tier: RightTier;
  readonly grant?: BrowserSiteGrant;
}

export function siteRights(grants: readonly BrowserSiteGrant[]): SiteRight[] {
  const granted = (tool: string): SiteRight["grant"] => grants.find((grant) => grant.tool === tool);
  const perSite = (label: string, tool: string): SiteRight => {
    const grant = granted(tool);
    return grant ? { label, tier: "granted", grant } : { label, tier: "ask" };
  };
  return [
    { label: "Чтение и переход", tier: "auto" },
    perSite("Нажатия", "browser.click"),
    perSite("Ввод текста", "browser.type"),
    { label: "Отправка форм и загрузки", tier: "ask" },
    { label: "Пароли и оплата", tier: "never" },
  ];
}

export function canTakeControl(status: BrowserAgentStatus): boolean {
  return status === "running" || status === "approval";
}

export function canResume(status: BrowserAgentStatus): boolean {
  return status === "paused" || status === "control";
}

export function canStop(status: BrowserAgentStatus): boolean {
  return (
    status === "running" || status === "approval" || status === "paused" || status === "control"
  );
}
