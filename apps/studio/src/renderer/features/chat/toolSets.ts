import type { MessageDto, OfferedToolDto, ToolDto } from "@zvs/shared";

export const RECOMMENDED_TOOLS: readonly string[] = ["fs.list", "fs.read", "fs.search", "fs.apply"];

export const AUTOMATIC_TOOLS: readonly string[] = ["skill.load"];

export const GRAPH_ONLY_TOOLS: readonly string[] = ["vector.search", "vector.list"];

export function isAutomaticTool(key: string): boolean {
  return AUTOMATIC_TOOLS.includes(key);
}

export function isModelTool(key: string): boolean {
  return !GRAPH_ONLY_TOOLS.includes(key);
}

export function toolBlockReason(tool: ToolDto): string | null {
  if (!tool.available) return "Источник недоступен";
  if (!tool.enabled) return "Выключен в подключениях";
  if (tool.tier === "off") return "Запрещён на странице инструментов";
  return null;
}

export const OFFERED_BY_SKILL = "Подключён навыком";
export const OFFERED_BY_SKILL_ROOTS = "Подключён навыком: доступны только пути chat: и skill:";
export const OFFERED_BY_KNOWLEDGE = "Подключён для поиска по хранилищам знаний";
export const OFFERED_BY_SYSTEM = "Подключён приложением";

export function offeredHint(tool: OfferedToolDto): string {
  if (tool.source === "skill") return tool.rootsOnly ? OFFERED_BY_SKILL_ROOTS : OFFERED_BY_SKILL;
  if (tool.source === "knowledge") return OFFERED_BY_KNOWLEDGE;
  return OFFERED_BY_SYSTEM;
}

export function automaticOffers(
  messages: readonly MessageDto[],
  selected: readonly string[],
): OfferedToolDto[] {
  const last = messages.findLast(
    (message) => message.role === "assistant" && message.offeredTools.length > 0,
  );
  return (last?.offeredTools ?? []).filter(
    (tool) => tool.source !== "selected" && !selected.includes(tool.key),
  );
}
