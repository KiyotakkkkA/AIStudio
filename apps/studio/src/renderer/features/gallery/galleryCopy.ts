import { isIpcError } from "@zvs/ipc";
import { AppErrorCode, type GalleryItemDto, type GalleryKind } from "@zvs/shared";
import { formatBytes } from "../downloads/downloadPresentation";
import { plural } from "../connections/connectionPresentation";

export const GALLERY_TITLE = "Галерея";
export const GALLERY_SUBTITLE = "Артефакты и дашборды, созданные в ходе диалогов";
export const GALLERY_SEARCH = "Поиск по названию или диалогу";
export const GALLERY_SORT = "Порядок";
export const GALLERY_EMPTY_TITLE = "Файлов нет";
export const GALLERY_EMPTY_DESCRIPTION = "Галерея пуста";
export const GALLERY_NOTHING_FOUND = "Ничего не найдено";
export const GALLERY_NOTHING_FOUND_DESCRIPTION = "Измените запрос.";
export const GALLERY_LOAD_FAILED = "Не удалось загрузить галерею";
export const GALLERY_RETRY = "Повторить";
export const GALLERY_ACTIONS = "Действия с файлом";
export const GALLERY_OPEN = "Открыть";
export const GALLERY_REVEAL = "Показать в папке";
export const GALLERY_REFRESH = "Обновить данные";
export const GALLERY_AUTO_REFRESH = "Автообновление";
export const GALLERY_AUTO_REFRESH_DEFAULT = "Как задано при создании";
export const GALLERY_APPROVE = "Подтвердить скрипты…";
export const GALLERY_REMOVE = "Удалить";
export const GALLERY_REMOVE_TITLE = "Удалить файл?";
export const GALLERY_APPROVE_TITLE = "Скрипты получения данных";
export const GALLERY_APPROVE_CONFIRM = "Разрешить обновление";
export const GALLERY_APPROVE_HINT =
  "Дашборды обновляются автоматически, подтверждение потребуется только в случае обновления скрипта получения данных";
export const GALLERY_SCRIPT_TRUNCATED = "Показано начало скрипта.";
export const GALLERY_NEEDS_APPROVAL = "Ждёт подтверждения";
export const GALLERY_OUTSIDE_DIALOG = "Вне диалога";
export const GALLERY_UNKNOWN_ERROR = "Не удалось выполнить действие. Попробуйте ещё раз.";

export type GallerySort = "newest" | "oldest" | "name";

export const GALLERY_SORTS: readonly { readonly value: GallerySort; readonly label: string }[] = [
  { value: "newest", label: "Сначала новые" },
  { value: "oldest", label: "Сначала старые" },
  { value: "name", label: "По названию" },
];

export const GALLERY_KINDS: readonly { readonly kind: GalleryKind; readonly label: string }[] = [
  { kind: "dashboard", label: "Дашборды" },
  { kind: "document", label: "Документы" },
  { kind: "table", label: "Таблицы" },
  { kind: "image", label: "Изображения" },
  { kind: "other", label: "Прочее" },
];

export const GALLERY_INTERVALS: readonly number[] = [0, 15, 60, 300, 900, 3600];

export function intervalLabel(seconds: number): string {
  if (seconds === 0) return "Вручную";
  if (seconds === 60) return "Каждую минуту";
  if (seconds === 3600) return "Каждый час";
  if (seconds % 3600 === 0) return `Каждые ${seconds / 3600} ч`;
  if (seconds % 60 === 0) return `Каждые ${seconds / 60} мин`;
  return `Каждые ${seconds} с`;
}

export function filesCount(count: number): string {
  return `${count.toLocaleString("ru-RU")} ${plural(count, "файл", "файла", "файлов")}`;
}

export function removeQuestion(name: string, dashboard: boolean): string {
  return dashboard
    ? `Дашборд «${name}», его сохранённая спецификация и разрешения будут удалены. Действие нельзя отменить.`
    : `Файл «${name}» будет удалён с диска. Действие нельзя отменить.`;
}

export function modifiedLabel(timestamp: number, now: number): string {
  const date = new Date(timestamp);
  const today = new Date(now);
  const sameDay = date.toDateString() === today.toDateString();
  return sameDay
    ? date.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })
    : date.toLocaleDateString("ru-RU", { day: "numeric", month: "short", year: "numeric" });
}

export function itemLine(item: GalleryItemDto, now: number): string {
  return [
    item.format.toUpperCase(),
    formatBytes(item.sizeBytes),
    modifiedLabel(item.modifiedAt, now),
  ].join(" · ");
}

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.NOT_FOUND,
  AppErrorCode.PERMISSION_DENIED,
  AppErrorCode.CONFLICT,
  AppErrorCode.VALIDATION_FAILED,
  AppErrorCode.UNKNOWN,
]);

export function galleryErrorCopy(error: unknown): string {
  if (isIpcError(error) && HOST_WORDED.has(error.code) && error.message.length > 0) {
    return error.message;
  }
  return GALLERY_UNKNOWN_ERROR;
}
