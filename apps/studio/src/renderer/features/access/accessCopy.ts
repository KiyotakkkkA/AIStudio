export const SCOPE_ONCE = "Один раз";
export const SCOPE_CONVERSATION = "В этом диалоге";
export const SCOPE_ALWAYS = "Всегда";

export const ACCESS_TABS_LABEL = "Вид доступа";
export const TAB_FOLDERS = "Папки";
export const TAB_SITES = "Сайты";
export const TAB_APPS = "Приложения";
export const TAB_RULES = "Правила";

export const MODE_LABEL = "Режим";
export const JUDGE_LABEL = "Модель-арбитр";

export const SITES_EMPTY = "Исключений для сайтов нет.";
export const SITE_DEFAULTS = "По умолчанию";
export const SITE_REVOKE = "Удалить исключение";
export const APPS_EMPTY = "Разрешений для приложений нет.";
export const APP_REVOKE = "Отозвать";
export const APP_REVOKE_TITLE = "Отозвать разрешение";

export const LIST_LOADING = "Загрузка…";
export const CHANGE_FAILED = "Не удалось изменить доступ. Попробуйте ещё раз.";

export function appScope(conversation: string | null): string {
  return conversation === null ? SCOPE_ALWAYS : `${SCOPE_CONVERSATION}: «${conversation}»`;
}
