import type { ScenarioNodeTypeDto, ToolDto } from "@zvs/shared";
import { effectiveTier } from "../tools/toolFilters";
import {
  NODE_TYPE_LABELS,
  PALETTE_GROUP_LABELS,
  TOOL_OFF,
  TOOL_UNAVAILABLE,
  TOOL_UNREGISTERED,
} from "./scenarioCopy";
import { isOutputType } from "./nodeForms";

export interface PaletteEntry {
  readonly type: string;
  readonly label: string;
  readonly spec: ScenarioNodeTypeDto;
}

export interface PaletteGroup {
  readonly key: keyof typeof PALETTE_GROUP_LABELS;
  readonly label: string;
  readonly entries: readonly PaletteEntry[];
}

const ORDER: readonly PaletteGroup["key"][] = ["triggers", "agents", "outputs", "core"];

export function toolFor(spec: ScenarioNodeTypeDto, tools: readonly ToolDto[]): ToolDto | undefined {
  return spec.toolKey === null ? undefined : tools.find((tool) => tool.key === spec.toolKey);
}

export function typeLabel(
  type: string,
  spec: ScenarioNodeTypeDto | undefined,
  tools: readonly ToolDto[],
): string {
  const tool = spec === undefined ? undefined : toolFor(spec, tools);
  return tool?.displayName ?? NODE_TYPE_LABELS[type] ?? type;
}

export function toolReason(key: string, tools: readonly ToolDto[]): string | null {
  const tool = tools.find((entry) => entry.key === key);
  if (tool === undefined) return TOOL_UNREGISTERED;
  if (effectiveTier(tool) === "off") return TOOL_OFF;
  if (!tool.available) return TOOL_UNAVAILABLE;
  return null;
}

export function ceilingReason(spec: ScenarioNodeTypeDto, tools: readonly ToolDto[]): string | null {
  return spec.toolKey === null ? null : toolReason(spec.toolKey, tools);
}

function groupOf(spec: ScenarioNodeTypeDto): PaletteGroup["key"] {
  if (spec.role === "trigger") return "triggers";
  if (isOutputType(spec.type)) return "outputs";
  return spec.role === "subNode" || spec.slots.length > 0 ? "agents" : "core";
}

function matches(entry: PaletteEntry, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return true;
  return entry.label.toLowerCase().includes(needle) || entry.type.toLowerCase().includes(needle);
}

export function paletteGroups(
  types: readonly ScenarioNodeTypeDto[],
  tools: readonly ToolDto[],
  query: string,
): PaletteGroup[] {
  const groups = new Map<PaletteGroup["key"], PaletteEntry[]>();
  for (const spec of types) {
    if (spec.toolKey !== null) continue;
    const entry: PaletteEntry = { type: spec.type, label: typeLabel(spec.type, spec, tools), spec };
    if (!matches(entry, query)) continue;
    const key = groupOf(spec);
    groups.set(key, [...(groups.get(key) ?? []), entry]);
  }
  return ORDER.flatMap((key) => {
    const entries = groups.get(key);
    if (entries === undefined) return [];
    return [
      {
        key,
        label: PALETTE_GROUP_LABELS[key],
        entries: [...entries].sort((a, b) => a.label.localeCompare(b.label, "ru")),
      },
    ];
  });
}
