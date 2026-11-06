import type { BrowserDownloadState } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import { formatBytes } from "../downloads/downloadPresentation";

export const CLEAR_DATA_ID = "browser.clearData";
export const CLEAR_DATA_LABEL = "Данные браузера";
export const CLEAR_DATA_HINT =
  "Файлы cookie, кэш, данные сайтов, история просмотров и загрузок встроенного браузера.";
export const CLEAR_DATA = "Очистить данные";
export const CLEAR_DATA_TITLE = "Очистить данные браузера?";
export const CLEAR_DATA_CONSEQUENCE =
  "Будет выполнен выход со всех сайтов, включая аккаунты провайдеров, связанные через браузер. Разрешения сайтов сохранятся.";
export const CLEAR_DATA_DONE = "Данные браузера очищены.";
export const CLEAR_DATA_FAILED = "Не удалось очистить данные браузера.";

export const AUTOFILL_TITLE = "Автозаполнение и пароли";
export const PASSWORDS_ID = "browser.passwords";
export const PASSWORDS_LABEL = "Сохранение паролей";
export const PASSWORDS_HINT =
  "Предложение сохранить пароль после входа на сайт и подстановка сохранённых паролей.";
export const FORMS_ID = "browser.forms";
export const FORMS_LABEL = "Автозаполнение форм";
export const FORMS_HINT = "Подсказки адресов, телефонов и других данных, введённых ранее.";

export const DOWNLOADS_TITLE = "Загрузки";
export const DOWNLOAD_DIR_ID = "browser.downloadDir";
export const DOWNLOAD_DIR_LABEL = "Папка загрузок";
export const DOWNLOAD_DIR_SYSTEM = "Системная папка «Загрузки»";
export const DOWNLOAD_DIR_CHANGE = "Изменить";
export const DOWNLOAD_DIR_RESET = "По умолчанию";
export const ASK_DOWNLOAD_ID = "browser.askDownload";
export const ASK_DOWNLOAD_LABEL = "Спрашивать, куда сохранить";
export const ASK_DOWNLOAD_HINT =
  "Окно сохранения после загрузки файла, начатой во встроенном браузере.";
export const DOWNLOAD_HISTORY_ID = "browser.downloadHistory";
export const DOWNLOAD_HISTORY_LABEL = "История загрузок";
export const DOWNLOAD_HISTORY_HINT = "Файлы, загруженные во встроенном браузере.";
export const MANAGE = "Управление";
export const BACK = "Назад";

export const DOWNLOADS_EMPTY = "Загрузок нет.";
export const DOWNLOADS_CLEAR = "Очистить историю";
export const DOWNLOADS_CLEAR_TITLE = "Очистить историю загрузок?";
export const DOWNLOADS_CLEAR_CONSEQUENCE = "Записи будут удалены, сами файлы останутся на диске.";
export const DOWNLOAD_OPEN = "Открыть файл";
export const DOWNLOAD_REVEAL = "Показать в папке";
export const DOWNLOAD_REMOVE = "Удалить из истории";
export const DOWNLOAD_BY_AGENT = "Модель";
export const DOWNLOADS_LOAD_FAILED = "Не удалось загрузить историю загрузок.";
export const DOWNLOAD_ACTION_FAILED = "Не удалось выполнить действие с файлом.";

export const DOWNLOAD_STATE_LABELS: Record<BrowserDownloadState, string> = {
  done: "Загружено",
  failed: "Ошибка",
  cancelled: "Отменено",
};

export function downloadCountLabel(count: number): string {
  return plural(count, "файл", "файла", "файлов");
}

export function downloadSize(bytes: number): string {
  return formatBytes(bytes);
}

export function downloadTime(at: number): string {
  return new Date(at).toLocaleString("ru-RU", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
