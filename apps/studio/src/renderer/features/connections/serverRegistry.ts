import type { McpStartup, McpTransport } from "@zvs/shared";

export interface PresetValue {
  readonly name: string;
  readonly kind: "literal" | "secret";
  readonly value?: string;
}

export interface ServerPreset {
  readonly id: string;
  readonly name: string;
  readonly title: string;
  readonly description: string;
  readonly transport: McpTransport;
  readonly startup: McpStartup;
  readonly command?: string;
  readonly args?: readonly string[];
  readonly url?: string;
  readonly env?: readonly PresetValue[];
  readonly headers?: readonly PresetValue[];
  readonly note?: string;
}

export const SERVER_REGISTRY: readonly ServerPreset[] = [
  {
    id: "filesystem",
    name: "filesystem",
    title: "Filesystem",
    description: "Чтение, запись и поиск файлов внутри разрешённых папок.",
    transport: "stdio",
    startup: "manual",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "."],
    note: "Замените «.» на папку, к которой серверу можно обращаться.",
  },
  {
    id: "memory",
    name: "memory",
    title: "Memory",
    description: "Граф знаний в локальном файле: сущности, связи и наблюдения.",
    transport: "stdio",
    startup: "manual",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-memory"],
  },
  {
    id: "fetch",
    name: "fetch",
    title: "Fetch",
    description: "Загружает веб-страницу и отдаёт её текст в Markdown.",
    transport: "stdio",
    startup: "manual",
    command: "uvx",
    args: ["mcp-server-fetch"],
  },
  {
    id: "git",
    name: "git",
    title: "Git",
    description: "Статус, история, диффы и коммиты в локальном репозитории.",
    transport: "stdio",
    startup: "manual",
    command: "uvx",
    args: ["mcp-server-git", "--repository", "."],
    note: "Замените «.» на путь к репозиторию.",
  },
  {
    id: "time",
    name: "time",
    title: "Time",
    description: "Текущее время и перевод между часовыми поясами.",
    transport: "stdio",
    startup: "manual",
    command: "uvx",
    args: ["mcp-server-time"],
  },
  {
    id: "sequential-thinking",
    name: "sequential-thinking",
    title: "Sequential Thinking",
    description: "Пошаговое рассуждение с ревизией и ветвлением мыслей.",
    transport: "stdio",
    startup: "manual",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-sequential-thinking"],
  },
  {
    id: "qdrant",
    name: "qdrant-local",
    title: "Qdrant",
    description: "Семантический поиск и запись в коллекцию Qdrant.",
    transport: "stdio",
    startup: "manual",
    command: "uvx",
    args: ["mcp-server-qdrant"],
    env: [
      { name: "QDRANT_URL", kind: "literal", value: "http://127.0.0.1:6333" },
      { name: "COLLECTION_NAME", kind: "literal", value: "" },
      { name: "QDRANT_API_KEY", kind: "secret" },
    ],
  },
  {
    id: "github",
    name: "github",
    title: "GitHub",
    description: "Репозитории, задачи и pull request'ы через удалённый MCP GitHub.",
    transport: "http",
    startup: "manual",
    url: "https://api.githubcopilot.com/mcp/",
    headers: [{ name: "Authorization", kind: "secret" }],
    note: "Секрет должен хранить значение целиком: «Bearer <токен>».",
  },
  {
    id: "everything",
    name: "everything",
    title: "Everything",
    description: "Эталонный тестовый сервер: инструменты, ресурсы и промпты всех видов.",
    transport: "stdio",
    startup: "manual",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-everything"],
  },
];
