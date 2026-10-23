import type { McpServerDto, ScenarioNodeTypeDto, ToolDto } from "@zvs/shared";
import { effectiveTier, sourceKey } from "../tools/toolFilters";
import {
  NODE_TYPE_LABELS,
  PALETTE_GROUP_LABELS,
  TOOL_GROUP_LABELS,
  TOOL_OFF,
  TOOL_UNAVAILABLE,
  TOOL_UNREGISTERED,
} from "./scenarioCopy";

export interface PaletteEntry {
  readonly type: string;
  readonly label: string;
  readonly spec: ScenarioNodeTypeDto;
  readonly disabledReason: string | null;
}

export interface PaletteGroup {
  readonly key: string;
  readonly label: string;
  readonly entries: readonly PaletteEntry[];
}

const FIXED_ORDER = ["triggers", "agents", "core"];

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

export function ceilingReason(spec: ScenarioNodeTypeDto, tools: readonly ToolDto[]): string | null {
  if (spec.toolKey === null) return null;
  const tool = toolFor(spec, tools);
  if (tool === undefined) return TOOL_UNREGISTERED;
  if (effectiveTier(tool) === "off") return TOOL_OFF;
  if (!tool.available) return TOOL_UNAVAILABLE;
  return null;
}

function groupOf(
  spec: ScenarioNodeTypeDto,
  tools: readonly ToolDto[],
  servers: readonly McpServerDto[],
): { key: string; label: string } {
  if (spec.role === "trigger") return { key: "triggers", label: PALETTE_GROUP_LABELS.triggers };
  if (spec.toolKey === null) {
    const agentic = spec.role === "subNode" || spec.slots.length > 0;
    return agentic
      ? { key: "agents", label: PALETTE_GROUP_LABELS.agents }
      : { key: "core", label: PALETTE_GROUP_LABELS.core };
  }
  const tool = toolFor(spec, tools);
  if (tool === undefined) return { key: "tool:unknown", label: TOOL_GROUP_LABELS.builtin };
  const server =
    tool.source === "mcp" ? servers.find((entry) => entry.id === tool.sourceRef) : undefined;
  return {
    key: `tool:${sourceKey(tool)}`,
    label: server?.name ?? TOOL_GROUP_LABELS[tool.source],
  };
}

function matches(entry: PaletteEntry, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return true;
  return entry.label.toLowerCase().includes(needle) || entry.type.toLowerCase().includes(needle);
}

export function paletteGroups(
  types: readonly ScenarioNodeTypeDto[],
  tools: readonly ToolDto[],
  servers: readonly McpServerDto[],
  query: string,
): PaletteGroup[] {
  const groups = new Map<string, { label: string; entries: PaletteEntry[] }>();
  for (const spec of types) {
    const entry: PaletteEntry = {
      type: spec.type,
      label: typeLabel(spec.type, spec, tools),
      spec,
      disabledReason: ceilingReason(spec, tools),
    };
    if (!matches(entry, query)) continue;
    const { key, label } = groupOf(spec, tools, servers);
    const group = groups.get(key) ?? { label, entries: [] };
    group.entries.push(entry);
    groups.set(key, group);
  }
  const rank = (key: string) => {
    const fixed = FIXED_ORDER.indexOf(key);
    return fixed >= 0 ? fixed : FIXED_ORDER.length;
  };
  return [...groups.entries()]
    .sort(([left, a], [right, b]) => rank(left) - rank(right) || a.label.localeCompare(b.label))
    .map(([key, group]) => ({
      key,
      label: group.label,
      entries: [...group.entries].sort((a, b) => {
        const off = Number(a.disabledReason !== null) - Number(b.disabledReason !== null);
        return off !== 0 ? off : a.label.localeCompare(b.label, "ru");
      }),
    }));
}
