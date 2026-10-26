import type { MessageDto, OfferedToolDto, RetrievalMode, ToolDto } from "@zvs/shared";

export const RECOMMENDED_TOOLS: readonly string[] = ["fs.list", "fs.read", "fs.search", "fs.apply"];

export const CORE_TOOLS: readonly string[] = ["chat.ask", "skill.load"];

export const KNOWLEDGE_SEARCH = "knowledge.search";
export const MEMORY_TOOL = "chat.memory";

export const GRAPH_ONLY_TOOLS: readonly string[] = ["vector.search", "vector.list"];

export function isCoreTool(key: string): boolean {
  return CORE_TOOLS.includes(key);
}

export const CORE_HINTS: Record<string, string> = {
  "chat.ask": "Модель может задать вопрос пользователю",
  "skill.load": "Подключается, когда к запросу подходит навык",
};

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
export const OFFERED_BY_MEMORY = "Подключён настройкой памяти";

export function offeredHint(tool: OfferedToolDto): string {
  if (tool.source === "skill") return tool.rootsOnly ? OFFERED_BY_SKILL_ROOTS : OFFERED_BY_SKILL;
  if (tool.source === "knowledge") return OFFERED_BY_KNOWLEDGE;
  return tool.key === MEMORY_TOOL ? OFFERED_BY_MEMORY : OFFERED_BY_SYSTEM;
}

export function automaticOffers(
  messages: readonly MessageDto[],
  selected: readonly string[],
): OfferedToolDto[] {
  const last = messages.findLast(
    (message) => message.role === "assistant" && message.offeredTools.length > 0,
  );
  return (last?.offeredTools ?? []).filter(
    (tool) => tool.source !== "selected" && !selected.includes(tool.key) && !isCoreTool(tool.key),
  );
}

export interface ToolPolicy {
  readonly attachedStores: number;
  readonly retrievalMode: RetrievalMode;
  readonly selected: readonly string[];
  readonly memory: boolean;
}

export function policyTools(
  offered: readonly OfferedToolDto[],
  policy: ToolPolicy,
): OfferedToolDto[] {
  const tools = [...offered];
  const missing = (key: string) =>
    !policy.selected.includes(key) && !tools.some((tool) => tool.key === key);
  if (policy.attachedStores > 0 && policy.retrievalMode !== "auto" && missing(KNOWLEDGE_SEARCH))
    tools.push({ key: KNOWLEDGE_SEARCH, source: "knowledge", rootsOnly: false });
  if (policy.memory && missing(MEMORY_TOOL))
    tools.push({ key: MEMORY_TOOL, source: "system", rootsOnly: false });
  return tools;
}
