import type { ChatContextToolDto, OfferedToolSource } from "@zvs/shared";

export const BASE_TOOLS_TITLE = "Базовые";
export const AUTOMATIC_TOOLS_TITLE = "Подключены автоматически";
export const COMMAND_TOOLS_TITLE = "Подключены командой";
export const LOADED_TOOLS_TITLE = "Загружены по запросу";

export const OFFERED_BY_KNOWLEDGE = "Подключён для поиска по хранилищам знаний";
export const OFFERED_BY_MEMORY = "Подключён настройкой памяти";
export const OFFERED_BY_COMMAND = "Подключён командой в этом диалоге";
export const OFFERED_ON_REQUEST = "Загружен моделью для этого диалога";

export const SOURCE_HINTS: Record<OfferedToolSource, string | null> = {
  base: null,
  knowledge: OFFERED_BY_KNOWLEDGE,
  memory: OFFERED_BY_MEMORY,
  command: OFFERED_BY_COMMAND,
  loaded: OFFERED_ON_REQUEST,
};

const SECTIONS: readonly { title: string; sources: readonly OfferedToolSource[] }[] = [
  { title: BASE_TOOLS_TITLE, sources: ["base"] },
  { title: AUTOMATIC_TOOLS_TITLE, sources: ["knowledge", "memory"] },
  { title: COMMAND_TOOLS_TITLE, sources: ["command"] },
  { title: LOADED_TOOLS_TITLE, sources: ["loaded"] },
];

export interface ToolSection {
  readonly title: string;
  readonly items: readonly ChatContextToolDto[];
}

export function toolSections(items: readonly ChatContextToolDto[]): ToolSection[] {
  return SECTIONS.map((section) => ({
    title: section.title,
    items: items.filter((item) => section.sources.includes(item.source)),
  })).filter((section) => section.items.length > 0);
}
