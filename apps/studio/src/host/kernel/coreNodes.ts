import { z } from "zod";
import {
  AppError,
  AppErrorCode,
  VectorIndexInput,
  VectorIndexReportDto,
  VectorSearchHitDto,
  VectorSearchInput,
} from "@zvs/shared";
import { NodeRegistry } from "./NodeRegistry.ts";

export const GenerateNodeInput = z.object({
  providerId: z.string().min(1),
  model: z.string().min(1),
  messages: z.array(
    z.object({ role: z.enum(["system", "user", "assistant"]), content: z.string() }),
  ),
  system: z.string().optional(),
  temperature: z.number().min(0).max(2).optional(),
  topK: z.number().int().positive().optional(),
  topP: z.number().min(0).max(1).optional(),
  maxOutputTokens: z.number().int().positive().optional(),
});

export function registerCoreNodes(registry: NodeRegistry): NodeRegistry {
  registry.register({
    type: "llm.generate",
    input: z.union([GenerateNodeInput, z.object({ request: GenerateNodeInput })]),
    output: z.object({ text: z.string(), reasoning: z.string() }),
    permission: { tool: "llm.generate" },
    sideEffect: true,
    async run(context, input) {
      const { providerId, ...request } = "request" in input ? input.request : input;
      if (!context.services.providers)
        throw new AppError(AppErrorCode.CONFLICT, "Provider service unavailable");
      const driver = await context.services.providers.text(providerId);
      let text = "";
      let reasoning = "";
      for await (const delta of driver.stream(request, context.signal)) {
        context.signal.throwIfAborted();
        if (delta.kind === "reasoning") reasoning += delta.text;
        else text += delta.text;
        context.emit({ type: "token", delta: delta.text, kind: delta.kind });
      }
      return { text, reasoning };
    },
  });
  registry.register({
    type: "vector.search",
    input: VectorSearchInput,
    output: z.array(VectorSearchHitDto),
    permission: { tool: "vector.search" },
    sideEffect: false,
    async run(context, { storeId, query, ...options }) {
      context.signal.throwIfAborted();
      if (!context.services.vectorStores)
        throw new AppError(AppErrorCode.CONFLICT, "Vector service unavailable");
      const hits = await context.services.vectorStores.search(
        storeId,
        query,
        options,
        context.signal,
      );
      context.signal.throwIfAborted();
      return hits;
    },
  });
  registry.register({
    type: "vector.index",
    input: VectorIndexInput,
    output: VectorIndexReportDto,
    permission: { tool: "vector.index" },
    sideEffect: true,
    async run(context, input) {
      context.signal.throwIfAborted();
      if (!context.services.indexing)
        throw new AppError(AppErrorCode.CONFLICT, "Сервис индексации недоступен");
      return context.services.indexing.index(input, {
        signal: context.signal,
        onProgress: ({ done, total, message }) => {
          context.emit({ type: "progress", done, total });
          if (message !== undefined)
            context.emit({ type: "log", line: { storeId: input.storeId, message } });
        },
      });
    },
  });
  registry.register({
    type: "flow.branch",
    input: z.object({ condition: z.boolean(), then: z.json(), else: z.json() }),
    output: z.json(),
    permission: { kind: "none" },
    sideEffect: false,
    async run(context, input) {
      context.signal.throwIfAborted();
      return input.condition ? input.then : input.else;
    },
  });
  registry.register({
    type: "flow.map",
    input: z.object({ items: z.array(z.json()), path: z.array(z.string()).default([]) }),
    output: z.array(z.json()),
    permission: { kind: "none" },
    sideEffect: false,
    async run(context, input) {
      return input.items.map((item) => {
        context.signal.throwIfAborted();
        let value = item;
        for (const key of input.path) {
          if (value === null || typeof value !== "object" || !Object.hasOwn(value, key))
            throw new AppError(AppErrorCode.VALIDATION_FAILED, `Missing map field: ${key}`);
          value = (value as Record<string, z.infer<ReturnType<typeof z.json>>>)[key]!;
        }
        return value;
      });
    },
  });
  return registry;
}
