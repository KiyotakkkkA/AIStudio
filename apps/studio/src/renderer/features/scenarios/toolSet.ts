import type { ScenarioParams, ToolDto } from "@zvs/shared";
import { toolReason } from "./palette";

export const TOOL_SET_TYPE = "tool.set";
export const TOOL_SET_PARAM = "tools";

export function toolSetKeys(params: ScenarioParams): string[] {
  const value = params[TOOL_SET_PARAM];
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is string => typeof entry === "string");
}

export function toggleToolKey(params: ScenarioParams, key: string): ScenarioParams {
  const keys = toolSetKeys(params);
  const next = keys.includes(key) ? keys.filter((entry) => entry !== key) : [...keys, key];
  return { ...params, [TOOL_SET_PARAM]: next };
}

export function toolSetProblem(params: ScenarioParams, tools: readonly ToolDto[]): string | null {
  for (const key of toolSetKeys(params)) {
    const reason = toolReason(key, tools);
    if (reason !== null) return `«${key}»: ${reason}`;
  }
  return null;
}
