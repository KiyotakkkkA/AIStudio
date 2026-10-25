import type { ToolDto } from "@zvs/shared";

export const RECOMMENDED_TOOLS: readonly string[] = ["fs.list", "fs.read", "fs.search", "fs.apply"];

export const AUTOMATIC_TOOLS: readonly string[] = [
  "skill.load",
  "skill.read",
  "skill.script",
  "chat.file",
];

export function isAutomaticTool(key: string): boolean {
  return AUTOMATIC_TOOLS.includes(key);
}

export function toolBlockReason(tool: ToolDto): string | null {
  if (!tool.available) return "Источник недоступен";
  if (!tool.enabled) return "Выключен в подключениях";
  if (tool.tier === "off") return "Запрещён на странице инструментов";
  return null;
}
