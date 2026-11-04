import type {
  ChatToolCallStatus,
  ComputerActionState,
  ComputerStatus,
  FolderAccessAnswer,
} from "@zvs/shared";

export const PANEL_TITLE = "Управление компьютером";
export const BADGE_TEXT = "Устройство находится под управлением";
export const STOP_HINT = "LShift+Esc — остановить";
export const STOP = "Остановить";
export const COLLAPSE = "Свернуть панель";
export const EXPAND = "Развернуть панель";
export const ACTIONS_TITLE = "Действия на экране";
export const ACTIONS_EMPTY = "Действий пока нет";
export const CALLS_TITLE = "Инструменты";
export const CALLS_EMPTY = "Вызовов инструментов нет";
export const REPLY_TITLE = "Ответ";
export const REPLY_EMPTY = "Текст ответа пока не получен";
export const ELAPSED_TITLE = "Время ответа";
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

export const CALL_STATUS_TEXT: Record<ChatToolCallStatus, string> = {
  running: "Выполняется",
  "awaiting-approval": "Ждёт подтверждения",
  "awaiting-answer": "Ждёт ответа",
  succeeded: "Выполнено",
  failed: "Ошибка",
  refused: "Не выполнено",
  denied: "Отклонено",
  cancelled: "Отменено",
};

export const ACCESS_ANSWERS: readonly { answer: FolderAccessAnswer; label: string }[] = [
  { answer: "once", label: "Только для этой сессии" },
  { answer: "conversation", label: "Разрешить для этого диалога" },
  { answer: "always", label: "Разрешить всегда" },
  { answer: "deny", label: DENY },
];

export const FOLDER_ANSWERS: readonly { answer: FolderAccessAnswer; label: string }[] = [
  { answer: "once", label: "Разрешить однократно" },
  { answer: "conversation", label: "Разрешить для этого диалога" },
  { answer: "always", label: "Разрешить всегда" },
  { answer: "deny", label: DENY },
];

export const FOLDER_REQUEST_KIND = "folder";

export const REQUEST_TITLE: Readonly<Record<string, string>> = {
  process: "Подтвердите запуск команды",
  files: "Подтвердите изменение файлов",
  folder: "Запрос доступа к папкам",
  mail: "Подтвердите отправку письма",
};

export function requestTitle(kind: string): string {
  return REQUEST_TITLE[kind] ?? ACTION_TITLE;
}

export function elapsedText(startedAt: number | undefined, now: number): string | null {
  if (startedAt === undefined) return null;
  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  return `${seconds.toLocaleString("ru-RU")} с`;
}
