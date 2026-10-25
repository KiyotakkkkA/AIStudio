import {
  AppErrorCode,
  type SkillDto,
  type SkillIssueDto,
  type SkillIssueRule,
  type SkillSource,
  type SkillUpdateDto,
} from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import type { StatusTone } from "../../ui/atoms/statusTone";
import { plural } from "../connections/connectionPresentation";

export { formatBytes as formatSize } from "../tasks/runPresentation";

const UNKNOWN_SKILL_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";
export const SKILL_INVALID_BANNER = "Навык не прошёл проверку. Исправьте отмеченные поля.";

export const SOURCE_LABELS: Record<SkillSource, string> = {
  local: "локальный",
  imported: "импортирован",
  bundled: "встроенный",
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
  readFirst: "Обязательные файлы",
  requires: "Файлы перед запуском скриптов",
  files: "Файлы",
  removeFiles: "Файлы",
  path: "Папка навыка",
};

const RULE_COPY: Record<SkillIssueRule, string> = {
  required: "Обязательное поле.",
  pattern: "Строчная латиница, цифры и одиночные дефисы, начиная с буквы.",
  tooLong: "Слишком длинное значение.",
  duplicate: "Навык с таким идентификатором уже есть.",
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

export function issueLine(issue: SkillIssueDto): string {
  const field = issueField(issue);
  const label = FIELD_LABELS[field] ?? field;
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

export function skillTone(skill: SkillDto): StatusTone {
  if (skill.status === "invalid") return "err";
  return skill.enabled ? "ok" : "idle";
}

export function skillLine(skill: SkillDto): string {
  if (skill.status === "invalid") return `Ошибки: ${issuesWord(skill.issues.length)}`;
  if (!skill.enabled) return "Выключен";
  return skill.fileCount > 0 ? filesWord(skill.fileCount) : "Без файлов";
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

export const UPDATE_CONFIRM_LABEL = "Обновить";
export const UPDATE_SKILL_TITLE = "Обновить навык?";
export const UPDATE_ALL_TITLE = "Обновить встроенные навыки?";
export const UPDATE_MODIFIED_WARNING =
  "Файлы навыка будут заменены версией из приложения. Изменения, внесённые в навык, будут потеряны.";
export const UPDATE_GUARD = "Сохраните или отмените изменения перед обновлением.";

export function updateLabel(update: SkillUpdateDto): string {
  return `Обновить до версии ${update.version}`;
}

export function updateAllLabel(count: number): string {
  return `Обновить всё: ${skillsCount(count)}`;
}

export function updateAllWarning(modified: number): string {
  return `Навыки будут заменены версиями из приложения. Изменения в ${plural(modified, "навыке", "навыках", "навыках")} будут потеряны.`;
}

export function updatedNotice(name: string, version: string | null): string {
  return version === null
    ? `Навык «${name}» обновлён`
    : `Навык «${name}» обновлён до версии ${version}`;
}

export function updatedAllNotice(count: number): string {
  return `Обновлено: ${skillsCount(count)}`;
}

export function versionLine(skill: SkillDto): string | null {
  const parts: string[] = [];
  if (skill.version !== null) parts.push(`Версия ${skill.version}`);
  if (skill.author !== null) parts.push(`Автор: ${skill.author}`);
  return parts.length > 0 ? parts.join(" · ") : null;
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
