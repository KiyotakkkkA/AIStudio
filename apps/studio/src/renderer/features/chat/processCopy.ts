import { ProcessResultDto, type CodeLanguage, type Json, type ProcessPlanDto } from "@zvs/shared";

export const PROCESS_TOOL_CODE = "code.run";
export const PROCESS_TOOL_SHELL = "shell.run";

export const LANGUAGE_LABEL: Record<CodeLanguage, string> = {
  python: "Python",
  javascript: "JavaScript",
  powershell: "PowerShell",
};

export const CODE_LEAD =
  "Модель предлагает запустить программу. Процесс работает с правами пользователя, разрешения на папки его не ограничивают.";
export const SHELL_LEAD =
  "Модель предлагает выполнить команду. Процесс работает с правами пользователя, разрешения на папки его не ограничивают.";
export const RUN_PROCESS = "Запустить";
export const REJECT_PROCESS = "Отклонить";
export const LANGUAGE_TITLE = "Язык";
export const PROGRAM_TITLE = "Программа";
export const COMMAND_TITLE = "Команда";
export const FOLDER_TITLE = "Папка";
export const PACKAGES_TITLE = "Пакеты npm";
export const SOURCE_TITLE = "Текст программы";
export const ARGS_TITLE = "Аргументы";
export const TIMED_OUT = "Остановлен по истечении времени";
export const NO_EXIT_CODE = "Код завершения неизвестен";

export function processResult(meta: Json): ProcessResultDto | null {
  const parsed = ProcessResultDto.safeParse(meta);
  return parsed.success ? parsed.data : null;
}

function quoted(argument: string): string {
  if (argument.length > 0 && !/[\s"]/.test(argument)) return argument;
  return `"${argument.replaceAll('"', '\\"')}"`;
}

export function commandLine(argv: readonly string[]): string {
  return argv.map(quoted).join(" ");
}

export function processLead(plan: ProcessPlanDto): string {
  return plan.language === undefined ? SHELL_LEAD : CODE_LEAD;
}

export function durationLabel(milliseconds: number): string {
  return milliseconds < 1000
    ? `${milliseconds.toLocaleString("ru-RU")} мс`
    : `${(milliseconds / 1000).toLocaleString("ru-RU", { maximumFractionDigits: 1 })} с`;
}

export function exitLabel(result: ProcessResultDto): string {
  if (result.timedOut) return TIMED_OUT;
  if (result.exitCode === null) return NO_EXIT_CODE;
  return `Код завершения: ${String(result.exitCode)}`;
}

export function processSummary(plan: ProcessPlanDto): string {
  if (plan.language !== undefined) {
    const firstLine = (plan.source ?? "").split("\n").find((line) => line.trim().length > 0) ?? "";
    const preview = firstLine.length > 60 ? `${firstLine.slice(0, 60)}…` : firstLine;
    return `${LANGUAGE_LABEL[plan.language]} · ${preview.trim()}`;
  }
  const line = commandLine(plan.argv);
  return line.length > 80 ? `${line.slice(0, 80)}…` : line;
}
