export function plural(count: number, one: string, few: string, many: string): string {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} ${many}`;
  if (last === 1) return `${String(count)} ${one}`;
  if (last >= 2 && last <= 4) return `${String(count)} ${few}`;
  return `${String(count)} ${many}`;
}

export const SECURITY_LABELS = {
  https: "HTTPS",
  http: "Подключение не защищено",
  none: "Нет страницы",
  error: "Ошибка загрузки контента",
} as const;

export const SECURITY_HINT =
  "HTTPS означает, что основная страница загружена по HTTPS без принятого исключения для сертификата. Надёжность сайта это не подтверждает.";

export const ACTION_FAILED =
  "Действие не выполнено. Допустимы только адреса HTTP и HTTPS; одновременно открыто не более 30 вкладок.";

export const STATUS_LOADING = "Загрузка…";
export const STATUS_READY = "Готово";
export const PROFILE_LABEL = "Рабочий профиль";
export const PROFILE_FOOTER = "Загрузки только с подтверждением";

export const SITES_TITLE = "Сайты и куки";
export const SITES_HINT =
  "Куки рабочего профиля. Удаление куки может завершить сеанс на сайте — войти можно снова.";
export const SITES_EMPTY = "Куки нет.";
export const SITES_EMPTY_HINT = "Они появятся после посещения сайтов в рабочем профиле.";
export const SITES_NOTHING_FOUND = "Ничего не найдено.";
export const CLEAR_SITE = "Очистить куки";
export const REMOVE_COOKIE = "Удалить";
export const CONFIRM_REMOVAL = "Подтвердить удаление";
export const CANCEL = "Отмена";
export const SESSION_COOKIE = "Сеанс";

export function cookiesWord(count: number): string {
  return plural(count, "куки", "куки", "куки");
}

export function sitesWord(count: number): string {
  return plural(count, "сайт", "сайта", "сайтов");
}

export function confirmTitle(domain: string | null): string {
  return domain === null ? "Удалить эту куки?" : `Удалить куки сайта ${domain}?`;
}

export function expiryLabel(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
