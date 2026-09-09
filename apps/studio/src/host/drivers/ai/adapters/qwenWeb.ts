import { AppError, AppErrorCode } from "@zvs/shared";
import { z } from "zod";
import type { ModelCapability } from "../../../data/schema/index.ts";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import { sessionExpired } from "../errors.ts";
import type {
  DiscoveredModel,
  FinishReason,
  GenerateRequest,
  GenerateResult,
  TextDelta,
  TokenUsage,
} from "../ports.ts";
import type { RequestSpec } from "../transport/Transport.ts";
import {
  AccountFamilyAdapter,
  flattenPrompt,
  malformedPayload,
  malformedTurn,
  truncatedStream,
  type VendorObservation,
} from "./accountFamily.ts";

export const QWEN_MODELS_PATH = "/models/";
export const QWEN_CHATS_NEW_PATH = "/chats/new";
export const QWEN_COMPLETIONS_PATH = "/chat/completions";

export const QWEN_CHAT_MODE = "local";
export const QWEN_CHAT_TYPE = "t2t";
export const QWEN_API_VERSION = "2.1";

export const QWEN_MODELS_OBSERVATION: VendorObservation = {
  source: "https://chat.qwen.ai/api/v2/models/",
  observedAt: "2026-09-10",
};

export const QWEN_GENERATION_OBSERVATION: VendorObservation = {
  source:
    "https://chat.qwen.ai/api/v2/chats/new then POST /api/v2/chat/completions?chat_id=… (SSE, one vendor chat per generation)",
  observedAt: "2026-09-10",
};

export const QWEN_WEB_CAPABILITIES: AdapterCapabilities = {
  family: "qwen-web",
  authModes: ["account"],
  streaming: true,
  liveModelList: true,
  embedding: false,
  image: false,
  honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
};

const positive = z.number().int().positive().nullish();
const flag = z.union([z.boolean(), z.number()]).nullish();

const QwenModelMeta = z.object({
  max_context_length: positive,
  max_generation_length: positive,
  modality: z.array(z.string()).nullish(),
  capabilities: z
    .object({ vision: flag, document: flag, video: flag, audio: flag, thinking: flag })
    .nullish(),
  abilities: z
    .object({ vision: flag, document: flag, video: flag, audio: flag, thinking: flag })
    .nullish(),
});

const QwenModelInfo = z.object({
  name: z.string().nullish(),
  is_active: z.boolean().nullish(),
  meta: QwenModelMeta.nullish(),
});

const QwenModelEntry = z.object({
  id: z.string(),
  name: z.string().nullish(),
  owned_by: z.string().nullish(),
  info: QwenModelInfo.nullish(),
});

const QwenModelList = z.array(QwenModelEntry);

export const QwenModelsPayload = z.object({
  success: z.boolean().nullish(),
  data: z.union([QwenModelList, z.object({ data: QwenModelList })]).nullish(),
});

export const QwenChatCreated = z.object({
  success: z.boolean().nullish(),
  id: z.string().nullish(),
  data: z.object({ id: z.string().nullish() }).nullish(),
});

const QwenSummary = z.object({ content: z.array(z.string()).nullish() }).nullish();

const QwenDelta = z.object({
  role: z.string().nullish(),
  content: z.string().nullish(),
  phase: z.string().nullish(),
  status: z.string().nullish(),
  extra: z.object({ summary_title: QwenSummary, summary_thought: QwenSummary }).loose().nullish(),
});

const QwenUsage = z.object({
  input_tokens: z.number().int().nonnegative().nullish(),
  output_tokens: z.number().int().nonnegative().nullish(),
  total_tokens: z.number().int().nonnegative().nullish(),
});

const QwenVendorError = z.union([
  z.string(),
  z.object({
    code: z.union([z.string(), z.number()]).nullish(),
    message: z.string().nullish(),
    status: z.number().int().nullish(),
  }),
]);

export const QwenStreamFrame = z.object({
  choices: z.array(z.object({ delta: QwenDelta.nullish() })).nullish(),
  usage: QwenUsage.nullish(),
  success: z.boolean().nullish(),
  status: z.number().int().nullish(),
  code: z.union([z.string(), z.number()]).nullish(),
  error: QwenVendorError.nullish(),
});

type QwenEntry = z.infer<typeof QwenModelEntry>;
type QwenFrame = z.infer<typeof QwenStreamFrame>;

interface TurnSummary {
  usage: TokenUsage | null;
  finishReason: FinishReason;
}

export function mapQwenModels(payload: unknown): DiscoveredModel[] | null {
  const parsed = QwenModelsPayload.safeParse(payload);
  if (!parsed.success) return null;
  const { success, data } = parsed.data;
  if (success === false || data == null) return null;
  const entries = Array.isArray(data) ? data : data.data;

  const seen = new Set<string>();
  const models: DiscoveredModel[] = [];
  for (const entry of entries) {
    const externalId = entry.id.trim();
    if (externalId.length === 0 || seen.has(externalId)) continue;
    if (entry.info?.is_active === false) continue;
    seen.add(externalId);
    models.push(toDiscoveredModel(externalId, entry));
  }
  return models;
}

export class QwenWebAdapter extends AccountFamilyAdapter {
  #models: DiscoveredModel[] | undefined;

  capabilities(): AdapterCapabilities {
    return QWEN_WEB_CAPABILITIES;
  }

  async listModels(signal: AbortSignal): Promise<DiscoveredModel[]> {
    const response = await this.request({ method: "GET", path: QWEN_MODELS_PATH }, signal);
    const models = mapQwenModels(this.decode(response.text, QWEN_MODELS_PATH));
    if (models === null) throw malformedPayload("qwen-web", QWEN_MODELS_PATH, this.logger);
    this.#models = models;
    return models;
  }

  async generate(request: GenerateRequest, signal: AbortSignal): Promise<GenerateResult> {
    const summary: TurnSummary = { usage: null, finishReason: "unknown" };
    let text = "";
    let reasoning = "";
    for await (const delta of this.#run(request, signal, summary)) {
      if (delta.kind === "reasoning") reasoning += delta.text;
      else text += delta.text;
    }
    return {
      model: request.model,
      text,
      reasoning: reasoning.length === 0 ? null : reasoning,
      finishReason: summary.finishReason,
      usage: summary.usage,
    };
  }

  stream(request: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta> {
    return this.#run(request, signal);
  }

  async *#run(
    request: GenerateRequest,
    signal: AbortSignal,
    summary?: TurnSummary,
  ): AsyncIterable<TextDelta> {
    this.dropUnsupported(request);
    const models = this.#models ?? (await this.listModels(signal));
    const thinking =
      models
        .find((model) => model.externalId === request.model)
        ?.capabilities.includes("reasoning") ?? false;
    const chatId = await this.#createChat(request.model, signal);
    const emitted: string[] = [];
    let finished = false;

    for await (const payload of this.frames(this.#turn(chatId, request, thinking), signal)) {
      const parsed = QwenStreamFrame.safeParse(parse(payload));
      if (!parsed.success) continue;
      const frame = parsed.data;
      const failure = vendorFailure(frame);
      if (failure !== null) throw this.normalise(failure);

      if (summary !== undefined) summary.usage = toUsage(frame.usage) ?? summary.usage;
      const delta = frame.choices?.[0]?.delta;
      if (delta == null) continue;

      if (delta.phase === "thinking_summary") {
        for (const chunk of summaryDeltas(emitted, delta.extra?.summary_thought?.content ?? [])) {
          yield { text: chunk, kind: "reasoning" };
        }
        continue;
      }
      if (delta.phase !== "answer") continue;
      const content = delta.content ?? "";
      if (content.length > 0) yield { text: content, kind: "text" };
      if (delta.status === "finished") finished = true;
    }

    if (!finished) throw truncatedStream("qwen-web", QWEN_COMPLETIONS_PATH);
    if (summary !== undefined) summary.finishReason = "stop";
  }

  async #createChat(model: string, signal: AbortSignal): Promise<string> {
    const response = await this.request(
      {
        method: "POST",
        path: QWEN_CHATS_NEW_PATH,
        body: {
          chatId: "",
          chat_mode: QWEN_CHAT_MODE,
          chat_type: QWEN_CHAT_TYPE,
          models: [model],
          project_id: "",
          timestamp: this.clock(),
        },
      },
      signal,
    );
    const parsed = QwenChatCreated.safeParse(this.decode(response.text, QWEN_CHATS_NEW_PATH));
    if (!parsed.success || parsed.data.success === false) {
      throw malformedTurn("qwen-web", QWEN_CHATS_NEW_PATH, this.logger);
    }
    const chatId = (parsed.data.data?.id ?? parsed.data.id ?? "").trim();
    if (chatId.length === 0) throw malformedTurn("qwen-web", QWEN_CHATS_NEW_PATH, this.logger);
    return chatId;
  }

  #turn(chatId: string, request: GenerateRequest, thinking: boolean): RequestSpec {
    const timestamp = Math.floor(this.clock() / 1000);
    const message = {
      id: null,
      fid: this.newId(),
      parentId: null,
      parent_id: null,
      childrenIds: [],
      role: "user",
      // Each generation is stateless: history is role-labelled text in a fresh vendor chat.
      content: flattenPrompt(request),
      user_action: "chat",
      files: [],
      timestamp,
      model: "",
      models: [request.model],
      chat_type: QWEN_CHAT_TYPE,
      sub_chat_type: QWEN_CHAT_TYPE,
      feature_config: {
        thinking_enabled: thinking,
        thinking_format: "summary",
        thinking_mode: thinking ? "Thinking" : "Fast",
        auto_thinking: false,
        auto_search: false,
        output_schema: "phase",
        research_mode: "normal",
      },
      extra: { meta: { subChatType: QWEN_CHAT_TYPE } },
    };
    return {
      method: "POST",
      path: QWEN_COMPLETIONS_PATH,
      query: { chat_id: chatId },
      accept: "text/event-stream",
      body: {
        chatId,
        chat_id: chatId,
        chat_mode: QWEN_CHAT_MODE,
        chat_type: QWEN_CHAT_TYPE,
        incremental_output: true,
        messages: [message],
        model: request.model,
        parentId: null,
        parent_id: null,
        stream: true,
        timestamp,
        version: QWEN_API_VERSION,
      },
    };
  }
}

function vendorFailure(frame: QwenFrame): AppError | null {
  const nested = typeof frame.error === "object" && frame.error !== null ? frame.error : null;
  const code = nested?.code ?? frame.code ?? null;
  const status = nested?.status ?? frame.status ?? null;
  const details = {
    family: "qwen-web",
    endpoint: QWEN_COMPLETIONS_PATH,
    ...(code === null ? {} : { vendorCode: code }),
    ...(status === null ? {} : { status }),
  };
  if (status === 401 || status === 403 || isAuthCode(code)) return sessionExpired(details);
  if (frame.success !== false && frame.error == null && (status === null || status < 400)) {
    return null;
  }
  return new AppError(AppErrorCode.UNKNOWN, "Вендор прервал генерацию", { details });
}

function isAuthCode(code: string | number | null): boolean {
  if (code === 401 || code === 403 || code === "401" || code === "403") return true;
  return typeof code === "string" && /unauth|forbidden|token|login|expire|session/i.test(code);
}

function summaryDeltas(emitted: string[], next: readonly string[]): string[] {
  const chunks: string[] = [];
  for (let index = 0; index < next.length; index += 1) {
    const value = next[index] ?? "";
    const seen = emitted[index] ?? "";
    if (value === seen) continue;
    chunks.push(value.startsWith(seen) ? value.slice(seen.length) : value);
    emitted[index] = value;
  }
  return chunks;
}

function toUsage(raw: z.infer<typeof QwenUsage> | null | undefined): TokenUsage | null {
  if (raw == null) return null;
  return {
    promptTokens: raw.input_tokens ?? null,
    completionTokens: raw.output_tokens ?? null,
    totalTokens: raw.total_tokens ?? null,
  };
}

function parse(payload: string): unknown {
  try {
    return JSON.parse(payload) as unknown;
  } catch {
    return null;
  }
}

function toDiscoveredModel(externalId: string, entry: QwenEntry): DiscoveredModel {
  const meta = entry.info?.meta;
  return {
    externalId,
    displayName: text(entry.name) ?? text(entry.info?.name) ?? externalId,
    family: text(entry.owned_by),
    contextWindow: meta?.max_context_length ?? null,
    maxOutput: meta?.max_generation_length ?? null,
    sizeBytes: null,
    capabilities: modelCapabilities(entry),
  };
}

function modelCapabilities(entry: QwenEntry): ModelCapability[] {
  const meta = entry.info?.meta;
  const capabilities: ModelCapability[] = ["streaming"];
  // Advertise only implemented request paths. Files/media are not sent by this adapter.
  // Explicit capabilities override legacy abilities, including false/zero.
  if (enabled(meta?.capabilities?.thinking ?? meta?.abilities?.thinking)) {
    capabilities.push("reasoning");
  }
  return capabilities;
}

function enabled(...values: readonly (boolean | number | null | undefined)[]): boolean {
  return values.some((value) => value === true || (typeof value === "number" && value > 0));
}

function text(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed.length === 0 ? null : trimmed;
}
