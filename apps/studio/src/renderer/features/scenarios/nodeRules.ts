import {
  isWarningRule,
  type ScenarioGraphDto,
  type ScenarioIssueDto,
  type ScenarioNodeDto,
  type ScenarioParams,
} from "@zvs/shared";
import { childrenIn, findNode, parentOf } from "./graphModel";
import { TOOL_SET_PARAM, TOOL_SET_TYPE } from "./toolSet";

export const TRIGGER_CHAT_TYPE = "trigger.chat";
export const HISTORY_TYPE = "memory.window";
export const FACTS_TYPE = "memory.facts";
export const STORES_TYPE = "knowledge.stores";
export const BRANCH_TYPE = "flow.branch";
export const CHAT_REPLY_TYPE = "output.chatReply";

export const STORES_PARAM = "stores";
export const POLICY_PARAM = "policy";
export const AREA_PARAM = "area";

export type FactsPolicy = "read" | "readWrite";
export type FactsArea = "global" | "dialog" | "both";

export const KNOWLEDGE_TOOL = "knowledge.search";
export const MEMORY_TOOL = "memory";

export const RESTRICTED_TOOL_PREFIXES: readonly string[] = ["browser.", "vector.", "knowledge."];

export const MEMORY_TYPES: ReadonlySet<string> = new Set([HISTORY_TYPE, FACTS_TYPE, STORES_TYPE]);

export type DialogNeed = "required" | "partial";

export interface GrantedTool {
  readonly key: string;
  readonly nodeId: string;
}

export function memoryKinds(graph: ScenarioGraphDto, parentId: string): Set<string> {
  return new Set(
    childrenIn(graph, parentId, "memory").flatMap((attachment) => {
      const child = findNode(graph, attachment.childNodeId);
      return child === undefined ? [] : [child.type];
    }),
  );
}

export function isRestrictedTool(key: string): boolean {
  return RESTRICTED_TOOL_PREFIXES.some((prefix) => key.startsWith(prefix));
}

export function isWarning(issue: ScenarioIssueDto): boolean {
  return isWarningRule(issue.rule);
}

export function blockingIssues(issues: readonly ScenarioIssueDto[]): ScenarioIssueDto[] {
  return issues.filter((issue) => !isWarning(issue));
}

export function factsArea(params: ScenarioParams): FactsArea {
  const area = params[AREA_PARAM];
  return area === "dialog" || area === "both" ? area : "global";
}

export function factsPolicy(params: ScenarioParams): FactsPolicy {
  return params[POLICY_PARAM] === "readWrite" ? "readWrite" : "read";
}

export function storeIds(params: ScenarioParams): string[] {
  const value = params[STORES_PARAM];
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is string => typeof entry === "string");
}

export function toggleStore(params: ScenarioParams, id: string): ScenarioParams {
  const ids = storeIds(params);
  const next = ids.includes(id) ? ids.filter((entry) => entry !== id) : [...ids, id];
  return { ...params, [STORES_PARAM]: next };
}

export function dialogNeed(node: ScenarioNodeDto): DialogNeed | null {
  if (node.type === HISTORY_TYPE || node.type === CHAT_REPLY_TYPE) return "required";
  if (node.type !== FACTS_TYPE) return null;
  const area = factsArea(node.params);
  if (area === "dialog") return "required";
  return area === "both" ? "partial" : null;
}

export function chatTrigger(graph: ScenarioGraphDto): ScenarioNodeDto | undefined {
  return graph.nodes.find((node) => node.type === TRIGGER_CHAT_TYPE);
}

export function defaultParams(type: string): ScenarioParams {
  if (type === TOOL_SET_TYPE) return { [TOOL_SET_PARAM]: [] };
  if (type === STORES_TYPE) return { [STORES_PARAM]: [] };
  if (type === FACTS_TYPE) return { [POLICY_PARAM]: "read", [AREA_PARAM]: "global" };
  return {};
}

export function grantedTools(graph: ScenarioGraphDto, toolSetId: string): GrantedTool[] {
  const parent = parentOf(graph, toolSetId);
  if (parent === undefined) return [];
  const granted: GrantedTool[] = [];
  for (const attachment of childrenIn(graph, parent.parentNodeId, "memory")) {
    const child = findNode(graph, attachment.childNodeId);
    if (child === undefined || child.disabled === true) continue;
    if (child.type === STORES_TYPE) granted.push({ key: KNOWLEDGE_TOOL, nodeId: child.id });
    if (child.type === FACTS_TYPE && factsPolicy(child.params) === "readWrite") {
      granted.push({ key: MEMORY_TOOL, nodeId: child.id });
    }
  }
  return granted.sort((left, right) => left.key.localeCompare(right.key));
}
