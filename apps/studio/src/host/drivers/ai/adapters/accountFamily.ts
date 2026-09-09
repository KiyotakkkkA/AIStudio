import { AppError, AppErrorCode, type AdapterFamily } from "@zvs/shared";
import type { Logger } from "../../../platform/logger.ts";
import {
  unsupportedParameters,
  type AdapterCapabilities,
  type TunableParameter,
} from "../AdapterCapabilities.ts";
import { cancelled, isCancellation, isSessionExpired, toAppError } from "../errors.ts";
import type {
  ChatRole,
  DiscoveredModel,
  GenerateRequest,
  GenerateResult,
  TextDelta,
  TextGenerationDriver,
} from "../ports.ts";
import type { RequestSpec, Transport, TransportResponse } from "../transport/Transport.ts";
import { sseData } from "./sse.ts";

export const ACCOUNT_GENERATION_PENDING =
  "account-mode generation is not implemented yet (TASK_014_EXTRA_3)";

export const ROLE_LABELS: Readonly<Record<ChatRole, string>> = {
  system: "System",
  user: "User",
  assistant: "Assistant",
};

export interface AccountAdapterOptions {
  transport: Transport;
  logger?: Logger;
  clock?: () => number;
  newId?: () => string;
  onSessionExpired?: (error: AppError) => void;
}

export interface VendorObservation {
  readonly source: string;
  readonly observedAt: string;
}

export function generationNotImplemented(family: AdapterFamily, model: string): AppError {
  return new AppError(AppErrorCode.UNKNOWN, ACCOUNT_GENERATION_PENDING, {
    details: { family, model },
  });
}

export function failingStream(error: AppError): AsyncIterable<TextDelta> {
  return {
    [Symbol.asyncIterator](): AsyncIterator<TextDelta> {
      return { next: (): Promise<IteratorResult<TextDelta>> => Promise.reject(error) };
    },
  };
}

export function malformedPayload(
  family: AdapterFamily,
  endpoint: string,
  logger: Logger | undefined,
): AppError {
  const details = { family, endpoint };
  logger?.log("warn", "ai", "Вендор вернул неизвестный формат списка моделей", details);
  return new AppError(AppErrorCode.UNKNOWN, "Вендор вернул неизвестный формат списка моделей", {
    details,
  });
}

export function malformedTurn(
  family: AdapterFamily,
  endpoint: string,
  logger: Logger | undefined,
): AppError {
  const details = { family, endpoint };
  logger?.log("warn", "ai", "Вендор вернул неизвестный формат ответа", details);
  return new AppError(AppErrorCode.UNKNOWN, "Вендор вернул неизвестный формат ответа", { details });
}

export function truncatedStream(family: AdapterFamily, endpoint: string): AppError {
  return new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Поток вендора оборвался", {
    details: { family, endpoint },
  });
}

export function flattenPrompt(request: GenerateRequest): string {
  const system = request.system?.trim() ?? "";
  const only = request.messages.length === 1 ? request.messages[0] : undefined;
  if (system.length === 0 && only?.role === "user") return only.content;
  const parts = system.length === 0 ? [] : [`${ROLE_LABELS.system}: ${system}`];
  for (const message of request.messages) {
    parts.push(`${ROLE_LABELS[message.role]}: ${message.content}`);
  }
  return parts.join("\n\n");
}

export abstract class AccountFamilyAdapter implements TextGenerationDriver {
  protected readonly transport: Transport;
  protected readonly logger: Logger | undefined;
  protected readonly clock: () => number;
  protected readonly newId: () => string;
  readonly #onSessionExpired: ((error: AppError) => void) | undefined;

  constructor(options: AccountAdapterOptions) {
    this.transport = options.transport;
    this.logger = options.logger;
    this.clock = options.clock ?? Date.now;
    this.newId = options.newId ?? (() => crypto.randomUUID());
    this.#onSessionExpired = options.onSessionExpired;
  }

  abstract capabilities(): AdapterCapabilities;

  abstract listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;

  abstract generate(request: GenerateRequest, signal: AbortSignal): Promise<GenerateResult>;

  abstract stream(request: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta>;

  protected async request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse> {
    try {
      return await this.transport.request(spec, signal);
    } catch (error: unknown) {
      throw this.normalise(error);
    }
  }

  protected async *frames(spec: RequestSpec, signal: AbortSignal): AsyncIterable<string> {
    try {
      for await (const payload of sseData(this.transport.stream(spec, signal))) {
        if (signal.aborted) throw cancelled();
        yield payload;
      }
    } catch (error: unknown) {
      throw this.normalise(error);
    }
  }

  protected normalise(error: unknown): AppError {
    const failure = isCancellation(error) ? cancelled() : toAppError(error);
    if (isSessionExpired(failure)) this.#onSessionExpired?.(failure);
    return failure;
  }

  protected decode(text: string, endpoint: string): unknown {
    try {
      return JSON.parse(text) as unknown;
    } catch {
      throw malformedPayload(this.capabilities().family, endpoint, this.logger);
    }
  }

  protected dropUnsupported(request: GenerateRequest): TunableParameter[] {
    const capabilities = this.capabilities();
    const dropped = unsupportedParameters(capabilities, {
      temperature: request.temperature,
      topK: request.topK,
      topP: request.topP,
      maxOutputTokens: request.maxOutputTokens,
    });
    if (dropped.length > 0) {
      this.logger?.log("debug", "ai", "Dropped parameters the adapter family does not honour", {
        family: capabilities.family,
        dropped,
      });
    }
    return dropped;
  }
}
