import { z } from "zod";
import {
  AppError,
  AppErrorCode,
  ChatSendInput,
  ConversationDetailDto,
  ConversationDto,
  CreateConversationInput,
  MessageDto,
  StartRunInput,
  VectorSearchHitDto,
  type HostEvent,
  type RunHandleDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import { GenerateNodeInput } from "../kernel/coreNodes.ts";
import type { NodeRegistry } from "../kernel/NodeRegistry.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { RunService } from "./RunService.ts";
import { estimateTokens, windowChatHistory } from "./chatHistory.ts";

export interface ChatServiceOptions {
  data: UnitOfWork;
  runs: RunService;
  registry: NodeRegistry;
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
          settings: z.object({
            temperature: z.number().optional(),
            topK: z.number().int().positive().optional(),
            topP: z.number().optional(),
            maxOutputTokens: z.number().int().positive().optional(),
          }),
        })
        .catchall(z.array(VectorSearchHitDto)),
      output: GenerateNodeInput,
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
        const sources = conversation.attachedStoreIds.flatMap((storeId, index) =>
          (input[`store${index}`] ?? []).map((hit) => ({ storeId, hit })),
        );
        const prepared = windowChatHistory(
          conversation.systemPrompt,
          history,
          user,
          model.contextWindow,
          model.maxOutputTokens,
          sources,
        );
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
          messages: prepared.messages,
          system: conversation.systemPrompt,
          ...input.settings,
          maxOutputTokens: model.maxOutputTokens,
        };
      },
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
    return ConversationDto.parse(
      this.options.data.repositories.chat.create({
        ...input,
        attachedStoreIds: [...new Set(input.attachedStoreIds)],
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
    const retrieval = conversation.attachedStoreIds.map((storeId, index) => ({
      id: `store${index}`,
      type: "vector.search",
      input: { storeId, query: input.text, k: 5 },
    }));
    const graph = {
      nodes: [
        ...retrieval,
        {
          id: "prepare",
          type: "chat.prepare",
          dependencies: retrieval.map((node) => node.id),
          input: { conversationId, userId, assistantId, providerId, modelId, settings },
          bindings: Object.fromEntries(
            retrieval.map((node) => [node.id, { source: "node", nodeId: node.id }]),
          ),
        },
        {
          id: "generate",
          type: "llm.generate",
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
        StartRunInput.parse({ kind: "chat", subjectId: conversationId, graph }),
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
      repository.update(conversation.id, {
        updatedAt: this.clock(),
        ...(conversation.title === "New conversation" && first && message.content
          ? { title: first.content.replace(/\s+/g, " ").slice(0, 80) }
          : {}),
      });
    }
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
    const contextWindow = model.contextWindow ?? 8192;
    const maxOutputTokens =
      settings.maxOutputTokens ??
      Math.min(model.maxOutput ?? 1024, Math.max(1, Math.floor(contextWindow / 4)));
    if (
      maxOutputTokens >= contextWindow ||
      (model.maxOutput !== null && maxOutputTokens > model.maxOutput)
    )
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Output budget exceeds model limits");
    return { externalId: model.externalId, contextWindow, maxOutputTokens };
  }
}
