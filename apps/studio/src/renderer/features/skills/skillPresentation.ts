import {
  AppErrorCode,
  type McpServerDto,
  type SkillDto,
  type SkillIssueDto,
  type SkillIssueRule,
  type SkillMatchMethod,
  type SkillSource,
  type ToolDto,
} from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import type { StatusTone } from "../../ui/atoms/statusTone";
import { plural, serverState, STATE_LABELS } from "../connections/connectionPresentation";

export { formatBytes as formatSize } from "../tasks/runPresentation";

const UNKNOWN_SKILL_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";
export const SKILL_INVALID_BANNER = "Навык не прошёл проверку. Исправьте отмеченные поля.";

export const SOURCE_LABELS: Record<SkillSource, string> = {
  local: "локальный",
  imported: "импортирован",
  bundled: "встроенный",
};

export const METHOD_LABELS: Record<SkillMatchMethod, string> = {
  embedding: "по смыслу",
  lexical: "по совпадению слов",
};

const FILE_DIR_LABELS = {
  references: "Справочные материалы",
  scripts: "Скрипты",
  assets: "Ресурсы",
} as const;

export type SkillFileDir = keyof typeof FILE_DIR_LABELS;

export const FILE_DIRS = Object.keys(FILE_DIR_LABELS) as SkillFileDir[];

const FIELD_LABELS: Record<string, string> = {
  key: "Идентификатор",
  name: "Название",
  description: "Описание",
  instructions: "Инструкции",
  allowedTools: "Разрешённые инструменты",
  files: "Файлы",
  removeFiles: "Файлы",
  path: "Папка навыка",
};

const RULE_COPY: Record<SkillIssueRule, string> = {
  required: "Обязательное поле.",
  pattern: "Строчная латиница, цифры и одиночные дефисы, начиная с буквы.",
  tooLong: "Слишком длинное значение.",
  duplicate: "Навык с таким идентификатором уже есть.",
  unknownTool: "Инструмент не найден в реестре.",
  toolOff: "Инструмент запрещён на странице «Инструменты».",
  toolDisabled: "Инструмент выключен на странице «Инструменты».",
  toolUnavailable: "Источник инструмента недоступен.",
  path: "Недопустимый путь: файлы лежат в references/, scripts/ или assets/.",
  fileTooLarge: "Файл больше 1 МБ.",
  skillTooLarge: "Файлы навыка вместе больше 10 МБ.",
  tooManyFiles: "Больше 200 файлов.",
  encoding: "Содержимое файла не удалось прочитать.",
  manifest: "Файл SKILL.md повреждён или не совпадает с идентификатором.",
  missing: "Папка или файл навыка не найдены на диске.",
};

export function issueCopy(rule: SkillIssueRule): string {
  return RULE_COPY[rule];
}

export function issueField(issue: SkillIssueDto): string {
  const [head = issue.field] = issue.field.split(".");
  return head;
}

export function issueLine(issue: SkillIssueDto, allowedTools: readonly string[] = []): string {
  const field = issueField(issue);
  const label = FIELD_LABELS[field] ?? field;
  if (field === "allowedTools") {
    const index = Number(issue.field.split(".")[1]);
    const key = Number.isInteger(index) ? allowedTools[index] : undefined;
    return key === undefined
      ? `${label}: ${issueCopy(issue.rule)}`
      : `${key}: ${issueCopy(issue.rule)}`;
  }
  return `${label}: ${issueCopy(issue.rule)}`;
}

const COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.VALIDATION_FAILED]: SKILL_INVALID_BANNER,
  [AppErrorCode.NOT_FOUND]: "Навык не найден — возможно, он уже удалён.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для этой операции.",
  [AppErrorCode.LIMIT_EXCEEDED]: "Превышен допустимый размер файлов навыка.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
};

const HOST_WORDED = new Set<AppErrorCode>([AppErrorCode.CONFLICT]);

export function skillErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_SKILL_ERROR;
  if (error.code === AppErrorCode.VALIDATION_FAILED && error.details?.issues === undefined) {
    return error.message.length > 0 ? error.message : SKILL_INVALID_BANNER;
  }
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return COPY[error.code] ?? UNKNOWN_SKILL_ERROR;
}

function isIssue(value: unknown): value is SkillIssueDto {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return typeof record.field === "string" && typeof record.rule === "string";
}

export function skillIssuesOf(error: unknown): SkillIssueDto[] {
  if (!isIpcError(error) || error.code !== AppErrorCode.VALIDATION_FAILED) return [];
  const issues = error.details?.issues;
  if (!Array.isArray(issues)) return [];
  return issues.filter(isIssue).filter((issue) => issue.rule in RULE_COPY);
}

type ToolBlock = "unavailable" | "offline" | "disabled" | "off";

export interface ToolCeiling {
  readonly block: ToolBlock | null;
  readonly reason: string | null;
}

export function toolCeiling(tool: ToolDto, server: McpServerDto | null): ToolCeiling {
  if (!tool.available) {
    if (tool.source === "mcp" && server !== null) {
      const state = serverState(server, null);
      return {
        block: "offline",
        reason: `Сервер «${server.name}» недоступен: ${STATE_LABELS[state].toLowerCase()}.`,
      };
    }
    return {
      block: "unavailable",
      reason:
        tool.source === "mcp"
          ? "MCP-сервер инструмента удалён."
          : "Источник больше не предоставляет инструмент.",
    };
  }
  if (!tool.enabled) {
    return { block: "disabled", reason: "Выключен на странице «Инструменты»." };
  }
  if (tool.tier === "off") {
    return { block: "off", reason: "Запрещён на странице «Инструменты» (off)." };
  }
  return { block: null, reason: null };
}

export function skillTone(skill: SkillDto): StatusTone {
  if (skill.status === "invalid") return "err";
  return skill.enabled ? "ok" : "idle";
}

export function skillLine(skill: SkillDto): string {
  if (skill.status === "invalid") return `Ошибки: ${issuesWord(skill.issues.length)}`;
  if (!skill.enabled) return "Выключен";
  const parts: string[] = [];
  if (skill.allowedTools.length > 0) parts.push(toolCount(skill.allowedTools.length));
  if (skill.fileCount > 0) parts.push(filesWord(skill.fileCount));
  return parts.length > 0 ? parts.join(" · ") : "Без инструментов и файлов";
}

function toolCount(count: number): string {
  return plural(count, "инструмент", "инструмента", "инструментов");
}

function issuesWord(count: number): string {
  return plural(count, "ошибка", "ошибки", "ошибок");
}

function filesWord(count: number): string {
  return plural(count, "файл", "файла", "файлов");
}

export function linesWord(count: number): string {
  return plural(count, "строка", "строки", "строк");
}

export function headerLine(skills: readonly SkillDto[]): string {
  const enabled = skills.filter((skill) => skill.enabled).length;
  return `${skillsCount(skills.length)} · ${plural(enabled, "включён", "включено", "включено")}`;
}

function skillsCount(count: number): string {
  return plural(count, "навык", "навыка", "навыков");
}

export function formatScore(score: number): string {
  return score.toLocaleString("ru-RU", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

const TRANSLIT: Record<string, string> = {
  а: "a",
  б: "b",
  в: "v",
  г: "g",
  д: "d",
  е: "e",
  ё: "e",
  ж: "zh",
  з: "z",
  и: "i",
  й: "y",
  к: "k",
  л: "l",
  м: "m",
  н: "n",
  о: "o",
  п: "p",
  р: "r",
  с: "s",
  т: "t",
  у: "u",
  ф: "f",
  х: "h",
  ц: "ts",
  ч: "ch",
  ш: "sh",
  щ: "sch",
  ъ: "",
  ы: "y",
  ь: "",
  э: "e",
  ю: "yu",
  я: "ya",
};

export function slugify(name: string, max = 64): string {
  const latin = [...name.toLowerCase()].map((char) => TRANSLIT[char] ?? char).join("");
  const slug = latin
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^[^a-z]+/, "")
    .replace(/-+$/, "");
  return slug.slice(0, max).replace(/-+$/, "");
}
