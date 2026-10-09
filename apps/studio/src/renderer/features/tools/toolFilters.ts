import type { McpServerDto, ToolDto, ToolSource, ToolTier } from "@zvs/shared";

export type TierFilter = ToolTier | "all";

export interface SourceGroup {
  readonly key: string;
  readonly source: ToolSource;
  readonly sourceRef: string | null;
  readonly label: string;
  readonly server: McpServerDto | null;
}

export interface ToolFilter {
  readonly source: string;
  readonly tier: TierFilter;
  readonly query: string;
}

export const ALL_SOURCES = "all";

const SOURCE_ORDER: readonly ToolSource[] = ["builtin", "mcp", "skill", "integration", "browser"];

export const SOURCE_CLASS_LABELS: Record<ToolSource, string> = {
  builtin: "Встроенные",
  mcp: "MCP-серверы",
  skill: "Навыки",
  integration: "Интеграции",
  browser: "Браузер",
};

export function effectiveTier(tool: Pick<ToolDto, "tier" | "enabled">): ToolTier {
  return tool.enabled ? tool.tier : "off";
}

export function sourceKey(tool: Pick<ToolDto, "source" | "sourceRef">): string {
  if (tool.source === "builtin" || tool.sourceRef === null) return tool.source;
  return `${tool.source}:${tool.sourceRef}`;
}

function shortRef(ref: string): string {
  return ref.length > 12 ? `${ref.slice(0, 8)}…` : ref;
}

export function sourceGroups(
  tools: readonly ToolDto[],
  servers: readonly McpServerDto[],
): SourceGroup[] {
  const groups = new Map<string, SourceGroup>();
  for (const tool of tools) {
    const key = sourceKey(tool);
    if (groups.has(key)) continue;
    const server =
      tool.source === "mcp" ? (servers.find((entry) => entry.id === tool.sourceRef) ?? null) : null;
    const label =
      tool.source === "builtin"
        ? "Встроенные"
        : server !== null
          ? server.name
          : tool.sourceRef === null
            ? SOURCE_CLASS_LABELS[tool.source]
            : `${SOURCE_CLASS_LABELS[tool.source]} · ${shortRef(tool.sourceRef)}`;
    groups.set(key, { key, source: tool.source, sourceRef: tool.sourceRef, label, server });
  }
  return [...groups.values()].sort((left, right) => {
    const order = SOURCE_ORDER.indexOf(left.source) - SOURCE_ORDER.indexOf(right.source);
    return order !== 0 ? order : left.label.localeCompare(right.label);
  });
}

export function matchesQuery(tool: ToolDto, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return true;
  return (
    tool.key.toLowerCase().includes(needle) ||
    tool.displayName.toLowerCase().includes(needle) ||
    tool.description.toLowerCase().includes(needle)
  );
}

function matchesSource(tool: ToolDto, source: string): boolean {
  return source === ALL_SOURCES || sourceKey(tool) === source;
}

function matchesTier(tool: ToolDto, tier: TierFilter): boolean {
  return tier === "all" || effectiveTier(tool) === tier;
}

export function filterTools(tools: readonly ToolDto[], filter: ToolFilter): ToolDto[] {
  return tools
    .filter(
      (tool) =>
        matchesSource(tool, filter.source) &&
        matchesTier(tool, filter.tier) &&
        matchesQuery(tool, filter.query),
    )
    .sort((left, right) => left.key.localeCompare(right.key));
}

export function sourceCounts(
  tools: readonly ToolDto[],
  filter: ToolFilter,
): Record<string, number> {
  const counts: Record<string, number> = { [ALL_SOURCES]: 0 };
  for (const tool of tools) {
    if (!matchesTier(tool, filter.tier) || !matchesQuery(tool, filter.query)) continue;
    counts[ALL_SOURCES] = (counts[ALL_SOURCES] ?? 0) + 1;
    const key = sourceKey(tool);
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}

export function tierCounts(
  tools: readonly ToolDto[],
  filter: ToolFilter,
): Record<ToolTier, number> {
  const counts: Record<ToolTier, number> = { auto: 0, ask: 0, off: 0 };
  for (const tool of tools) {
    if (!matchesSource(tool, filter.source) || !matchesQuery(tool, filter.query)) continue;
    counts[effectiveTier(tool)] += 1;
  }
  return counts;
}
