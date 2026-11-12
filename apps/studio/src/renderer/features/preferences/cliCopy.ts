import { isIpcError } from "@zvs/ipc";
import { AppErrorCode, type CliConnectionDto, type CliTheme } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import { EXEC_MODE_COPY } from "../execPolicy/execPolicyCopy";
import type { ToggleRowCopy } from "./preferencesPresentation";

export const CLI_SETTING_ID = "cli.install";
export const CLI_TITLE = "Консольная версия";
export const CLI_COMMAND = "aistudio";
export const CLI_INSTALL = "Установить";
export const CLI_UNINSTALL = "Удалить";
export const CLI_CHECKING = "Проверка…";
export const CLI_INSTALLED = "Установлена";
export const CLI_NOT_INSTALLED = "Не установлена";
export const CLI_NOT_BUNDLED = "Не входит в эту сборку приложения.";
export const CLI_NEW_TERMINAL = "Команда aistudio доступна в новых окнах терминала.";
export const CLI_REMOVED = "Команда удалена из переменной Path.";
export const CLI_INSTALL_FAILED = "Не удалось установить консольную версию.";
const CLI_UNINSTALL_FAILED = "Не удалось удалить консольную версию.";
export const CLI_STATUS_FAILED = "Не удалось проверить установку консольной версии.";
export const CLI_THEME_SETTING_ID = "cli.theme";
export const CLI_THEME_LABEL = "Тема оформления";
export const CLI_THEME_HINT = "Цвета консольной версии. Тему также можно сменить командой /theme.";
export const CLI_THEME_LOAD_FAILED = "Не удалось загрузить тему консольной версии.";
export const CLI_THEME_SAVE_FAILED = "Не удалось сохранить тему консольной версии.";
export const CLI_THEME_NAMES: Record<CliTheme, string> = {
  ember: "Янтарь",
  graphite: "Графит",
  moss: "Мох",
  dusk: "Сумерки",
  frost: "Иней",
};
export const CLI_THEME_PREVIEW_PROMPT = "Сообщение…";

export function cliFailureCopy(error: unknown, installing: boolean): string {
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return CLI_NOT_BUNDLED;
  return installing ? CLI_INSTALL_FAILED : CLI_UNINSTALL_FAILED;
}

export const CLI_CONFIRM_ROW: ToggleRowCopy = {
  key: "cli.confirmLoosening",
  label: "Понижение режима проверки доступа",
  hint: `Без подтверждения из консоли доступен только режим «${EXEC_MODE_COPY["auto-bypass"].label}».`,
};
export const CLI_CONNECTIONS_SETTING_ID = "cli.connections";
export const CLI_CONNECTIONS_LABEL = "Подключения консоли";
export const CLI_CONNECTIONS_EMPTY = "Активных подключений нет.";
export const CLI_CONNECTIONS_HINT = "Процессы, подключённые к приложению через консоль.";
export const CLI_CONNECTIONS_FAILED = "Не удалось загрузить сессии консоли.";
export const CLI_DISCONNECT = "Отключить";
export const CLI_DISCONNECT_FAILED = "Не удалось отключить консоль.";
export const CLI_CLIENT_RECOGNISED = "Консольная версия";
export const CLI_CLIENT_UNKNOWN = "Неизвестный процесс";
export const CLI_CLIENT_NO_IMAGE = "путь не определён";

export function cliDialogsWord(count: number): string {
  return plural(count, "диалог", "диалога", "диалогов");
}

export function cliClientLine(connection: CliConnectionDto): string {
  const name = connection.recognised ? CLI_CLIENT_RECOGNISED : CLI_CLIENT_UNKNOWN;
  const pid = connection.pid === null ? "" : ` · PID ${String(connection.pid)}`;
  return `${name}${pid} · ${cliDialogsWord(connection.dialogs)}`;
}
