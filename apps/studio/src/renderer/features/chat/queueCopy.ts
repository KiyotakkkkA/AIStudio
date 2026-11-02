import { plural } from "../connections/connectionPresentation";

export const QUEUE_LABEL = "Очередь сообщений";
export const QUEUE_SEND_NOW = "Отправить все сейчас";
export const QUEUE_SENDING = "Отправка после текущего шага";
export const QUEUE_EDIT = "Изменить сообщение";
export const QUEUE_REMOVE = "Удалить сообщение";
export const QUEUE_SAVE = "Сохранить";
export const QUEUE_CANCEL = "Отмена";
export const QUEUE_EDIT_FIELD = "Текст сообщения в очереди";
export const QUEUE_HINT = "Сообщение будет добавлено в очередь и отправлено после ответа";
export const QUEUE_ADD_FAILED = "Не удалось добавить сообщение в очередь";
export const QUEUE_EDIT_FAILED = "Не удалось изменить сообщение";
export const QUEUE_REMOVE_FAILED = "Не удалось удалить сообщение";
export const QUEUE_SEND_FAILED = "Не удалось отправить сообщения из очереди";

export function queuedWord(count: number): string {
  return plural(count, "сообщение", "сообщения", "сообщений");
}
