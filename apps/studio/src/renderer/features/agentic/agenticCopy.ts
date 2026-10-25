import { isIpcError } from "@zvs/ipc";
import { AppErrorCode, type AgenticPermissionMode, type AgenticStopReason } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import type { StatusTone } from "../../ui/atoms/statusTone";
import type { BudgetKey, SessionPhase } from "./sessionModel";

export const PAGE_TITLE = "Агенты";
export const NEW_SESSION = "Новая сессия";
export const START_SESSION = "Запустить";
export const STOP_SESSION = "Остановить";
export const REMOVE_SESSION = "Удалить";
export const BACK_TO_LIST = "К списку сессий";
export const BACK_TO_SESSION = "Назад к сессии";
export const EXTEND_SESSION = "Продлить";

export const LOAD_FAILED = "Не удалось загрузить сессии.";
export const OPEN_FAILED = "Не удалось открыть сессию.";
export const CREATE_FAILED = "Не удалось запустить сессию.";
export const STOP_FAILED = "Не удалось остановить сессию.";
export const REMOVE_FAILED = "Не удалось удалить сессию.";
export const PLAN_FAILED = "Не удалось изменить план.";
export const MODE_FAILED = "Не удалось сменить режим разрешений.";
export const INSTRUCT_FAILED = "Не удалось передать инструкцию.";
export const EXTEND_FAILED = "Не удалось продлить сессию.";
export const DECIDE_FAILED = "Не удалось передать решение по подтверждению.";
export const FOLDER_FAILED = "Не удалось выбрать папку.";

export const EMPTY_TITLE = "Сессий нет";
export const EMPTY_DESCRIPTION =
  "Агентная сессия составляет план, выполняет его инструментами в рабочей папке и показывает каждый вызов.";
export const LOADING = "Загрузка…";

export const PLAN_TITLE = "План";
export const PLAN_EMPTY = "План ещё не составлен";
export const PLAN_PENDING = "Изменения плана применятся на границе шага";
export const PLAN_HINT =
  "Шаги в очереди можно переименовать, удалить и перетащить. Агент получит изменения на границе шага.";
export const EDIT_STEP = "Переименовать шаг";
export const DELETE_STEP = "Удалить шаг";
export const STEP_TITLE_LABEL = "Название шага";
export const STEP_WORKING = "Обработка запроса";

export const GOAL_TITLE = "Цель";
export const STREAM_TITLE = "Ход сессии";
export const SUMMARY_TITLE = "Итог";
export const INSTRUCTION_LABEL = "Инструкция для агента";
export const INSTRUCTION_PLACEHOLDER = "Инструкция для агента — будет передана после текущего шага";
export const INSTRUCTION_SEND = "Отправить";
export const INSTRUCTIONS_QUEUED = "Ожидают границы шага";

export const APPROVAL_TITLE = "Требуется подтверждение";
export const APPROVE_ONCE = "Разрешить один раз";
export const APPROVE_ALWAYS = "Разрешать всегда";
export const DENY = "Отклонить";

export const SESSION_TITLE = "Сессия";
export const MODE_TITLE = "Режим разрешений";
export const MODE_PENDING = "Новый режим применится на границе шага";
export const WORKSPACE_TITLE = "Рабочая папка";
export const CHANGED_TITLE = "Изменённые файлы";
export const CHANGED_EMPTY = "Файлы не изменялись";
export const BUDGET_TITLE = "Бюджет";

export const FORM_GOAL = "Цель";
export const FORM_GOAL_PLACEHOLDER = "Что нужно сделать в рабочей папке";
export const FORM_WORKSPACE = "Рабочая папка";
export const FORM_WORKSPACE_HELP =
  "Файлы и команды сессии ограничены этой папкой. Без неё сессия не запускается.";
export const FORM_PICK_FOLDER = "Выбрать папку";
export const FORM_NO_FOLDER = "Папка не выбрана";
export const FORM_PROVIDER = "Провайдер";
export const FORM_MODEL = "Модель";
export const FORM_NO_PROVIDERS = "Нет включённых провайдеров с генерацией текста";
export const FORM_BUDGET = "Бюджет";
export const FORM_BUDGET_HELP =
  "Пустое поле — значение из настроек. По исчерпании сессия приостанавливается.";
export const FORM_STEPS = "Шаги";
export const FORM_TOKENS = "Токены";
export const FORM_MINUTES = "Минуты";
export const CANCEL = "Отмена";

export const MODE_LABELS: Record<AgenticPermissionMode, string> = {
  "read-only": "Чтение",
  ask: "С подтверждением",
  auto: "Авто",
};

export const MODE_HINTS: Record<AgenticPermissionMode, string> = {
  "read-only": "Доступны только инструменты чтения.",
  ask: "Каждое изменяющее действие подтверждается.",
  auto: "Подтверждаются только разрушающие действия.",
};

export const PHASE_LABELS: Record<SessionPhase, string> = {
  planning: "Составление плана",
  running: "Выполняется",
  approval: "Ждёт подтверждения",
  budget: "Бюджет исчерпан",
  stopped: "Остановлена",
  failed: "Ошибка",
  cancelled: "Остановлена пользователем",
  completed: "Завершена",
};

export const PHASE_TONES: Record<SessionPhase, StatusTone> = {
  planning: "accent",
  running: "accent",
  approval: "warn",
  budget: "warn",
  stopped: "warn",
  failed: "err",
  cancelled: "idle",
  completed: "ok",
};

export const STOP_REASON_COPY: Record<AgenticStopReason, string> = {
  goal: "Цель достигнута.",
  steps: "Исчерпан лимит шагов.",
  tokens: "Исчерпан лимит токенов.",
  time: "Исчерпан лимит времени.",
  loop: "Агент повторял одни и те же действия, сессия остановлена.",
  cancelled: "Сессия остановлена пользователем.",
  error: "Сессия завершилась с ошибкой.",
};

export const BUDGET_LABELS: Record<BudgetKey, string> = {
  steps: "Шаги",
  tokens: "Токены",
  time: "Время",
};

function stepsWord(count: number): string {
  return plural(count, "шаг", "шага", "шагов");
}

export function sessionsWord(count: number): string {
  return plural(count, "сессия", "сессии", "сессий");
}

export function callsWord(count: number): string {
  return plural(count, "вызов", "вызова", "вызовов");
}

export function toolsWord(count: number): string {
  return plural(count, "инструмент", "инструмента", "инструментов");
}

function compactNumber(value: number): string {
  if (value < 1000) return value.toLocaleString("ru-RU");
  return `${(value / 1000).toLocaleString("ru-RU", { maximumFractionDigits: value < 10_000 ? 1 : 0 })} тыс.`;
}

function minutesLabel(ms: number): string {
  const total = Math.floor(ms / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes === 0) return `${seconds.toLocaleString("ru-RU")} с`;
  return `${minutes.toLocaleString("ru-RU")} мин ${seconds.toLocaleString("ru-RU")} с`;
}

export function budgetValue(key: BudgetKey, used: number, limit: number): string {
  if (key === "time") {
    return `${minutesLabel(used)} / ${(limit / 60_000).toLocaleString("ru-RU")} мин`;
  }
  if (key === "tokens") return `${compactNumber(used)} / ${compactNumber(limit)}`;
  return `${used.toLocaleString("ru-RU")} / ${limit.toLocaleString("ru-RU")}`;
}

export function progressLabel(current: number | null, total: number): string {
  return current === null
    ? stepsWord(total)
    : `шаг ${current.toLocaleString("ru-RU")} из ${total.toLocaleString("ru-RU")}`;
}

export function extendLead(steps: number, tokens: number, minutes: number): string {
  return `Новые лимиты: ${stepsWord(steps)}, ${compactNumber(tokens)} токенов, ${minutes.toLocaleString("ru-RU")} мин. План и история сохраняются.`;
}

export function removeLead(goal: string): string {
  return `Сессия «${goal.length > 80 ? `${goal.slice(0, 80)}…` : goal}» и её история будут удалены. Действие нельзя отменить.`;
}

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.VALIDATION_FAILED,
  AppErrorCode.CONFLICT,
  AppErrorCode.NOT_FOUND,
]);

export function agenticErrorCopy(error: unknown, fallback: string): string {
  if (!isIpcError(error)) return fallback;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return fallback;
}
