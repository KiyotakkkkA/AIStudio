import { plural } from "../connections/connectionPresentation";

export const EXTENSIONS_TITLE = "Расширения";
export const EXTENSIONS_ID = "browser.extensions";
export const EXTENSIONS_LABEL = "Управление расширениями";
export const EXTENSIONS_HINT =
  "Распакованные расширения Chromium из папки с файлом manifest.json. Магазин расширений не поддерживается.";
export const EXTENSIONS_ADD = "Добавить…";
export const EXTENSIONS_EMPTY = "Расширения не установлены.";
export const EXTENSION_REMOVE = "Удалить расширение";
export const EXTENSION_REMOVE_TITLE = "Удалить расширение?";
export const EXTENSION_ON = "Включено";
export const EXTENSION_OFF = "Выключено";
export const EXTENSION_INVALID = "В выбранной папке нет распакованного расширения Chromium.";
export const EXTENSIONS_FULL = "Достигнуто наибольшее число расширений.";
export const EXTENSIONS_LOAD_FAILED = "Не удалось загрузить список расширений.";
export const EXTENSION_ACTION_FAILED = "Не удалось изменить расширение.";

export function extensionRemoveConsequence(name: string): string {
  return `«${name}» будет удалено из встроенного браузера вместе с его копией в данных приложения.`;
}

export function extensionCountLabel(count: number): string {
  return plural(count, "расширение", "расширения", "расширений");
}
