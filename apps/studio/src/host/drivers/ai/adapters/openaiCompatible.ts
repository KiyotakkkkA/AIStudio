import type { ModelCapability } from "../../../data/schema/index.ts";
import type { Logger } from "../../../platform/logger.ts";
import {
  unsupportedParameters,
  type AdapterCapabilities,
  type TunableParameter,
} from "../AdapterCapabilities.ts";
import { cancelled, isCancellation, isNotFoundResponse, toAppError } from "../errors.ts";
import type {
  DiscoveredModel,
  EmbeddingDriver,
  FinishReason,
  GenerateRequest,
  GenerateResult,
  TextDelta,
  TextGenerationDriver,
  TokenUsage,
} from "../ports.ts";
import type { RequestSpec, Transport, TransportResponse } from "../transport/Transport.ts";
import { sseData } from "./sse.ts";

export const OPENAI_COMPATIBLE_CAPABILITIES: AdapterCapabilities = {
  family: "openai-compatible",
  authModes: ["api"],
  streaming: true,
  liveModelList: true,
  embedding: true,
  image: false,
  honours: { temperature: true, topK: false, topP: true, maxOutputTokens: true },
};

interface ChatContent {
  content?: unknown;
  reasoning_content?: unknown;
  reasoning?: unknown;
}

interface ChatChoice {
  message?: ChatContent;
  delta?: ChatContent;
  finish_reason?: unknown;
}

interface ChatResponse {
  model?: unknown;
  choices?: ChatChoice[];
  usage?: Record<string, unknown>;
}

interface ModelListEntry {
  id?: unknown;
  model?: unknown;
  name?: unknown;
  display_name?: unknown;
  details?: { family?: unknown; parameter_size?: unknown };
  context_length?: unknown;
  context_window?: unknown;
  max_output_tokens?: unknown;
  owned_by?: unknown;
  size?: unknown;
  architecture?: { input_modalities?: unknown; modality?: unknown };
  supported_parameters?: unknown;
  top_provider?: { context_length?: unknown; max_completion_tokens?: unknown };
}

interface ModelListResponse {
  data?: ModelListEntry[];
  models?: ModelListEntry[];
}

const MODEL_LIST_PATHS = ["/models", "/tags"] as const;

interface EmbeddingResponse {
  data?: { embedding?: unknown }[];
}

export interface OpenAiCompatibleOptions {
  transport: Transport;
  logger?: Logger;
}

export class OpenAiCompatibleAdapter implements TextGenerationDriver, EmbeddingDriver {
  readonly #transport: Transport;
  readonly #logger: Logger | undefined;
  #dimensions: number | null = null;

  constructor(options: OpenAiCompatibleOptions) {
    this.#transport = options.transport;
    this.#logger = options.logger;
  }

  capabilities(): AdapterCapabilities {
    return OPENAI_COMPATIBLE_CAPABILITIES;
  }

  dimensions(): number | null {
    return this.#dimensions;
  }

  async generate(request: GenerateRequest, signal: AbortSignal): Promise<GenerateResult> {
    const response = await this.#post("/chat/completions", this.#body(request, false), signal);
    const payload = decode<ChatResponse>(response.text);
    const choice = payload.choices?.[0];
    return {
      model: typeof payload.model === "string" ? payload.model : request.model,
      text: typeof choice?.message?.content === "string" ? choice.message.content : "",
      reasoning: reasoningOf(choice?.message),
      finishReason: finishReason(choice?.finish_reason),
      usage: usage(payload.usage),
    };
  }

  stream(request: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta> {
    const spec = specOf("/chat/completions", this.#body(request, true));
    const transport = this.#transport;
    return {
      async *[Symbol.asyncIterator](): AsyncIterator<TextDelta> {
        try {
          for await (const payload of sseData(transport.stream(spec, signal))) {
            if (signal.aborted) throw cancelled();
            const delta = tryDecode<ChatResponse>(payload)?.choices?.[0]?.delta;
            const reasoning = reasoningOf(delta);
            if (reasoning !== null) yield { text: reasoning, kind: "reasoning" };
            const content = delta?.content;
            if (typeof content === "string" && content.length > 0) {
              yield { text: content, kind: "text" };
            }
          }
        } catch (error: unknown) {
          throw isCancellation(error) ? cancelled() : toAppError(error);
        }
      },
    };
  }

  async listModels(signal: AbortSignal): Promise<DiscoveredModel[]> {
    const payload = await this.#modelList(signal);
    const entries = payload.data ?? payload.models ?? [];
    const seen = new Set<string>();
    const discovered: DiscoveredModel[] = [];
    for (const entry of entries) {
      const externalId = firstString(entry.id, entry.model, entry.name) ?? "";
      if (externalId.length === 0 || seen.has(externalId)) continue;
      seen.add(externalId);
      discovered.push(toDiscoveredModel(externalId, entry));
    }
    return discovered;
  }

  async #modelList(signal: AbortSignal): Promise<ModelListResponse> {
    for (const [index, path] of MODEL_LIST_PATHS.entries()) {
      try {
        const response = await this.#request({ method: "GET", path }, signal);
        return decode<ModelListResponse>(response.text);
      } catch (error: unknown) {
        if (index === MODEL_LIST_PATHS.length - 1 || !isNotFoundResponse(error)) throw error;
        this.#logger?.log("debug", "ai", "Model list endpoint is absent, trying the next one", {
          baseUrl: this.#transport.baseUrl,
          path,
        });
      }
    }
    return {};
  }

  async embed(
    texts: readonly string[],
    model: string,
    signal: AbortSignal,
  ): Promise<Float32Array[]> {
    if (texts.length === 0) return [];
    const response = await this.#post("/embeddings", { model, input: [...texts] }, signal);
    const payload = decode<EmbeddingResponse>(response.text);
    const vectors = (payload.data ?? []).map((entry) => toVector(entry.embedding));
    this.#dimensions = vectors[0]?.length ?? this.#dimensions;
    return vectors;
  }

  #body(request: GenerateRequest, stream: boolean): Record<string, unknown> {
    this.#dropUnsupported(request);
    const messages = [
      ...(request.system === undefined ? [] : [{ role: "system", content: request.system }]),
      ...request.messages.map((message) => ({ role: message.role, content: message.content })),
    ];
    const body: Record<string, unknown> = { model: request.model, messages, stream };
    if (request.temperature !== undefined) body.temperature = request.temperature;
    if (request.topP !== undefined) body.top_p = request.topP;
    if (request.maxOutputTokens !== undefined) body.max_tokens = request.maxOutputTokens;
    return body;
  }

  #dropUnsupported(request: GenerateRequest): void {
    const dropped: TunableParameter[] = unsupportedParameters(OPENAI_COMPATIBLE_CAPABILITIES, {
      temperature: request.temperature,
      topK: request.topK,
      topP: request.topP,
      maxOutputTokens: request.maxOutputTokens,
    });
    if (dropped.length === 0) return;
    this.#logger?.log("debug", "ai", "Dropped parameters the adapter family does not honour", {
      family: OPENAI_COMPATIBLE_CAPABILITIES.family,
      dropped,
    });
  }

  async #post(
    path: string,
    body: Record<string, unknown>,
    signal: AbortSignal,
  ): Promise<TransportResponse> {
    return await this.#request(specOf(path, body), signal);
  }

  async #request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse> {
    try {
      return await this.#transport.request(spec, signal);
    } catch (error: unknown) {
      throw isCancellation(error) ? cancelled() : toAppError(error);
    }
  }
}

function specOf(path: string, body: Record<string, unknown>): RequestSpec {
  return { method: "POST", path, body };
}

function toDiscoveredModel(externalId: string, entry: ModelListEntry): DiscoveredModel {
  return {
    externalId,
    displayName: firstString(entry.display_name, entry.name) ?? externalId,
    family: firstString(entry.owned_by, entry.details?.family) ?? familyOf(externalId),
    contextWindow: firstNumber(
      entry.context_length,
      entry.context_window,
      entry.top_provider?.context_length,
    ),
    maxOutput: firstNumber(entry.max_output_tokens, entry.top_provider?.max_completion_tokens),
    sizeBytes: firstNumber(entry.size),
    capabilities: modelCapabilities(entry),
  };
}

function modelCapabilities(entry: ModelListEntry): ModelCapability[] {
  const capabilities: ModelCapability[] = ["streaming"];
  const parameters = Array.isArray(entry.supported_parameters)
    ? entry.supported_parameters.filter((value): value is string => typeof value === "string")
    : [];
  if (parameters.includes("tools") || parameters.includes("tool_choice")) {
    capabilities.push("tools");
  }
  if (parameters.includes("reasoning") || parameters.includes("include_reasoning")) {
    capabilities.push("reasoning");
  }
  const modalities = entry.architecture?.input_modalities;
  const modality = entry.architecture?.modality;
  if (
    (Array.isArray(modalities) && modalities.includes("image")) ||
    (typeof modality === "string" && modality.includes("image"))
  ) {
    capabilities.push("vision");
  }
  return capabilities;
}

function familyOf(externalId: string): string | null {
  const [family] = externalId.split(/[/:]/);
  return family === undefined || family === externalId ? null : family;
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function firstNumber(...values: unknown[]): number | null {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) return Math.trunc(value);
  }
  return null;
}

function finishReason(raw: unknown): FinishReason {
  if (raw === "stop" || raw === "end_turn") return "stop";
  if (raw === "length" || raw === "max_tokens") return "length";
  if (raw === "cancelled" || raw === "aborted") return "cancelled";
  return "unknown";
}

function reasoningOf(content: ChatContent | undefined): string | null {
  const raw = content?.reasoning_content ?? content?.reasoning;
  return typeof raw === "string" && raw.length > 0 ? raw : null;
}

function usage(raw: Record<string, unknown> | undefined): TokenUsage | null {
  if (raw === undefined) return null;
  return {
    promptTokens: firstNumber(raw.prompt_tokens, raw.input_tokens),
    completionTokens: firstNumber(raw.completion_tokens, raw.output_tokens),
    totalTokens: firstNumber(raw.total_tokens),
  };
}

function toVector(raw: unknown): Float32Array {
  if (!Array.isArray(raw)) return new Float32Array(0);
  return Float32Array.from(raw.filter((value): value is number => typeof value === "number"));
}

function decode<T>(text: string): T {
  const parsed = tryDecode<T>(text);
  if (parsed === null) {
    throw toAppError(new Error("Provider returned a body that is not valid JSON"));
  }
  return parsed;
}

function tryDecode<T>(text: string): T | null {
  try {
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
}
