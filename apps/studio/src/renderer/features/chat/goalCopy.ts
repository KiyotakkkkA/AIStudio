import type { ChatGoalDto, GoalReason, GoalStatus } from "@zvs/shared";

export const GOAL_COMMAND = "goal";
export const GOAL_TOOL = "goal.complete";
export const GOAL_PLAN_TOOL = "goal.plan";
export const GOAL_STEP_TOOL = "goal.step";
export const GOAL_PLAN_TOOL_TITLE = "План цели";
export const GOAL_STEP_TOOL_TITLE = "Шаг плана выполнен";
export const GOAL_PLAN = "План";
export const GOAL_PLAN_LABEL = "План цели";
export const GOAL_STEP_FIELD = "Шаг";
export const GOAL_STEP_SUMMARY = "Итог";
export const GOAL_PLACEHOLDER = "Опишите цель…";
export const GOAL_EMPTY = "Укажите цель после команды /goal";
export const GOAL_LABEL = "Цель";
export const GOAL_DONE = "Цель достигнута";
export const GOAL_STOP = "Остановить";
export const GOAL_STOPPING = "Остановка…";
export const GOAL_CONTINUE = "Продолжить";
export const GOAL_DISMISS = "Скрыть";
export const GOAL_EVIDENCE = "Подтверждение";
export const GOAL_TOOL_TITLE = "Завершение цели";
export const GOAL_STOP_FAILED = "Не удалось остановить цель";
export const GOAL_CONTINUE_FAILED = "Не удалось продолжить цель";

const STATUS: Record<GoalStatus, string> = {
  active: "Выполняется",
  paused: "Приостановлено",
  done: GOAL_DONE,
  stopped: "Остановлено",
  failed: "Ошибка",
};

const REASON: Record<GoalReason, string> = {
  rounds: "исчерпан лимит раундов",
  tokens: "исчерпан лимит токенов",
  minutes: "исчерпан лимит времени",
  repetition: "повторяющиеся вызовы",
  restart: "приложение было перезапущено",
  user: "по запросу",
  error: "обработка запроса завершилась ошибкой",
};

export function planProgressText(done: number, total: number): string {
  return `${done.toLocaleString("ru-RU")} из ${total.toLocaleString("ru-RU")}`;
}

export function goalStatusText(goal: Pick<ChatGoalDto, "status" | "reason">): string {
  const status = STATUS[goal.status];
  if (goal.reason === null || goal.status === "failed") return status;
  if (goal.status === "stopped" && goal.reason === "user") return status;
  return `${status}: ${REASON[goal.reason]}`;
}

export function goalRoundText(round: number, total: number): string {
  return `Раунд ${round.toLocaleString("ru-RU")} из ${total.toLocaleString("ru-RU")}`;
}

export function goalTokensText(used: number, total: number): string {
  return `Токены: ${used.toLocaleString("ru-RU")} из ${total.toLocaleString("ru-RU")}`;
}

export function goalMinutesText(elapsedMs: number): string {
  return `${Math.floor(elapsedMs / 60_000).toLocaleString("ru-RU")} мин`;
}
