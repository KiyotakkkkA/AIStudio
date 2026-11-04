import {
  AppErrorCode,
  type AppGrantDto,
  type BrowserGrantDto,
  type FolderGrantDto,
} from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import { plural } from "../connections/connectionPresentation";
import { formatLastUsed } from "../tools/toolPresentation";

export const PAGE_TITLE = "Разрешения";
export const FOLDERS_TAB = "Папки";
export const TOOLS_TAB = "Инструменты";
export const TOOLS_TAB_PARAM = "tools";
export const BROWSER_TAB = "Браузер";
export const BROWSER_TAB_PARAM = "browser";
export const APPS_TAB = "Приложения";
export const APPS_TAB_PARAM = "apps";
export const APPS_SEARCH_LABEL = "Поиск приложений";
export const APPS_SEARCH_PLACEHOLDER = "Поиск приложений…";
export const APPS_EMPTY_TITLE = "Разрешений для приложений нет";
export const APPS_EMPTY_DESCRIPTION =
  "Разрешение появляется после ответа «Разрешить всегда» или «Разрешить для этого диалога» на запрос доступа к приложению.";
export const APPS_NOTHING_SELECTED = "Выберите приложение в списке.";
export const APPS_LOAD_ERROR = "Не удалось загрузить разрешения приложений. Попробуйте ещё раз.";
export const APPS_NOTE =
  "Ответ «Только для этого ответа» действует до конца ответа агента и здесь не показывается.";
export const APP_PATH_UNKNOWN = "Путь не определён";
export const SITES_TITLE = "Разрешения на сайтах";
export const SITES_DESCRIPTION = "Действия, которые агент выполняет на сайте без подтверждения.";
export const SITES_EMPTY_TITLE = "Разрешений для сайтов нет";
export const SITES_EMPTY_DESCRIPTION = "Разрешение появляется после ответа «Всегда на этом сайте».";
export const SITE_GRANTED_COLUMN = "Выдано";
export const SITES_LOAD_ERROR = "Не удалось загрузить разрешения сайтов. Попробуйте ещё раз.";

const SITE_ACTIONS: Readonly<Record<string, string>> = {
  "browser.click": "Нажатия",
  "browser.type": "Ввод текста",
};
export const DEFAULTS_TITLE = "Разрешения по умолчанию";
export const DEFAULTS_DESCRIPTION =
  "Уровень, который получает инструмент при первой регистрации. Разрешения известных инструментов не меняются, разрушающие всегда начинают с «off».";
export const DEFAULTS_SOURCE_COLUMN = "Источник";
export const DEFAULTS_TIER_COLUMN = "Разрешение для новых инструментов";
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
export const PROCESS_TOOLS_NOTE =
  "«shell.run» не ограничен разрешениями на папки: каждый запуск подтверждается отдельно.";

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

export function siteCountLabel(count: number): string {
  return plural(count, "сайт", "сайта", "сайтов");
}

export function siteActionLabel(tool: string): string {
  return SITE_ACTIONS[tool] ?? tool;
}

export function siteHost(origin: string): string {
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}

export interface SiteGrantGroup {
  readonly origin: string;
  readonly host: string;
  readonly grants: readonly BrowserGrantDto[];
}

export function groupSiteGrants(grants: readonly BrowserGrantDto[]): SiteGrantGroup[] {
  const groups = new Map<string, BrowserGrantDto[]>();
  for (const grant of grants)
    groups.set(grant.origin, [...(groups.get(grant.origin) ?? []), grant]);
  return [...groups]
    .map(([origin, list]) => ({
      origin,
      host: siteHost(origin),
      grants: list.slice().sort((a, b) => a.tool.localeCompare(b.tool)),
    }))
    .sort((a, b) => a.host.localeCompare(b.host, "ru-RU"));
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

export function siteRevokeConsequence(grant: BrowserGrantDto): string {
  return `«${siteActionLabel(grant.tool)}» на ${siteHost(grant.origin)} снова потребуют подтверждения.`;
}

export function revokeErrorCopy(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.NOT_FOUND) return ALREADY_REVOKED;
  return REVOKE_ERROR;
}
