import { AppErrorCode, type FolderApprovalDto } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const WORKING_FOLDER_LABEL = "Рабочая папка";
export const CHOOSE_FOLDER = "Выбрать папку";
export const CHANGE_FOLDER = "Сменить папку";
export const REMOVE_FOLDER = "Убрать папку";
export const FOLDER_ACCESS_MISSING = "Нет доступа";
export const RESTORE_FOLDER_ACCESS = "Предоставить доступ";
export const FOLDER_LOCKED = "Рабочую папку нельзя изменить во время ответа";

export const ALLOW_ONCE = "Разрешить однократно";
export const ALLOW_CONVERSATION = "Разрешить для этого диалога";
export const ALLOW_ALWAYS = "Разрешить всегда";
export const DENY = "Запретить";
export const CANCEL = "Отмена";

export const BIND_TITLE = "Доступ к рабочей папке";
export const SUBFOLDERS_NOTE =
  "Доступ распространяется на все вложенные папки. Допустимые действия определяются на странице «Инструменты».";
export const CREATES_LABEL = "Будут созданы:";

export const FOLDER_ERROR_FALLBACK = "Не удалось изменить рабочую папку. Попробуйте ещё раз.";
export const ANSWER_ERROR_FALLBACK = "Не удалось передать ответ на запрос доступа.";

const ERRORS: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.PERMISSION_DENIED]:
    "Эта папка недоступна: системные папки, корни дисков, сетевые и съёмные диски и данные приложения закрыты для доступа.",
  [AppErrorCode.NOT_FOUND]: "Папка не найдена.",
  [AppErrorCode.VALIDATION_FAILED]: "Выбранный путь не является папкой.",
  [AppErrorCode.CONFLICT]: `${FOLDER_LOCKED}.`,
};

const ANSWER_ERRORS: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.CONFLICT]: "Запрос доступа больше не ожидает ответа.",
  [AppErrorCode.PERMISSION_DENIED]: ERRORS[AppErrorCode.PERMISSION_DENIED],
};

export function folderErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return FOLDER_ERROR_FALLBACK;
  return ERRORS[error.code] ?? FOLDER_ERROR_FALLBACK;
}

export function answerErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return ANSWER_ERROR_FALLBACK;
  return ANSWER_ERRORS[error.code] ?? ANSWER_ERROR_FALLBACK;
}

export function folderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).pop();
  return name === undefined || name.length === 0 ? path : name;
}

export function accessTitle(request: FolderApprovalDto): string {
  return request.folders.length > 1 ? "Доступ к папкам" : "Доступ к папке";
}

export function accessLead(request: FolderApprovalDto): string {
  const action = request.action.trim();
  return action.length > 0
    ? `Инструмент «${request.tool}» запрашивает доступ: ${action}`
    : `Инструмент «${request.tool}» запрашивает доступ.`;
}
