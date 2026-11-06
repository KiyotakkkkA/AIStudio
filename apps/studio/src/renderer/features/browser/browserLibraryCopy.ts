import type { BrowserImportKind, BrowserImportResultDto } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const IMPORT_OPEN = "Импорт…";
export const IMPORT_TITLE = "Импорт из браузеров";
export const IMPORT_HINT =
  "История и закладки из браузеров, установленных на компьютере. Пароли и файлы cookie не переносятся.";
export const IMPORT_HISTORY = "История";
export const IMPORT_BOOKMARKS = "Закладки";
export const IMPORT_RUN = "Импортировать";
export const IMPORT_RUNNING = "Импорт…";
export const IMPORT_EMPTY = "Поддерживаемые браузеры не найдены.";
export const IMPORT_LOADING = "Поиск браузеров…";
export const IMPORT_FAILED = "Не удалось выполнить импорт.";
export const IMPORT_LOCKED =
  "Не удалось прочитать данные браузера. Закройте его и повторите попытку.";
export const IMPORT_GONE = "Браузер больше не найден. Обновите список.";
export const IMPORT_SOURCES_FAILED = "Не удалось найти установленные браузеры.";
export const IMPORT_DEFAULT_PROFILE = "Основной профиль";

export const IMPORT_KIND_LABELS: Record<BrowserImportKind, string> = {
  chrome: "Google Chrome",
  edge: "Microsoft Edge",
  brave: "Brave",
  vivaldi: "Vivaldi",
  yandex: "Яндекс Браузер",
  opera: "Opera",
  firefox: "Mozilla Firefox",
};

export function importResultLabel(kind: BrowserImportKind, result: BrowserImportResultDto): string {
  const parts = [
    plural(result.history, "страница истории", "страницы истории", "страниц истории"),
    plural(result.bookmarks, "закладка", "закладки", "закладок"),
  ];
  return `${IMPORT_KIND_LABELS[kind]}: добавлено ${parts.join(" и ")}.`;
}

export const HISTORY_ID = "browser.history";
export const HISTORY_LABEL = "История просмотров";
export const HISTORY_HINT = "Страницы, открытые во встроенном браузере.";
export const HISTORY_SEARCH = "Поиск по истории";
export const HISTORY_EMPTY = "История пуста.";
export const HISTORY_NOTHING_FOUND = "Ничего не найдено.";
export const HISTORY_CLEAR = "Очистить историю";
export const HISTORY_CLEAR_TITLE = "Очистить историю просмотров?";
export const HISTORY_CLEAR_CONSEQUENCE =
  "Все записи, в том числе импортированные, будут удалены. Файлы cookie и закладки сохранятся.";
export const HISTORY_REMOVE = "Удалить из истории";
export const HISTORY_IMPORTED = "Импорт";
export const HISTORY_LOAD_FAILED = "Не удалось загрузить историю.";
export const HISTORY_ACTION_FAILED = "Не удалось изменить историю.";

export function historyCountLabel(shown: number, total: number): string {
  const pages = plural(total, "страница", "страницы", "страниц");
  return shown < total ? `Показаны ${shown.toLocaleString("ru-RU")} из ${pages}` : pages;
}

export const BOOKMARKS_ID = "browser.bookmarks";
export const BOOKMARKS_LABEL = "Закладки";
export const BOOKMARKS_HINT =
  "Страницы, отмеченные звёздочкой в адресной строке или импортированные.";
export const BOOKMARKS_EMPTY = "Закладок нет.";
export const BOOKMARK_ROOT = "Без папки";
export const BOOKMARK_RENAME = "Переименовать";
export const BOOKMARK_RENAME_LABEL = "Название закладки";
export const BOOKMARK_SAVE = "Сохранить";
export const BOOKMARK_CANCEL = "Отмена";
export const BOOKMARK_REMOVE = "Удалить закладку";
export const BOOKMARKS_LOAD_FAILED = "Не удалось загрузить закладки.";
export const BOOKMARKS_ACTION_FAILED = "Не удалось изменить закладку.";

export function visitTime(at: number): string {
  return new Date(at).toLocaleString("ru-RU", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
