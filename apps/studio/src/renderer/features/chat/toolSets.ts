import type { ToolDto } from "@zvs/shared";

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
