import type { ComputerAction, ComputerStatus } from "@zvs/shared";

export const PANEL_TITLE = "Управление компьютером";
export const BADGE_TEXT = "Устройство находится под управлением";
export const STOP_HINT = "LShift+Esc — остановить";
export const STOP = "Остановить";
export const COLLAPSE = "Свернуть панель";
export const EXPAND = "Развернуть панель";
export const FEED_TITLE = "Ход выполнения";
export const FEED_EMPTY = "Нет сообщений";
export const ELAPSED_TITLE = "Время ответа";
export const COMPOSER_PLACEHOLDER = "Уточнить задачу…";
export const SEND = "Отправить";
export const COMMAND_FAILED = "Не удалось выполнить команду";
export const VERIFICATION_LEAD =
  "Работа приостановлена до прохождения антибот проверки";
export const VERIFICATION_OPEN = "Открыть проверку";
export const VERIFICATION_GENERIC = "Провайдер запросил антибот проверку";

const PROVIDER_NAMES: Record<string, string> = {
  "https://chat.qwen.ai": "Qwen",
  "https://chat.z.ai": "Z.ai",
};

export function providerName(origin: string): string {
  const known = PROVIDER_NAMES[origin];
  if (known !== undefined) return known;
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}

export function verificationTitle(origins: readonly string[]): string {
  const names = origins.map(providerName).join(", ");
  return names === "" ? VERIFICATION_GENERIC : `Провайдер ${names} запросил антибот проверку`;
}

export const STATUS_TEXT: Record<ComputerStatus, string> = {
  idle: "Ожидание",
  starting: "Обработка запроса",
  working: "Выполнение действий",
  awaiting: "Требуется подтверждение",
  stopping: "Остановка",
};

export function currentAction(actions: readonly ComputerAction[]): ComputerAction | null {
  for (let index = actions.length - 1; index >= 0; index -= 1) {
    const action = actions[index];
    if (action !== undefined && (action.state === "running" || action.state === "waiting")) {
      return action;
    }
  }
  return null;
}

export function statusLine(status: ComputerStatus, actions: readonly ComputerAction[]): string {
  const action = status === "working" ? currentAction(actions) : null;
  return action === null ? STATUS_TEXT[status] : `${STATUS_TEXT[status]}: ${action.label}`;
}

export function elapsedText(startedAt: number | undefined, now: number): string | null {
  if (startedAt === undefined) return null;
  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  return `${seconds.toLocaleString("ru-RU")} с`;
}
