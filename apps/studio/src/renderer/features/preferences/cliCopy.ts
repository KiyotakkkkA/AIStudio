import { isIpcError } from "@zvs/ipc";
import { AppErrorCode, type CliTheme } from "@zvs/shared";

export const CLI_SETTING_ID = "cli.install";
export const CLI_TITLE = "Консольная версия";
export const CLI_COMMAND = "aistudio";
export const CLI_INSTALL = "Установить";
export const CLI_UNINSTALL = "Удалить";
export const CLI_CHECKING = "Проверка…";
export const CLI_INSTALLED = "Установлена";
export const CLI_NOT_INSTALLED = "Не установлена";
export const CLI_NOT_BUNDLED = "Не входит в эту сборку приложения.";
export const CLI_NEW_TERMINAL = "Откройте новое окно терминала, чтобы команда стала доступна.";
export const CLI_REMOVED = "Команда удалена из переменной Path.";
export const CLI_INSTALL_FAILED = "Не удалось установить консольную версию.";
export const CLI_UNINSTALL_FAILED = "Не удалось удалить консольную версию.";
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
export const CLI_THEME_PREVIEW_USER = "Вы";
export const CLI_THEME_PREVIEW_ASSISTANT = "Ассистент";
export const CLI_THEME_PREVIEW_TOOL = "готово";

export function cliFailureCopy(error: unknown, installing: boolean): string {
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return CLI_NOT_BUNDLED;
  return installing ? CLI_INSTALL_FAILED : CLI_UNINSTALL_FAILED;
}
