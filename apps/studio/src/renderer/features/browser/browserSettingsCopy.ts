import type { BrowserLinkTarget } from "@zvs/shared";

export const BROWSER_SECTION_HINT = "Встроенный браузер, открытие ссылок и доступ к сайтам.";

export const BROWSER_ENABLED_ID = "browser.enabled";
export const BROWSER_ENABLED_LABEL = "Встроенный браузер";
export const BROWSER_ENABLED_HINT =
  "Просмотр сайтов в приложении и управление браузером с помощью команды /browser-use.";
export const BROWSER_DISABLED_NOTE =
  "Встроенный браузер отключён. Ссылки открываются в системном браузере.";

export const BROWSER_MAIN_TITLE = "Основное";

export const BROWSER_GENERAL_TITLE = "Настройки браузера";

export const WEB_LINKS_ID = "browser.webLinks";
export const WEB_LINKS_LABEL = "Открытие веб-ссылок";
export const WEB_LINKS_HINT = "Браузер для открытия веб-ссылок из диалогов, артефактов и плагинов.";

export const LOCAL_LINKS_ID = "browser.localLinks";
export const LOCAL_LINKS_LABEL = "Открытие локальных адресов";
export const LOCAL_LINKS_HINT =
  "Браузер для открытия локальных веб-приложений по адресам localhost и 127.0.0.1.";

export const LINK_TARGET_OPTIONS: readonly { value: BrowserLinkTarget; label: string }[] = [
  { value: "studio", label: "Встроенный браузер" },
  { value: "system", label: "Системный браузер" },
];

export const FULL_URL_ID = "browser.fullUrl";
export const FULL_URL_LABEL = "Отображение полного адреса";
export const FULL_URL_HINT = "Отображение полного URL страницы в адресной строке.";

export const BROWSER_SAVE_FAILED = "Не удалось сохранить настройки браузера.";
