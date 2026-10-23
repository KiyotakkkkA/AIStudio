import type { DocumentSelectionDto } from "@zvs/shared";

export const SELECTIONS_LABEL = "Фрагменты документов";
export const EXCERPT_PREVIEW = 48;

export function selectionTitle(selection: DocumentSelectionDto): string {
  return selection.page === null
    ? selection.name
    : `${selection.name} · стр. ${selection.page.toLocaleString("ru-RU")}`;
}

export function excerptPreview(text: string, limit = EXCERPT_PREVIEW): string {
  const flat = text.replace(/\s+/gu, " ").trim();
  return flat.length <= limit ? flat : `${flat.slice(0, limit).trimEnd()}…`;
}

export function removeSelectionLabel(selection: DocumentSelectionDto): string {
  return `Убрать фрагмент: ${selectionTitle(selection)}`;
}
