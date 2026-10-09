import { z } from "zod";
import type { Json, ToolDto } from "@zvs/shared";
import type { NodeRegistry } from "../kernel/NodeRegistry.ts";
import type { ToolRegistry } from "./ToolRegistry.ts";

interface BuiltinToolMeta {
  displayName: string;
  description: string;
  destructive?: boolean;
}

const NOT_TOOLS: ReadonlySet<string> = new Set(["llm.generate"]);

const BUILTIN: Record<string, BuiltinToolMeta> = {
  "vector.search": {
    displayName: "vector.search",
    description:
      "Семантический поиск по векторному хранилищу; возвращает фрагменты с оценками. storeId — UUID из vector.list.",
  },
  "vector.list": {
    displayName: "vector.list",
    description:
      "Список векторных хранилищ: UUID, название, описание и число документов. UUID передаётся в vector.search как storeId.",
  },
  "vector.index": {
    displayName: "vector.index",
    description: "Индексация источников хранилища: извлечение текста, нарезка и эмбеддинги.",
  },
  "download.fetch": {
    displayName: "download.fetch",
    description: "Загрузка файла из очереди загрузок с докачкой и проверкой контрольной суммы.",
  },
  "job.run": {
    displayName: "job.run",
    description: "Запуск задания в сайдкаре с передачей прогресса в поток выполнения.",
  },
};

function toJsonSchema(schema: z.ZodType<unknown>): Json {
  try {
    return z.toJSONSchema(schema, { io: "input", unrepresentable: "any" }) as Json;
  } catch {
    return null;
  }
}

export function registerKernelTools(nodes: NodeRegistry, tools: ToolRegistry): ToolDto[] {
  const registered = new Map<string, ToolDto>();
  for (const node of nodes.list()) {
    const requirement = node.permission;
    if ("kind" in requirement) continue;
    const key = requirement.tool;
    if (NOT_TOOLS.has(key)) continue;
    const previous = registered.get(key);
    const meta = BUILTIN[key];
    const dto = tools.register({
      key,
      source: "builtin",
      displayName: meta?.displayName ?? key,
      description: meta?.description ?? "",
      inputSchema: toJsonSchema(node.input),
      readOnly: node.sideEffectFree && (previous?.readOnly ?? true),
      destructive: meta?.destructive ?? false,
    });
    registered.set(key, dto);
  }
  return [...registered.values()];
}
