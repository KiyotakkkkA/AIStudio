import { AppErrorCode, type AppGrantDto, type FolderGrantDto } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import { plural } from "../connections/connectionPresentation";
import { formatLastUsed } from "../tools/toolPresentation";

export const PAGE_TITLE = "Разрешения";
export const FOLDERS_TAB = "Папки";
export const APPS_TAB = "Приложения";
export const APPS_TAB_PARAM = "apps";
export const APPS_SEARCH_LABEL = "Поиск приложений";
export const APPS_SEARCH_PLACEHOLDER = "Поиск приложений…";
export const APPS_EMPTY_TITLE = "Разрешений для приложений нет";
export const APPS_EMPTY_DESCRIPTION =
  "Разрешение появляется после ответа «Разрешить всегда» или «Разрешить для этого диалога» на запрос доступа к приложению.";
export const APPS_NOTHING_SELECTED = "Выберите приложение в списке.";
export const APPS_LOAD_ERROR = "Не удалось загрузить разрешения приложений. Попробуйте ещё раз.";
export const APP_PATH_UNKNOWN = "Путь не определён";
export const ALWAYS_SECTION = "Постоянные";
export const DIALOG_SECTION = "В диалогах";
export const REVOKE = "Отозвать";
export const REVOKE_TITLE = "Отозвать разрешение";
export const EMPTY_TITLE = "Разрешений нет";
export const EMPTY_DESCRIPTION =
  "Разрешение на папку появляется после ответа «Разрешить всегда» или «Разрешить для этого диалога».";
export const NOTHING_SELECTED_TITLE = "Разрешение не выбрано";
export const NOTHING_SELECTED_DESCRIPTION = "Выберите папку в списке.";
export const NOTHING_FOUND = "Ничего не найдено.";
export const LOAD_ERROR = "Не удалось загрузить разрешения. Попробуйте ещё раз.";
export const REVOKE_ERROR = "Не удалось отозвать разрешение. Попробуйте ещё раз.";
export const ALREADY_REVOKED = "Разрешение уже отозвано.";

export function grantCountLabel(count: number): string {
  return plural(count, "папка", "папки", "папок");
}

export function appCountLabel(count: number): string {
  return plural(count, "приложение", "приложения", "приложений");
}

export function appScopeLabel(grant: AppGrantDto): string {
  return grant.conversation === null ? "Во всех диалогах" : `Диалог «${grant.conversation.title}»`;
}

export function appLastUsedLabel(grant: AppGrantDto, now: number): string {
  return grant.lastUsedAt === null
    ? "Не использовалось"
    : `Использовано ${formatLastUsed(grant.lastUsedAt, now)}`;
}

export function appRevokeConsequence(grant: AppGrantDto): string {
  return grant.conversation === null
    ? `Агент снова запросит доступ к ${grant.app} в любом диалоге.`
    : `Агент снова запросит доступ к ${grant.app} в диалоге «${grant.conversation.title}».`;
}

export function chatsLabel(count: number): string {
  return plural(count, "диалог", "диалога", "диалогов");
}

export function scopeLabel(grant: FolderGrantDto): string {
  return grant.conversation === null ? "Постоянное" : `Диалог «${grant.conversation.title}»`;
}

export function lastUsedLabel(grant: FolderGrantDto, now: number): string {
  return grant.lastUsedAt === null
    ? "Не использовалось"
    : `Использовано ${formatLastUsed(grant.lastUsedAt, now)}`;
}

export function boundLabel(grant: FolderGrantDto): string {
  const count = grant.boundConversations.length;
  return count === 0 ? "Нет привязанных диалогов" : `Рабочая папка: ${chatsLabel(count)}`;
}

export function grantedLabel(grant: Pick<FolderGrantDto, "grantedAt">): string {
  return new Date(grant.grantedAt).toLocaleString("ru-RU", {
    day: "numeric",
    month: "long",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function revokeConsequence(grant: FolderGrantDto): string {
  const bound = grant.boundConversations.length;
  const kept =
    bound === 0
      ? ""
      : ` ${bound === 1 ? "Привязанный диалог сохранит" : "Привязанные диалоги сохранят"} рабочую папку, но следующий доступ к ней потребует подтверждения.`;
  return `Доступ к папке ${grant.path} будет закрыт.${kept}`;
}

export function revokeErrorCopy(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.NOT_FOUND) return ALREADY_REVOKED;
  return REVOKE_ERROR;
}
