import { AppErrorCode, type FolderGrantDto } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import { plural } from "../connections/connectionPresentation";
import { formatLastUsed } from "../tools/toolPresentation";

export const PAGE_TITLE = "Разрешения";
export const FOLDERS_TAB = "Папки";
export const ALWAYS_SECTION = "Постоянные";
export const DIALOG_SECTION = "В диалогах";
export const REVOKE = "Отозвать";
export const REVOKE_TITLE = "Отозвать разрешение";
export const EMPTY_TITLE = "Разрешений нет";
export const EMPTY_DESCRIPTION =
  "Разрешение на папку появляется после ответа «Разрешить всегда» или «Разрешить для этого диалога».";
export const NOTHING_SELECTED_TITLE = "Разрешение не выбрано";
export const NOTHING_SELECTED_DESCRIPTION = "Выберите папку в списке.";
export const NOTHING_FOUND = "Ничего не найдено по текущему запросу.";
export const LOAD_ERROR = "Не удалось загрузить разрешения. Попробуйте ещё раз.";
export const REVOKE_ERROR = "Не удалось отозвать разрешение. Попробуйте ещё раз.";
export const ALREADY_REVOKED = "Разрешение уже отозвано.";
export const PROCESS_TOOLS_NOTE =
  "Разрешения на папки не ограничивают «code.run» и «shell.run»: процессы работают с правами пользователя, каждый запуск подтверждается отдельно.";

export function grantCountLabel(count: number): string {
  return plural(count, "папка", "папки", "папок");
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

export function grantedLabel(grant: FolderGrantDto): string {
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
