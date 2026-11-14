import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const NOTES_PANEL = "Заметки";
export const NOTES_LIST_LABEL = "Заметки диалога";
export const NOTES_EMPTY = "Заметок нет";
export const NOTES_EMPTY_HINT = "Подробные сведения, сохранённые в ходе диалога.";
export const NOTES_LOADING = "Загрузка заметок";
export const NOTES_LOAD_ERROR = "Не удалось загрузить заметки";
export const NOTES_LOAD_HINT = "Список заметок недоступен.";
export const NOTES_RETRY = "Повторить";
export const NOTE_INVALID = "Ошибка";
export const NOTE_VIEW_LABEL = "Просмотр заметки";
export const NOTE_LOADING = "Загрузка заметки";
export const NOTE_REMOVED = "Заметка удалена";
export const NOTE_REMOVED_HINT = "Заметка отсутствует в текущем диалоге.";
export const NOTE_CLOSE = "Закрыть заметку";
export const NOTE_BACK = "Назад к диалогу";
export const NOTE_EDIT = "Изменить";
export const NOTE_SAVE = "Сохранить";
export const NOTE_CANCEL = "Отмена";
export const NOTE_RENAME = "Переименовать";
export const NOTE_RENAME_LABEL = "Имя файла заметки";
export const NOTE_RENAME_INVALID =
  'Требуется имя файла с расширением .md без символов / \\ : * ? " < > |';
export const NOTE_DELETE = "Удалить";
export const NOTE_DELETE_TITLE = "Удаление заметки";
export const NOTE_RELOAD = "Открыть заново";
export const NOTE_EDITOR_LABEL = "Текст заметки";
export const NOTE_SOURCE = "Заметка";
export const OPEN_NOTE = "Открыть заметку";
export const NOTE_CONTEXT = "Контекст";
export const NOTE_READ_WHEN = "Условия чтения";
export const NOTE_UPDATE_WHEN = "Условия обновления";
export const NOTE_DELETE_WHEN = "Условия удаления";
export const NOTE_CREATED = "Создана";
export const NOTE_UPDATED = "Изменена";

const SAVE_FALLBACK = "Не удалось сохранить заметку";
const ACTION_FALLBACK = "Не удалось выполнить действие с заметкой";
const OPEN_FALLBACK = "Не удалось открыть заметку";

export function noteDeleteBody(name: string): string {
  return `Заметка «${name}» будет удалена без возможности восстановления.`;
}

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.VALIDATION_FAILED,
  AppErrorCode.CONFLICT,
  AppErrorCode.LIMIT_EXCEEDED,
]);

function hostWorded(error: unknown, fallback: string): string {
  if (isIpcError(error) && HOST_WORDED.has(error.code) && error.message.length > 0) {
    return error.message;
  }
  return fallback;
}

export function noteSaveErrorCopy(error: unknown): string {
  return hostWorded(error, SAVE_FALLBACK);
}

export function noteActionErrorCopy(error: unknown): string {
  return hostWorded(error, ACTION_FALLBACK);
}

export function noteOpenErrorCopy(error: unknown): string {
  return hostWorded(error, OPEN_FALLBACK);
}

export function isMissing(error: unknown): boolean {
  return isIpcError(error) && error.code === AppErrorCode.NOT_FOUND;
}

export function isConflict(error: unknown): boolean {
  return isIpcError(error) && error.code === AppErrorCode.CONFLICT;
}

export function noteDate(value: string | null): string | null {
  if (value === null) return null;
  const at = new Date(value);
  if (Number.isNaN(at.getTime())) return null;
  return at.toLocaleString("ru-RU", {
    day: "numeric",
    month: "long",
    hour: "2-digit",
    minute: "2-digit",
  });
}
