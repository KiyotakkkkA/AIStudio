import { FileChangesDto, type FileChangeKind, type FileDiffDto, type Json } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const FILE_TOOL_APPLY = "fs.apply";
export const VISIBLE_CHANGES = 3;

export const CHANGE_KIND_LABEL: Record<FileChangeKind, string> = {
  create: "создан",
  modify: "изменён",
  move: "перемещён",
  delete: "удалён",
};

export const CHANGES_TITLE = "Изменения файлов";
export const DIFF_TRUNCATED = "Дифф сокращён";
export const NO_TEXT_CHANGES = "Содержимое не изменилось";
export const APPROVE_CHANGES = "Применить изменения";
export const REJECT_CHANGES = "Отклонить";
export const CHANGES_LEAD = "Модель предлагает изменить файлы. Набор применяется только целиком.";

export function filesWord(count: number): string {
  return plural(count, "файл", "файла", "файлов");
}

export function moreChanges(count: number): string {
  return `Показать ещё ${filesWord(count)}`;
}

export function changesSummary(changes: readonly FileDiffDto[]): string {
  const added = changes.reduce((sum, change) => sum + change.added, 0);
  const removed = changes.reduce((sum, change) => sum + change.removed, 0);
  return `${filesWord(changes.length)} · +${added.toLocaleString("ru-RU")} −${removed.toLocaleString("ru-RU")}`;
}

export function fileChanges(meta: Json): FileDiffDto[] {
  const parsed = FileChangesDto.safeParse(meta);
  return parsed.success ? parsed.data.changes : [];
}

export type DiffLineTone = "added" | "removed" | "hunk" | "context";

export function diffLineTone(line: string): DiffLineTone {
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+")) return "added";
  if (line.startsWith("-")) return "removed";
  return "context";
}

export function diffLines(diff: string): string[] {
  const lines = diff.split("\n");
  if (lines.at(-1) === "") lines.pop();
  return lines;
}
