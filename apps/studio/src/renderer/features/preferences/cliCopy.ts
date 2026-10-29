import { isIpcError } from "@zvs/ipc";
import { AppErrorCode } from "@zvs/shared";

export const CLI_SETTING_ID = "cli.install";
export const CLI_TITLE = "Консольная версия";
export const CLI_DESCRIPTION =
  "Команда для работы с диалогами из терминала. Запустите её в папке проекта: диалоги сессии получают доступ к этой папке.";
export const CLI_INSTALL = "Установить";
export const CLI_UNINSTALL = "Удалить";
export const CLI_CHECKING = "Проверка…";
export const CLI_INSTALLED = "Установлена";
export const CLI_NOT_INSTALLED = "Не установлена";
export const CLI_NOT_BUNDLED = "Не входит в эту сборку приложения.";
export const CLI_USAGE = "Запуск в терминале";
export const CLI_TRAY_NOTE =
  "При установке включается значок в области уведомлений: консольная версия работает, пока приложение запущено.";
export const CLI_NEW_TERMINAL = "Откройте новое окно терминала, чтобы команда стала доступна.";
export const CLI_REMOVED = "Команда удалена из переменной Path.";
export const CLI_INSTALL_FAILED = "Не удалось установить консольную версию.";
export const CLI_UNINSTALL_FAILED = "Не удалось удалить консольную версию.";
export const CLI_STATUS_FAILED = "Не удалось проверить установку консольной версии.";

export function cliFailureCopy(error: unknown, installing: boolean): string {
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return CLI_NOT_BUNDLED;
  return installing ? CLI_INSTALL_FAILED : CLI_UNINSTALL_FAILED;
}
