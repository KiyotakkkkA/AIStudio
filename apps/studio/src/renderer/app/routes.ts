export const ROUTES = {
  browser: "/browser",
  secrets: "/secrets",
  providers: "/providers",
  vectorStores: "/vector-stores",
  chat: "/chat",
  agenticBuild: "/agentic-build",
  scenarios: "/scenarios",
  skills: "/skills",
  connections: "/connections",
  integrations: "/integrations",
  tools: "/tools",
  tasks: "/tasks",
  downloads: "/downloads",
  runs: "/runs",
  settings: "/settings",
} as const;

export type RoutePath = (typeof ROUTES)[keyof typeof ROUTES];

export const DEFAULT_ROUTE: RoutePath = ROUTES.secrets;

const KNOWN = new Set<string>(Object.values(ROUTES));

export function isRoutePath(value: unknown): value is RoutePath {
  return typeof value === "string" && KNOWN.has(value);
}
