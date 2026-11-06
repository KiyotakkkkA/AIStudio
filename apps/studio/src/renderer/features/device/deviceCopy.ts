import { ACCESS_ANSWERS } from "../computer/computerCopy";
import type { AppGrantDto } from "@zvs/shared";

export const DEVICE_TITLE = "Устройство";
export const DEVICE_HINT = "Управление устройством и доступ к приложениям.";
export const DEVICE_ENABLED_ID = "computer.enabled";
export const DEVICE_ENABLED_LABEL = "Управление устройством";
export const DEVICE_ENABLED_HINT = "Управление приложениями с помощью команды /computer-use.";
export const APP_PERMISSIONS_ID = "computer.permissions";
export const APP_PERMISSIONS_TITLE = "Разрешения приложений";
export const APP_PERMISSIONS_HINT = "Постоянный доступ и доступ в отдельных диалогах.";
export const APP_PERMISSIONS_EMPTY = "Разрешений для приложений нет.";
export const APP_LOAD_FAILED = "Не удалось загрузить разрешения приложений.";
export const APP_ADD_FAILED = "Не удалось добавить разрешение. Проверьте выбранный файл и диалоги.";
export const APP_PICK_FAILED = "Не удалось открыть окно выбора приложения.";
export const APP_SCOPE_OPTIONS = ACCESS_ANSWERS.filter(
  (answer) => answer.answer === "always" || answer.answer === "conversation",
).map((answer) => ({ value: answer.answer, label: answer.label }));
export const APP_DIALOGS_EMPTY = "Диалогов нет.";
export const APP_DETAILS = "Подробнее";
export const APP_ADD = "Добавить";
export const APP_CANCEL = "Отмена";
export const APP_SAVE = "Добавить разрешение";
export const APP_DIALOGS = "Разрешённые диалоги";
export const APP_OPEN_DIALOG = "Открыть диалог";
export const APP_PATH_UNKNOWN = "Путь не определён";
export const ALWAYS_SECTION = "Постоянные";
export const DIALOG_SECTION = "В диалогах";
export const REVOKE = "Отозвать";
export const REVOKE_TITLE = "Отозвать разрешение";
export function appRevokeConsequence(grant: AppGrantDto): string {
  return grant.conversation === null
    ? `Доступ к ${grant.app} во всех диалогах будет отозван. Разрешения отдельных диалогов сохранятся.`
    : `Доступ к ${grant.app} в диалоге «${grant.conversation.title}» будет отозван. Постоянное разрешение, если оно выдано, сохранится.`;
}
