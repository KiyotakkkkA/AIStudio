import { AppErrorCode, type MemoryDto } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import { plural } from "../connections/connectionPresentation";

export const MEMORY_TOOL = "chat.memory";

export const MEMORY_PANEL = "Память";
export const PANEL_ACCOUNT = "Общая память";
export const PANEL_DIALOG = "Этот диалог";
export const PANEL_EMPTY = "Записей нет.";
export const PANEL_MANAGE = "Управление памятью";
export const PANEL_OFF = "Память выключена: факты не передаются модели.";

export const ENABLED_ROW = {
  key: "memory.enabled",
  label: "Память",
  hint: "Добавлять сохранённые факты о вас при генерации ответа",
} as const;

export const MODEL_ROW = {
  key: "memory.model",
  label: "Сохранение моделью",
  hint: "Разрешить модели сохранять, изменять и удалять данные о вас.",
} as const;

export const SCOPE_TABS_LABEL = "Область памяти";
export const ACCOUNT_TAB = "Общая";
export const DIALOG_TAB = "Диалоги";
export const ACCOUNT_HINT = "Факты о пользователе. Доступны во всех диалогах.";
export const DIALOG_HINT =
  "Факты, которые относятся к одному диалогу. Удаляются вместе с диалогом.";
export const ACCOUNT_PLACEHOLDER = "Новый факт, например: отвечать кратко, примеры кода — на Rust";
export const DIALOG_PLACEHOLDER = "Новый факт для выбранного диалога";
export const DIALOG_PICKER = "Выберите диалог";
export const ADD = "Добавить";
export const SAVE = "Сохранить";
export const CANCEL = "Отмена";
export const EDIT = "Изменить";
export const REMOVE = "Удалить";
export const CLEAR = "Очистить";
export const ACCOUNT_EMPTY_TITLE = "Общая память пуста";
export const ACCOUNT_EMPTY = "Добавьте факт вручную или попросите модель запомнить его в диалоге.";
export const DIALOG_EMPTY_TITLE = "Память диалогов пуста";
export const DIALOG_EMPTY = "Модель сохраняет факты по ходу диалога.";
export const NO_DIALOGS = "Диалогов нет: факт можно добавить после создания первого диалога.";
export const LOADING = "Загрузка памяти…";
export const SOURCE_USER = "Вручную";
export const SOURCE_MODEL = "Модель";
export const DISABLED_NOTE = "Память выключена: сохранённые факты не передаются модели.";

export const CLEAR_ACCOUNT_TITLE = "Очистить общую память";
export const CLEAR_DIALOG_TITLE = "Очистить память диалога";
export const CLEAR_CONSEQUENCE = "Записи будут удалены без возможности восстановления.";
export const REMOVE_TITLE = "Удалить запись";

export const LOAD_ERROR = "Не удалось загрузить память. Попробуйте ещё раз.";
const SAVE_ERROR = "Не удалось сохранить запись. Попробуйте ещё раз.";
export const ALREADY_REMOVED = "Запись уже удалена.";

const HOST_WORDED = new Set<AppErrorCode>([AppErrorCode.CONFLICT]);

export function memoryErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return SAVE_ERROR;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  if (error.code === AppErrorCode.NOT_FOUND) return ALREADY_REMOVED;
  return SAVE_ERROR;
}

export function factsLabel(count: number): string {
  return plural(count, "запись", "записи", "записей");
}

export function memoryDate(memory: MemoryDto): string {
  return new Date(memory.updatedAt).toLocaleString("ru-RU", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function field(value: unknown, name: string): unknown {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)[name]
    : undefined;
}

export function memoryCallSummary(argumentsValue: unknown): string | null {
  const action = field(argumentsValue, "action");
  const content = field(argumentsValue, "content");
  const ref = field(argumentsValue, "ref");
  if (action === "save" && typeof content === "string") return `сохранено: «${content}»`;
  if (action === "update" && typeof content === "string") return `изменено: «${content}»`;
  if (action === "forget" && typeof ref === "string") return `удалена запись ${ref}`;
  return null;
}
