import type { BrowserSiteAction, BrowserSiteDecision } from "@zvs/shared";

export const SITES_TITLE = "Разрешения сайтов";
export const SITES_HINT =
  "Действия модели в браузере по умолчанию и исключения для отдельных сайтов.";
export const SITES_ID = "browser.sites";
export const SITES_ADD = "Добавить";
export const SITE_COLUMN = "Сайт";
export const SITE_DEFAULT_ROW = "По умолчанию";
export const SITE_USE_DEFAULT = "По умолчанию";
export const SITE_PLACEHOLDER = "https://example.com";
export const SITE_INPUT_LABEL = "Адрес сайта";
export const SITE_ADD_CONFIRM = "Добавить";
export const SITE_ADD_CANCEL = "Отмена";
export const SITE_REMOVE = "Удалить исключение";
export const SITE_REMOVE_TITLE = "Удалить исключение для сайта?";
export const SITES_EMPTY = "Здесь появятся сайты с собственными разрешениями.";
export const SITE_INVALID = "Укажите адрес сайта HTTP или HTTPS.";
export const SITES_LOAD_FAILED = "Не удалось загрузить разрешения сайтов.";
export const SITES_SAVE_FAILED = "Не удалось сохранить разрешение.";

export const SITE_ACTION_LABELS: Record<BrowserSiteAction, string> = {
  browse: "Просмотр",
  click: "Нажатие",
  type: "Ввод",
  submit: "Отправка форм",
  download: "Загрузка файлов",
};

export const SITE_ACTION_HINTS: Record<BrowserSiteAction, string> = {
  browse: "Переход по адресу, чтение страницы, поиск элементов и снимок экрана",
  click: "Нажатие на элементы страницы",
  type: "Ввод текста в поля, кроме полей паролей и платёжных данных",
  submit: "Отправка форм",
  download: "Скачивание файлов",
};

export const SITE_DECISION_LABELS: Record<BrowserSiteDecision, string> = {
  ask: "Спрашивать",
  allow: "Разрешено",
  block: "Запрещено",
};

export function siteRemoveConsequence(host: string): string {
  return `Для ${host} снова будут действовать разрешения по умолчанию.`;
}

export function siteHost(origin: string): string {
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}
