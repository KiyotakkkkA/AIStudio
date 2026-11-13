import { plural } from "../connections/connectionPresentation";

export const COMPACT_ACTION = "compact";
export const COMPACT_LABEL = "/compact";
export const COMPACT_DESCRIPTION = "Сжать диалог";
export const COMPACTING = "Сжатие диалога";
export const AUTO_COMPACTING = "Сжатие контекста";
export const COMPACTION_TITLE = "Предыдущие сообщения включены в сводку";
export const COMPACTION_AUTO = "автоматически";
export const COMPACTION_SHOW = "Показать сводку";
export const COMPACTION_HIDE = "Скрыть сводку";
export const COMPACTION_UNDO = "Отменить сжатие";
export const COMPACTION_STOP = "Остановить";
export const COMPACTION_STOPPED = "Сжатие остановлено.";
export const RECAP_TITLE = "Итог";

export function compactionMessages(count: number): string {
  const word = plural(count, "сообщение", "сообщения", "сообщений").slice(String(count).length + 1);
  return `${count.toLocaleString("ru-RU")} ${word}`;
}

export function compactionTokens(before: number, after: number): string {
  return `${before.toLocaleString("ru-RU")} → ${after.toLocaleString("ru-RU")} токенов`;
}
