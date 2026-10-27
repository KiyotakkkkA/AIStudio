import { AppErrorCode, EXPLORER_LIST_MAX } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const PANEL_GROUP_LABEL = "Боковая панель";
export const CONTEXT_PANEL = "Контекст";
export const FILES_PANEL = "Файлы";
export const TREE_LABEL = "Файлы рабочей папки";
export const COLLAPSE_ALL = "Свернуть всё";
export const CLEAR_MARKS = "Сбросить отметки";
export const LOADING = "Загрузка…";
export const EMPTY_FOLDER = "Папка пуста";
export const EMPTY_FOLDER_HINT = "В рабочей папке нет файлов.";
export const ACCESS_MISSING = "Нет доступа к рабочей папке";
export const ACCESS_MISSING_HINT = "Разрешение на эту папку отозвано или не выдано.";
export const FOLDER_GONE = "Рабочая папка недоступна";
export const FOLDER_GONE_HINT = "Папка удалена, перемещена или находится на отключённом диске.";
export const LIST_TRUNCATED = `Показаны первые ${EXPLORER_LIST_MAX.toLocaleString("ru-RU")} элементов`;
export const MARK_CREATED = "Новый";
export const MARK_MODIFIED = "Изменён";
export const MARK_INSIDE = "Есть изменения внутри";
export const IGNORED_HINT = "Исключено в .gitignore";

export const FILE_VIEW_LABEL = "Просмотр файла";
export const CLOSE_FILE = "Закрыть файл";
export const BACK_TO_CHAT = "Назад к диалогу";
export const RESIZE_VIEW = "Изменить ширину просмотра";
export const MODE_LABEL = "Режим просмотра";
export const MODE_PREVIEW = "Просмотр";
export const MODE_TEXT = "Текст";
export const FILE_LOADING = "Загрузка файла…";
export const BINARY_TITLE = "Двоичный файл";
export const BINARY_HINT = "Содержимое не отображается";
export const FILE_TRUNCATED = "Показан первый 1 МБ файла";
export const FILE_REMOVED = "Файл удалён";
export const FILE_REMOVED_HINT = "Ниже показано последнее загруженное содержимое.";
export const FILE_EMPTY = "Файл пуст";

export const LIST_ERROR_FALLBACK = "Не удалось загрузить содержимое папки.";
export const READ_ERROR_FALLBACK = "Не удалось открыть файл.";

export type RootState = "ready" | "missing" | "gone";

export function rootStateOf(error: unknown): RootState | null {
  if (!isIpcError(error)) return null;
  if (error.code === AppErrorCode.PERMISSION_DENIED) return "missing";
  if (error.code === AppErrorCode.NOT_FOUND || error.code === AppErrorCode.CONFLICT) return "gone";
  return null;
}

const READ_ERRORS: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.NOT_FOUND]: "Файл не найден.",
  [AppErrorCode.PERMISSION_DENIED]:
    "Файл находится за пределами рабочей папки или доступ к ней закрыт.",
  [AppErrorCode.VALIDATION_FAILED]: "Файл не удалось прочитать.",
};

export function readErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return READ_ERROR_FALLBACK;
  return READ_ERRORS[error.code] ?? READ_ERROR_FALLBACK;
}

export function isMarkdown(path: string): boolean {
  return /\.(md|markdown)$/i.test(path);
}

export function baseName(path: string): string {
  return path.split("/").pop() ?? path;
}

export function modifiedLabel(timestamp: number | null): string | null {
  return timestamp === null ? null : new Date(timestamp).toLocaleString("ru-RU", { hour12: false });
}
