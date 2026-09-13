import type { ModelCapability } from "../../../data/schema/index.ts";
import type { Logger } from "../../../platform/logger.ts";
import { cancelled, isCancellation, toAppError } from "../errors.ts";
import type {
  DiscoveredModel,
  GenerateRequest,
  GenerateResult,
  TextDelta,
  TextGenerationDriver,
  TokenUsage,
} from "../ports.ts";
import type { RequestSpec, Transport } from "../transport/Transport.ts";

export const OLLAMA_CAPABILITIES = {
  family: "openai-compatible" as const,
  authModes: ["api"] as const,
  streaming: true,
  liveModelList: true,
  embedding: false,
  image: false,
  honours: { temperature: true, topK: true, topP: true, maxOutputTokens: true },
};

type OllamaMessage = { role?: unknown; content?: unknown; thinking?: unknown };
type OllamaFrame = {
  model?: unknown;
  message?: OllamaMessage;
  done?: unknown;
  done_reason?: unknown;
  prompt_eval_count?: unknown;
  eval_count?: unknown;
};

export class OllamaAdapter implements TextGenerationDriver {
  constructor(
    private readonly transport: Transport,
    private readonly logger?: Logger,
  ) {}

  capabilities() {
    return OLLAMA_CAPABILITIES;
  }

  async listModels(signal: AbortSignal): Promise<DiscoveredModel[]> {
    const response = await this.request({ method: "GET", path: "/tags" }, signal);
    const payload = JSON.parse(response.text) as { models?: Array<Record<string, unknown>> };
    return (payload.models ?? []).flatMap((entry) => {
      const name = typeof entry.name === "string" ? entry.name.trim() : "";
      if (!name) return [];
      const details =
        typeof entry.details === "object" && entry.details !== null ? entry.details : {};
      const family =
        typeof (details as Record<string, unknown>).family === "string"
          ? ((details as Record<string, unknown>).family as string)
          : null;
      const size = typeof entry.size === "number" ? entry.size : null;
      const capabilities: ModelCapability[] = ["streaming"];
      return [
        {
          externalId: name,
          displayName: name,
          family,
          contextWindow: null,
          maxOutput: null,
          sizeBytes: size,
          capabilities,
        },
      ];
    });
  }

  async generate(request: GenerateRequest, signal: AbortSignal): Promise<GenerateResult> {
    let text = "";
    let reasoning = "";
    let model = request.model;
    let usage: TokenUsage | null = null;
    let finishReason: GenerateResult["finishReason"] = "unknown";
    for await (const delta of this.streamWithSummary(request, signal, (frame) => {
      if (typeof frame.model === "string") model = frame.model;
      usage = usageOf(frame) ?? usage;
      if (frame.done === true) finishReason = frame.done_reason === "length" ? "length" : "stop";
    })) {
      if (delta.kind === "reasoning") reasoning += delta.text;
      else text += delta.text;
    }
    return { model, text, reasoning: reasoning || null, finishReason, usage };
  }

  stream(request: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta> {
    return this.streamWithSummary(request, signal);
  }

  private async *streamWithSummary(
    request: GenerateRequest,
    signal: AbortSignal,
    observe?: (frame: OllamaFrame) => void,
  ): AsyncIterable<TextDelta> {
    const spec: RequestSpec = {
      method: "POST",
      path: "/chat",
      accept: "application/x-ndjson",
      body: {
        model: request.model,
        messages: [
          ...(request.system ? [{ role: "system", content: request.system }] : []),
          ...request.messages,
        ],
        stream: true,
        options: {
          ...(request.temperature === undefined ? {} : { temperature: request.temperature }),
          ...(request.topK === undefined ? {} : { top_k: request.topK }),
          ...(request.topP === undefined ? {} : { top_p: request.topP }),
          ...(request.maxOutputTokens === undefined
            ? {}
            : { num_predict: request.maxOutputTokens }),
        },
      },
    };
    try {
      for await (const line of ndjson(this.transport.stream(spec, signal))) {
        if (signal.aborted) throw cancelled();
        const frame = JSON.parse(line) as OllamaFrame;
        observe?.(frame);
        const message = frame.message;
        if (typeof message?.thinking === "string" && message.thinking.length > 0)
          yield { text: message.thinking, kind: "reasoning" };
        if (typeof message?.content === "string" && message.content.length > 0)
          yield { text: message.content, kind: "text" };
      }
    } catch (error: unknown) {
      throw isCancellation(error) ? cancelled() : toAppError(error);
    }
  }

  private async request(spec: RequestSpec, signal: AbortSignal) {
    try {
      return await this.transport.request(spec, signal);
    } catch (error: unknown) {
      this.logger?.log("debug", "ai", "Ollama request failed", { path: spec.path });
      throw isCancellation(error) ? cancelled() : toAppError(error);
    }
  }
}

async function* ndjson(chunks: AsyncIterable<Uint8Array>): AsyncIterable<string> {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of chunks) {
    buffer += decoder.decode(chunk, { stream: true });
    const lines = buffer.split(/\r?\n/);
    buffer = lines.pop() ?? "";
    for (const line of lines) if (line.trim()) yield line.trim();
  }
  buffer += decoder.decode();
  if (buffer.trim()) yield buffer.trim();
}

function usageOf(frame: OllamaFrame): TokenUsage | null {
  const promptTokens = numberOrNull(frame.prompt_eval_count);
  const completionTokens = numberOrNull(frame.eval_count);
  if (promptTokens === null && completionTokens === null) return null;
  return {
    promptTokens,
    completionTokens,
    totalTokens:
      promptTokens === null || completionTokens === null ? null : promptTokens + completionTokens,
  };
}

function numberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}
