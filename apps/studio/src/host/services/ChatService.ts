import { z } from "zod";
import {
  AppError,
  AppErrorCode,
  ChatEnabledTools,
  ChatSendInput,
  type ChatToolCallDto,
  ConversationDetailDto,
  DEFAULT_CHAT_MIN_SCORE,
  GRAPH_EXPANSION_DEFAULT_MAX,
  mergeGraphNeighbors,
  modelToolSupport,
  resolveToolMode,
  type GraphNeighborhood,
  ConversationDto,
  CreateConversationInput,
  MessageDto,
  StartRunInput,
  VectorSearchHitDto,
  type HostEvent,
  type RunHandleDto,
} from "@zvs/shared";
import { withInjectedTools } from "../drivers/ai/injectedTools.ts";
import type {
  ChatMessage,
  GenerateRequest,
  TextGenerationDriver,
  ToolCall,
} from "../drivers/ai/ports.ts";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import { GenerateNodeInput } from "../kernel/coreNodes.ts";
import type { NodeRegistry } from "../kernel/NodeRegistry.ts";
import type { StepContext } from "../kernel/types.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { RunService } from "./RunService.ts";
import {
  estimateTokens,
  windowChatHistory,
  type RelatedSource,
  type RetrievedSource,
} from "./chatHistory.ts";
import { CHAT_FETCH_K, mergeRetrieval } from "./retrievalMerge.ts";
import { retrievalQuery } from "./retrievalQuery.ts";
import type { ToolRegistry } from "../tools/ToolRegistry.ts";
import {
  CHAT_TOOL_CALLS,
  CHAT_TOOL_ROUNDS,
  OfferedTool,
  TOOL_BUDGET_NOTE,
  TOOL_PREVIEW_CHARS,
  TOOL_RESULTS_INSTRUCTION,
  capText,
  fenceToolResult,
  offeredTools,
  parseToolArguments,
  toolResultBudget,
  toolResultText,
  toolSpecs,
} from "./chatTools.ts";

export const CHAT_RELATED_MAX = 3;
const BUDGET_EXHAUSTED = "Бюджет вызовов инструментов на этот ход исчерпан";

export const ChatGenerateRequest = GenerateNodeInput.extend({
  assistantId: z.string(),
  tools: z.array(OfferedTool).default([]),
  modelTools: z.boolean().nullable().default(null),
  toolResultChars: z.number().int().positive().default(4000),
});
export type ChatGenerateRequest = z.infer<typeof ChatGenerateRequest>;

interface RoundOutput {
  text: string;
  reasoning: string;
  toolCalls: ToolCall[];
}

export interface ChatServiceOptions {
  data: UnitOfWork;
  runs: RunService;
  registry: NodeRegistry;
  tools?: Pick<ToolRegistry, "list">;
  logger?: Logger;
  clock?: () => number;
}
export class ChatService {
  private readonly clock: () => number;
  constructor(private readonly options: ChatServiceOptions) {
    this.clock = options.clock ?? Date.now;
    options.registry.register({
      type: "chat.prepare",
      input: z
        .object({
          conversationId: z.string(),
          userId: z.string(),
          assistantId: z.string(),
          providerId: z.string(),
          modelId: z.string(),
          query: z.string().optional(),
          settings: z.object({
            temperature: z.number().optional(),
            topK: z.number().int().positive().optional(),
            topP: z.number().optional(),
            maxOutputTokens: z.number().int().positive().optional(),
          }),
        })
        .catchall(z.array(VectorSearchHitDto)),
      output: ChatGenerateRequest,
      permission: { kind: "none" },
      sideEffect: true,
      run: async (context, input) => {
        context.signal.throwIfAborted();
        const conversation = this.get(input.conversationId);
        this.requireTurn(context.runId, input.conversationId, input.userId);
        const user = conversation.messages.find((message) => message.id === input.userId)!;
        const history = conversation.messages.slice(0, conversation.messages.indexOf(user));
        const model = this.resolveModel(
          conversation,
          input.providerId ?? conversation.providerId,
          input.modelId ?? conversation.modelId,
          input.settings,
        );
        const merged = mergeRetrieval(
          conversation.attachedStoreIds.map((storeId, index) => ({
            storeId,
            hits: input[`store${index}`] ?? [],
            floor:
              this.options.data.repositories.vectorStores.findById(storeId)?.chatMinScore ??
              DEFAULT_CHAT_MIN_SCORE,
          })),
        );
        const ranked = merged.hits;
        const sources = await this.withNeighbours(context, ranked);
        const related =
          conversation.useRelations && ranked.length > 0
            ? await this.relatedSources(context, input.query ?? user.content, ranked)
            : [];
        const offered = offeredTools(conversation.enabledTools, this.options.tools?.list() ?? []);
        const prepared = windowChatHistory(
          offered.length > 0
            ? `${conversation.systemPrompt}\n\n${TOOL_RESULTS_INSTRUCTION}\n${JSON.stringify(toolSpecs(offered))}`
            : conversation.systemPrompt,
          history.map(({ role, content }) => ({ role, content })),
          { role: user.role, content: user.content },
          model.contextWindow,
          model.maxOutputTokens,
          sources,
          related,
        );
        if (conversation.attachedStoreIds.length > 0) {
          const line = {
            message: "Retrieval context assembled",
            conversationId: conversation.id,
            query: input.query ?? user.content,
            fetched: merged.stats.fetched,
            belowFloor: merged.stats.belowFloor,
            duplicates: merged.stats.duplicates,
            overDocumentCap: merged.stats.overCap,
            overLimit: merged.stats.overLimit,
            injected: prepared.citations.length - prepared.related,
            droppedByBudget: prepared.droppedByBudget,
            neighbours: prepared.neighbours,
            related: prepared.related,
          };
          context.emit({ type: "log", line });
          this.options.logger?.log("info", "chat", line.message, { ...line, runId: context.runId });
        }
        if (prepared.trimmed) {
          const line = {
            message: "Chat history trimmed",
            conversationId: conversation.id,
            trimmed: prepared.trimmed,
          };
          context.emit({ type: "log", line });
          this.options.logger?.log("info", "chat", line.message, { ...line, runId: context.runId });
        }
        this.options.data.repositories.chat.addMessage({
          id: input.assistantId,
          conversationId: conversation.id,
          role: "assistant",
          content: "",
          reasoning: "",
          citations: prepared.citations,
          tokensIn: prepared.tokensIn,
          tokensOut: 0,
          usageEstimated: true,
          durationMs: 0,
          runId: context.runId,
          partial: true,
          createdAt: this.clock(),
        });
        return {
          providerId: conversation.providerId,
          model: model.externalId,
          messages: prepared.messages.flatMap(({ role, content }) =>
            role === "tool" ? [] : [{ role, content }],
          ),
          system: conversation.systemPrompt,
          ...input.settings,
          maxOutputTokens: model.maxOutputTokens,
          assistantId: input.assistantId,
          tools: offered,
          modelTools: model.supportsTools,
          toolResultChars: toolResultBudget(
            model.contextWindow - model.maxOutputTokens - prepared.tokensIn,
          ),
        };
      },
    });
    options.registry.register({
      type: "chat.generate",
      input: z.object({ request: ChatGenerateRequest }),
      output: z.object({ text: z.string(), reasoning: z.string() }),
      permission: { tool: "llm.generate" },
      sideEffect: true,
      run: async (context, { request }) => this.respond(context, request),
    });
    options.registry.register({
      type: "chat.persist",
      input: z.object({
        assistantId: z.string(),
        result: z.object({ text: z.string(), reasoning: z.string() }),
      }),
      output: MessageDto,
      permission: { kind: "none" },
      sideEffect: true,
      run: async (context, input) => {
        context.signal.throwIfAborted();
        const repository = this.options.data.repositories.chat;
        const message = repository.getMessage(input.assistantId);
        if (!message || message.runId !== context.runId)
          throw new AppError(AppErrorCode.CONFLICT, "Message does not belong to this run");
        repository.updateMessage(message.id, {
          content: input.result.text,
          reasoning: input.result.reasoning,
          tokensOut: estimateTokens(input.result.text + input.result.reasoning),
          partial: false,
          durationMs: Math.max(0, this.clock() - message.createdAt),
        });
        return MessageDto.parse(repository.getMessage(message.id));
      },
    });
  }
  list(): ConversationDto[] {
    return this.options.data.repositories.chat.list().map((row) => ConversationDto.parse(row));
  }
  get(id: string): ConversationDetailDto {
    const repository = this.options.data.repositories.chat;
    const row = repository.get(id);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Conversation not found");
    return ConversationDetailDto.parse({ ...row, messages: repository.messages(id) });
  }
  create(raw: CreateConversationInput): ConversationDto {
    const input = CreateConversationInput.parse(raw);
    this.resolveModel(input);
    for (const id of input.attachedStoreIds)
      if (!this.options.data.repositories.vectorStores.findById(id))
        throw new AppError(AppErrorCode.NOT_FOUND, "Vector store not found");
    this.requireTools(input.enabledTools);
    return ConversationDto.parse(
      this.options.data.repositories.chat.create({
        ...input,
        attachedStoreIds: [...new Set(input.attachedStoreIds)],
        enabledTools: [...new Set(input.enabledTools)],
        id: createId(),
        createdAt: this.clock(),
        updatedAt: this.clock(),
      }),
    );
  }
  rename(id: string, title: string): ConversationDto {
    this.get(id);
    const parsed = z.string().trim().min(1).max(200).parse(title);
    return ConversationDto.parse(
      this.options.data.repositories.chat.update(id, { title: parsed, updatedAt: this.clock() }),
    );
  }
  updateStores(id: string, attachedStoreIds: string[]): ConversationDto {
    this.get(id);
    const ids = z.array(z.string()).max(64).parse(attachedStoreIds);
    for (const storeId of ids)
      if (!this.options.data.repositories.vectorStores.findById(storeId))
        throw new AppError(AppErrorCode.NOT_FOUND, "Vector store not found");
    return ConversationDto.parse(
      this.options.data.repositories.chat.update(id, {
        attachedStoreIds: [...new Set(ids)],
        updatedAt: this.clock(),
      }),
    );
  }
  updateTools(id: string, enabledTools: string[]): ConversationDto {
    this.get(id);
    const keys = [...new Set(ChatEnabledTools.parse(enabledTools))];
    this.requireTools(keys);
    return ConversationDto.parse(
      this.options.data.repositories.chat.update(id, {
        enabledTools: keys,
        updatedAt: this.clock(),
      }),
    );
  }
  updateRelations(id: string, useRelations: boolean): ConversationDto {
    this.get(id);
    return ConversationDto.parse(
      this.options.data.repositories.chat.update(id, {
        useRelations: z.boolean().parse(useRelations),
        updatedAt: this.clock(),
      }),
    );
  }
  truncate(conversationId: string, messageId: string): ConversationDetailDto {
    const conversation = this.get(conversationId);
    this.requireIdle(conversationId);
    if (!conversation.messages.some((message) => message.id === messageId))
      throw new AppError(AppErrorCode.NOT_FOUND, "Message not found");
    this.options.data.transaction(({ chat }) => {
      chat.truncateFrom(conversationId, messageId);
      chat.update(conversationId, { updatedAt: this.clock() });
    });
    return this.get(conversationId);
  }
  remove(id: string): void {
    this.get(id);
    this.requireIdle(id);
    this.options.data.repositories.chat.remove(id);
  }
  sendMessage(
    conversationId: string,
    text: string,
    selection: Pick<ChatSendInput, "providerId" | "modelId" | "settings"> = {},
  ): RunHandleDto {
    const input = ChatSendInput.parse({ conversationId, text, ...selection });
    const conversation = this.get(conversationId);
    this.requireIdle(conversationId);
    const providerId = input.providerId ?? conversation.providerId;
    const modelId = input.modelId ?? conversation.modelId;
    const settings = input.settings ?? conversation.settings;
    const model = this.resolveModel(conversation, providerId, modelId, settings);
    windowChatHistory(
      conversation.systemPrompt,
      [],
      { role: "user", content: input.text },
      model.contextWindow,
      model.maxOutputTokens,
    );
    const userId = createId();
    const assistantId = createId();
    const previousUser = conversation.messages.findLast((message) => message.role === "user");
    const query = retrievalQuery(input.text, previousUser?.content);
    const retrieval = conversation.attachedStoreIds.map((storeId, index) => ({
      id: `store${index}`,
      type: "vector.search",
      input: { storeId, query, k: CHAT_FETCH_K },
    }));
    const graph = {
      nodes: [
        ...retrieval,
        {
          id: "prepare",
          type: "chat.prepare",
          dependencies: retrieval.map((node) => node.id),
          input: { conversationId, userId, assistantId, providerId, modelId, query, settings },
          bindings: Object.fromEntries(
            retrieval.map((node) => [node.id, { source: "node", nodeId: node.id }]),
          ),
        },
        {
          id: "generate",
          type: "chat.generate",
          dependencies: ["prepare"],
          input: {},
          bindings: { request: { source: "node", nodeId: "prepare" } },
        },
        {
          id: "persist",
          type: "chat.persist",
          dependencies: ["generate"],
          input: { assistantId },
          bindings: { result: { source: "node", nodeId: "generate" } },
        },
      ],
    };
    const handle = this.options.data.transaction(({ chat }) => {
      chat.addMessage({
        id: userId,
        conversationId,
        role: "user",
        content: input.text,
        citations: [],
        tokensIn: estimateTokens(input.text),
        tokensOut: 0,
        durationMs: 0,
        runId: null,
        partial: false,
        createdAt: this.clock(),
      });
      const handle = this.options.runs.start(
        StartRunInput.parse({
          kind: "chat",
          subjectId: conversationId,
          title: conversation.title,
          graph,
        }),
      );
      chat.updateMessage(userId, { runId: handle.id });
      chat.update(conversationId, { updatedAt: this.clock() });
      return handle;
    });
    this.options.runs.subscribe(handle.id, (event) => this.record(assistantId, event));
    return handle;
  }
  async cancel(id: string): Promise<void> {
    if (this.options.runs.get(id).kind !== "chat")
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Not a chat run");
    await this.options.runs.cancel(id);
  }
  private record(assistantId: string, event: HostEvent): void {
    const repository = this.options.data.repositories.chat;
    const message = repository.getMessage(assistantId);
    if (!message) return;
    if (event.type === "token") {
      repository.updateMessage(assistantId, {
        content: event.kind === "reasoning" ? message.content : message.content + event.delta,
        reasoning: event.kind === "reasoning" ? message.reasoning + event.delta : message.reasoning,
        tokensOut: message.tokensOut + estimateTokens(event.delta),
        durationMs: Math.max(0, this.clock() - message.createdAt),
      });
    }
    if (event.type === "end") {
      repository.updateMessage(assistantId, {
        durationMs: Math.max(0, this.clock() - message.createdAt),
      });
      const conversation = this.get(message.conversationId);
      const first = conversation.messages.find((item) => item.role === "user");
      const renamed =
        conversation.title === "New conversation" && first && message.content
          ? first.content.replace(/\s+/g, " ").slice(0, 80)
          : undefined;
      repository.update(conversation.id, {
        updatedAt: this.clock(),
        ...(renamed === undefined ? {} : { title: renamed }),
      });
      if (renamed !== undefined && message.runId !== null)
        this.options.data.repositories.runs.update(message.runId, { title: renamed });
    }
  }
  private async withNeighbours(
    context: StepContext,
    ranked: readonly RetrievedSource[],
  ): Promise<RetrievedSource[]> {
    const chunks = context.services.vectorStores?.chunks;
    if (chunks === undefined || ranked.length === 0) return [...ranked];
    const wanted = new Map<string, { storeId: string; documentId: string; indices: Set<number> }>();
    for (const { storeId, hit } of ranked) {
      const key = `${storeId}\u0000${hit.documentId}`;
      const entry = wanted.get(key) ?? { storeId, documentId: hit.documentId, indices: new Set() };
      for (const index of [hit.chunkIndex - 1, hit.chunkIndex + 1])
        if (index >= 0) entry.indices.add(index);
      wanted.set(key, entry);
    }
    const found = new Map<string, Awaited<ReturnType<typeof chunks>>>();
    for (const [key, entry] of wanted) {
      context.signal.throwIfAborted();
      try {
        found.set(
          key,
          await chunks.call(context.services.vectorStores, entry.storeId, entry.documentId, [
            ...entry.indices,
          ]),
        );
      } catch (error) {
        this.options.logger?.log("warn", "chat", "Could not fetch neighbouring chunks", {
          runId: context.runId,
          storeId: entry.storeId,
          error: String(error),
        });
      }
    }
    return ranked.map((source) => ({
      ...source,
      neighbours: (found.get(`${source.storeId}\u0000${source.hit.documentId}`) ?? []).filter(
        (chunk) => Math.abs(chunk.chunkIndex - source.hit.chunkIndex) === 1,
      ),
    }));
  }
  private async relatedSources(
    context: StepContext,
    query: string,
    ranked: readonly RetrievedSource[],
  ): Promise<RelatedSource[]> {
    const graph = context.services.graph;
    const vectorStores = context.services.vectorStores;
    if (graph === undefined || vectorStores?.relatedChunks === undefined) return [];
    const related: RelatedSource[] = [];
    const storeIds = [...new Set(ranked.map((source) => source.storeId))];
    for (const storeId of storeIds) {
      context.signal.throwIfAborted();
      const hits = ranked
        .filter((source) => source.storeId === storeId)
        .map((source) => source.hit);
      try {
        const neighborhoods: GraphNeighborhood[] = [];
        for (const documentId of [...new Set(hits.map((hit) => hit.documentId))])
          neighborhoods.push({
            documentId,
            neighbors: await graph.neighbors({ storeId, documentId, direction: "both" }),
          });
        const documents = this.options.data.repositories.vectorDocuments.listByStore(storeId);
        const expansions = mergeGraphNeighbors(
          hits,
          neighborhoods,
          documents,
          Math.min(CHAT_RELATED_MAX, GRAPH_EXPANSION_DEFAULT_MAX),
        );
        if (expansions.length === 0) continue;
        const best = await vectorStores.relatedChunks.call(
          vectorStores,
          storeId,
          query,
          expansions.map((expansion) => expansion.documentId),
          context.signal,
        );
        for (const expansion of expansions) {
          const hit = best.find((candidate) => candidate.documentId === expansion.documentId);
          if (hit !== undefined)
            related.push({
              storeId: storeId as RetrievedSource["storeId"],
              hit,
              relation: expansion.relation,
            });
        }
      } catch (error) {
        if (context.signal.aborted) throw error;
        this.options.logger?.log("warn", "chat", "Could not expand sources through the graph", {
          runId: context.runId,
          storeId,
          error: String(error),
        });
      }
    }
    return related;
  }
  private requireTools(keys: readonly string[]): void {
    if (keys.length === 0) return;
    const known = new Set(this.options.data.repositories.tools.list().map((tool) => tool.key));
    for (const key of keys)
      if (!known.has(key))
        throw new AppError(AppErrorCode.NOT_FOUND, "Инструмент не найден", { details: { key } });
  }
  private async respond(
    context: StepContext,
    request: ChatGenerateRequest,
  ): Promise<{ text: string; reasoning: string }> {
    const {
      providerId,
      assistantId,
      tools,
      modelTools,
      toolResultChars,
      messages: initial,
      system: prompt,
      ...base
    } = request;
    if (!context.services.providers)
      throw new AppError(AppErrorCode.CONFLICT, "Provider service unavailable");
    const raw = await context.services.providers.text(providerId);
    const mode =
      tools.length === 0 ? "none" : resolveToolMode(raw.capabilities().tools, modelTools);
    const offered = mode === "none" ? [] : tools;
    const driver = mode === "injected" ? withInjectedTools(raw) : raw;
    const specs = toolSpecs(offered);
    const byName = new Map(offered.map((tool) => [tool.name, tool]));
    if (tools.length > 0 && offered.length === 0)
      context.emit({
        type: "log",
        line: { message: "The model does not support tool calling; tools were not offered" },
      });
    const system =
      specs.length > 0
        ? [prompt ?? "", TOOL_RESULTS_INSTRUCTION].filter((part) => part.trim()).join("\n\n")
        : prompt;
    const messages: ChatMessage[] = [...initial];
    const calls: ChatToolCallDto[] = [];
    let text = "";
    let reasoning = "";
    let executed = 0;
    let round = 0;
    try {
      for (;;) {
        const final = round >= CHAT_TOOL_ROUNDS || executed >= CHAT_TOOL_CALLS;
        if (final && specs.length > 0) messages.push({ role: "user", content: TOOL_BUDGET_NOTE });
        const output = await this.generateRound(
          context,
          driver,
          {
            ...base,
            ...(system === undefined ? {} : { system }),
            messages: [...messages],
            ...(specs.length > 0 ? { tools: specs } : {}),
          },
          text,
        );
        text += output.text;
        reasoning += output.reasoning;
        if (output.toolCalls.length === 0 || specs.length === 0) break;
        messages.push({ role: "assistant", content: output.text, toolCalls: output.toolCalls });
        for (const call of output.toolCalls) {
          const record: ChatToolCallDto = {
            id: call.id,
            key: byName.get(call.name)?.key ?? call.name,
            name: call.name,
            round,
            arguments: parseToolArguments(call.arguments) ?? call.arguments,
            status: "running",
            result: null,
            truncated: false,
            error: null,
            approvalId: null,
            stepId: null,
            durationMs: 0,
          };
          calls.push(record);
          const refuse = final || executed >= CHAT_TOOL_CALLS ? BUDGET_EXHAUSTED : undefined;
          if (refuse === undefined) executed += 1;
          const content = await this.callTool(context, assistantId, calls, record, {
            tool: byName.get(call.name),
            label: `call${String(calls.length)}`,
            limit: toolResultChars,
            ...(refuse === undefined ? {} : { refuse }),
          });
          messages.push({
            role: "tool",
            toolCallId: call.id,
            content:
              mode === "native" ? fenceToolResult(call.name, record.status, content) : content,
          });
        }
        if (final) break;
        round += 1;
      }
    } catch (error) {
      for (const call of calls)
        if (call.status === "running" || call.status === "awaiting-approval") {
          call.status = "cancelled";
          call.error = context.signal.aborted
            ? "Выполнение отменено"
            : error instanceof Error
              ? error.message
              : String(error);
        }
      this.saveToolCalls(context, assistantId, calls);
      throw error;
    }
    return { text, reasoning };
  }
  private async generateRound(
    context: StepContext,
    driver: TextGenerationDriver,
    request: GenerateRequest,
    before: string,
  ): Promise<RoundOutput> {
    let text = "";
    let reasoning = "";
    const toolCalls: ToolCall[] = [];
    for await (const delta of driver.stream(request, context.signal)) {
      context.signal.throwIfAborted();
      if (delta.kind === "tool_call") {
        toolCalls.push({
          ...delta.call,
          id: delta.call.id || `call_${String(toolCalls.length + 1)}`,
        });
        continue;
      }
      if (delta.text.length === 0) continue;
      if (delta.kind === "reasoning") {
        reasoning += delta.text;
        context.emit({ type: "token", delta: delta.text, kind: "reasoning" });
        continue;
      }
      if (text === "" && before !== "" && !before.endsWith("\n")) {
        text += "\n\n";
        context.emit({ type: "token", delta: "\n\n", kind: "text" });
      }
      text += delta.text;
      context.emit({ type: "token", delta: delta.text, kind: "text" });
    }
    return { text, reasoning, toolCalls };
  }
  private async callTool(
    context: StepContext,
    assistantId: string,
    calls: ChatToolCallDto[],
    record: ChatToolCallDto,
    options: { tool: OfferedTool | undefined; label: string; limit: number; refuse?: string },
  ): Promise<string> {
    const started = this.clock();
    const settle = (
      status: ChatToolCallDto["status"],
      error: string | null,
      result: string | null,
    ): string => {
      const preview = result === null ? null : capText(result, TOOL_PREVIEW_CHARS);
      record.status = status;
      record.error = error;
      record.result = preview?.text ?? null;
      record.truncated = preview?.truncated ?? false;
      record.durationMs = Math.max(0, Math.round(this.clock() - started));
      this.saveToolCalls(context, assistantId, calls);
      if (result === null) return `Tool call ${status}: ${error ?? "no result"}`;
      const capped = capText(result, options.limit);
      return capped.truncated
        ? `${capped.text}\n[result truncated to ${String(options.limit)} characters]`
        : capped.text;
    };
    this.saveToolCalls(context, assistantId, calls);
    if (options.refuse !== undefined) return settle("refused", options.refuse, null);
    if (options.tool === undefined)
      return settle("refused", "Модель вызвала инструмент, который не был ей предложен", null);
    if (record.arguments === null || typeof record.arguments !== "object")
      return settle("failed", "Аргументы вызова не являются JSON-объектом", null);
    if (context.invokeTool === undefined)
      return settle("refused", "Вызов инструментов недоступен в этом запуске", null);
    try {
      const { stepId, output } = await context.invokeTool({
        key: options.tool.key,
        input: record.arguments,
        label: options.label,
        ...(options.tool.destructive ? { minimumTier: "ask" as const } : {}),
        onApproval: (approval, decision) => {
          record.approvalId = typeof approval.id === "string" ? approval.id : null;
          record.status = decision === null ? "awaiting-approval" : "running";
          this.saveToolCalls(context, assistantId, calls);
        },
      });
      record.stepId = stepId;
      const result = toolResultText(output);
      return settle(
        result.isError ? "failed" : "succeeded",
        result.isError ? "Инструмент вернул ошибку" : null,
        result.text,
      );
    } catch (error) {
      if (context.signal.aborted) throw error;
      if (error instanceof AppError && error.code === AppErrorCode.APPROVAL_DENIED)
        return settle("denied", "Вызов отклонён пользователем", null);
      if (error instanceof AppError && error.code === AppErrorCode.PERMISSION_DENIED)
        return settle("refused", "Инструмент запрещён разрешением", null);
      return settle("failed", error instanceof Error ? error.message : String(error), null);
    }
  }
  private saveToolCalls(
    context: StepContext,
    assistantId: string,
    calls: readonly ChatToolCallDto[],
  ): void {
    if (calls.length === 0) return;
    const snapshot = calls.map((call) => ({ ...call }));
    this.options.data.repositories.chat.updateMessage(assistantId, { toolCalls: snapshot });
    context.emit({ type: "step", step: { domain: "tool-calls", calls: snapshot } });
  }
  private requireIdle(id: string): void {
    if (
      this.options.data.repositories.runs
        .unfinished()
        .some((run) => run.kind === "chat" && run.subjectId === id)
    )
      throw new AppError(AppErrorCode.CONFLICT, "Conversation already has an active turn");
  }
  private requireTurn(runId: string, conversationId: string, userId: string): void {
    const run = this.options.runs.get(runId);
    const user = this.options.data.repositories.chat.getMessage(userId);
    if (
      run.kind !== "chat" ||
      run.subjectId !== conversationId ||
      user?.runId !== runId ||
      user.role !== "user"
    )
      throw new AppError(AppErrorCode.CONFLICT, "Invalid chat turn");
  }
  private resolveModel(
    conversation: CreateConversationInput,
    providerId: string = conversation.providerId,
    modelId = conversation.modelId,
    settings = conversation.settings,
  ) {
    const repositories = this.options.data.repositories;
    const provider = repositories.providers.findById(providerId);
    if (!provider?.enabled || !provider.capabilities.includes("text"))
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Text provider unavailable");
    const model = repositories.models
      .listByProvider(provider.id)
      .find((model) => model.id === modelId || model.externalId === modelId);
    if (!model?.available)
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Text model unavailable");
    const supportsTools = modelToolSupport(model, repositories.models.listByProvider(provider.id));
    const contextWindow = model.contextWindow ?? 8192;
    const maxOutputTokens =
      settings.maxOutputTokens ??
      Math.min(model.maxOutput ?? 1024, Math.max(1, Math.floor(contextWindow / 4)));
    if (
      maxOutputTokens >= contextWindow ||
      (model.maxOutput !== null && maxOutputTokens > model.maxOutput)
    )
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Output budget exceeds model limits");
    return { externalId: model.externalId, contextWindow, maxOutputTokens, supportsTools };
  }
}
