import type { ComputerActionState, ComputerStatus, FolderAccessAnswer } from "@zvs/shared";

export const PANEL_TITLE = "Управление компьютером";
export const BADGE_TEXT = "ИИ управляет компьютером";
export const STOP_HINT = "LShift+Esc — остановить";
export const STOP = "Остановить";
export const COLLAPSE = "Свернуть панель";
export const EXPAND = "Развернуть панель";
export const ACTIONS_EMPTY = "Действий пока нет";
export const COMPOSER_PLACEHOLDER = "Уточнить задачу…";
export const SEND = "Отправить";
export const ACCESS_TITLE = "Запрос доступа к приложениям";
export const ACTION_TITLE = "Подтвердите действие";
export const ALLOW = "Разрешить";
export const DENY = "Запретить";
export const COMMAND_FAILED = "Не удалось выполнить команду";

export const STATUS_TEXT: Record<ComputerStatus, string> = {
  idle: "Ожидание",
  starting: "Обработка сообщения",
  working: "Выполнение действий",
  awaiting: "Требуется подтверждение",
  stopping: "Остановка",
};

export const ACTION_STATE_TEXT: Record<ComputerActionState, string> = {
  running: "Выполняется",
  waiting: "Ждёт подтверждения",
  done: "Выполнено",
  failed: "Ошибка",
  denied: "Отклонено",
};

export const ACCESS_ANSWERS: readonly { answer: FolderAccessAnswer; label: string }[] = [
  { answer: "once", label: "Только для этого ответа" },
  { answer: "conversation", label: "Разрешить для этого диалога" },
  { answer: "always", label: "Разрешить всегда" },
  { answer: "deny", label: DENY },
];
