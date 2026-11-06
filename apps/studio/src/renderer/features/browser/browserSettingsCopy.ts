import type { BrowserLinkTarget } from "@zvs/shared";

export const BROWSER_SECTION_HINT = "Встроенный браузер, открытие ссылок и доступ к сайтам.";

export const BROWSER_ENABLED_ID = "browser.enabled";
export const BROWSER_ENABLED_LABEL = "Встроенный браузер";
export const BROWSER_ENABLED_HINT =
  "Работа с сайтами внутри приложения и управление браузером по команде /browser-use.";
export const BROWSER_DISABLED_NOTE =
  "Браузер скрыт, команда /browser-use недоступна, ссылки открываются в системном браузере.";

export const BROWSER_GENERAL_TITLE = "Основные";

export const WEB_LINKS_ID = "browser.webLinks";
export const WEB_LINKS_LABEL = "Открытие веб-ссылок";
export const WEB_LINKS_HINT = "Ссылки на сайты из диалогов, артефактов и плагинов.";

export const LOCAL_LINKS_ID = "browser.localLinks";
export const LOCAL_LINKS_LABEL = "Открытие локальных адресов";
export const LOCAL_LINKS_HINT = "Адреса localhost и 127.0.0.1, в том числе живые дашборды.";

export const LINK_TARGET_OPTIONS: readonly { value: BrowserLinkTarget; label: string }[] = [
  { value: "studio", label: "Встроенный браузер" },
  { value: "system", label: "Системный браузер" },
];

export const FULL_URL_ID = "browser.fullUrl";
export const FULL_URL_LABEL = "Полный адрес";
export const FULL_URL_HINT = "Полный адрес веб-сайта";

export const BROWSER_SAVE_FAILED = "Не удалось сохранить настройки браузера.";
