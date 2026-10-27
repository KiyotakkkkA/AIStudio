import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import { plural } from "../features/connections/connectionPresentation";
import { formatBytes } from "../features/downloads/downloadPresentation";

export const VIEWER_LABEL = "Просмотр документа";
export const RESIZE_VIEWER = "Изменить ширину просмотра";
export const BACK_TO_CHAT = "Назад к диалогу";
export const PREVIOUS_PAGE = "Предыдущая страница";
export const NEXT_PAGE = "Следующая страница";
export const PAGE_INPUT = "Номер страницы";
export const ZOOM_LABEL = "Масштаб";
export const ZOOM_IN = "Увеличить";
export const ZOOM_OUT = "Уменьшить";
export const FIT_WIDTH = "По ширине";
export const SEARCH = "Поиск по документу";
export const SEARCH_PLACEHOLDER = "Найти в документе";
export const SEARCH_PREVIOUS = "Предыдущее совпадение";
export const SEARCH_NEXT = "Следующее совпадение";
export const SEARCH_CLOSE = "Закрыть поиск";
export const SEARCH_EMPTY = "Нет совпадений";
export const DOWNLOAD = "Скачать";
export const REVEAL = "Показать в папке";
export const EXPAND = "Развернуть";
export const COLLAPSE = "Свернуть";
export const CLOSE = "Закрыть";
export const ASK_ABOUT = "Спросить об этом";
export const CLEAR_SELECTION = "Сбросить";
export const SAVED = "Документ сохранён";

export const LOADING = "Загрузка документа…";
export const RENDERING = "Подготовка страниц…";

export const CHIP_APPROXIMATE = "приближённое отображение";
export const CHIP_APPROXIMATE_HINT =
  "Вёрстка, шрифты и разрывы страниц могут отличаться от оригинала.";
export const PPTX_NOTE = "Для презентации показан только текст слайдов.";

export const MISSING_TITLE = "Файл не найден";
export const MISSING_HINT = "Файл перемещён или удалён. Проверьте исходную папку.";
export const UNSUPPORTED_TITLE = "Формат не поддерживается";
export const UNSUPPORTED_HINT = "Этот файл нельзя показать в просмотрщике.";
export const FORBIDDEN_TITLE = "Доступ к файлу закрыт";
export const TOO_LARGE_TITLE = "Файл слишком большой";
export const FAILED_TITLE = "Не удалось открыть документ";
export const FAILED_HINT = "Повторите попытку позже.";
export const RENDER_FAILED = "Не удалось отобразить документ.";
export const EMPTY_DOCUMENT = "Документ пуст.";
export const SAVE_FAILED = "Не удалось сохранить документ.";
export const REVEAL_FAILED = "Не удалось показать файл в папке.";

export const SHEETS_LABEL = "Листы книги";
export const SLIDE = "Слайд";

export interface ViewerProblem {
  readonly kind: "missing" | "unsupported" | "forbidden" | "tooLarge" | "failed";
  readonly title: string;
  readonly hint: string;
}

function hostHint(error: { readonly message: string }, fallback: string): string {
  return error.message.length > 0 ? error.message : fallback;
}

export function viewerProblem(error: unknown): ViewerProblem {
  if (!isIpcError(error)) return { kind: "failed", title: FAILED_TITLE, hint: FAILED_HINT };
  switch (error.code) {
    case AppErrorCode.NOT_FOUND:
      return { kind: "missing", title: MISSING_TITLE, hint: MISSING_HINT };
    case AppErrorCode.UNSUPPORTED_FORMAT:
      return {
        kind: "unsupported",
        title: UNSUPPORTED_TITLE,
        hint: hostHint(error, UNSUPPORTED_HINT),
      };
    case AppErrorCode.PERMISSION_DENIED:
      return { kind: "forbidden", title: FORBIDDEN_TITLE, hint: hostHint(error, FAILED_HINT) };
    case AppErrorCode.LIMIT_EXCEEDED:
      return { kind: "tooLarge", title: TOO_LARGE_TITLE, hint: hostHint(error, FAILED_HINT) };
    default:
      return { kind: "failed", title: FAILED_TITLE, hint: FAILED_HINT };
  }
}

export function actionErrorCopy(error: unknown, fallback: string): string {
  if (isIpcError(error) && error.code === AppErrorCode.NOT_FOUND && error.message.length > 0)
    return error.message;
  return fallback;
}

export function pagesLabel(count: number): string {
  return plural(count, "страница", "страницы", "страниц");
}

export function documentFacts(pageCount: number | null, sizeBytes: number): string {
  const size = formatBytes(sizeBytes);
  return pageCount === null ? size : `${pagesLabel(pageCount)} · ${size}`;
}

export function selectionLabel(length: number): string {
  return `Выделено ${plural(length, "символ", "символа", "символов")}`;
}

export function matchesLabel(current: number, total: number): string {
  return `${current.toLocaleString("ru-RU")} из ${total.toLocaleString("ru-RU")}`;
}

export function zoomLabel(zoom: number): string {
  return `${Math.round(zoom * 100).toLocaleString("ru-RU")}%`;
}

export function pageLocation(page: number | null): string | null {
  return page === null ? null : `стр. ${page.toLocaleString("ru-RU")}`;
}
