import { AppError, AppErrorCode, type AdapterFamily } from "@zvs/shared";
import type { Logger } from "../../../platform/logger.ts";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import { cancelled, isCancellation, toAppError } from "../errors.ts";
import type { DiscoveredModel, GenerateResult, TextDelta, TextGenerationDriver } from "../ports.ts";
import type { RequestSpec, Transport, TransportResponse } from "../transport/Transport.ts";

export const ACCOUNT_GENERATION_PENDING =
  "account-mode generation is not implemented yet (TASK_014_EXTRA_3)";

export interface AccountAdapterOptions {
  transport: Transport;
  logger?: Logger;
}

export interface VendorObservation {
  readonly source: string;
  readonly observedAt: string;
}

export function generationNotImplemented(family: AdapterFamily): AppError {
  return new AppError(AppErrorCode.UNKNOWN, ACCOUNT_GENERATION_PENDING, { details: { family } });
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

export abstract class AccountFamilyAdapter implements TextGenerationDriver {
  protected readonly transport: Transport;
  protected readonly logger: Logger | undefined;

  constructor(options: AccountAdapterOptions) {
    this.transport = options.transport;
    this.logger = options.logger;
  }

  abstract capabilities(): AdapterCapabilities;

  abstract listModels(signal: AbortSignal): Promise<DiscoveredModel[]>;

  generate(): Promise<GenerateResult> {
    return Promise.reject(generationNotImplemented(this.capabilities().family));
  }

  stream(): AsyncIterable<TextDelta> {
    const family = this.capabilities().family;
    return {
      [Symbol.asyncIterator](): AsyncIterator<TextDelta> {
        return {
          next: (): Promise<IteratorResult<TextDelta>> =>
            Promise.reject(generationNotImplemented(family)),
        };
      },
    };
  }

  protected async request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse> {
    try {
      return await this.transport.request(spec, signal);
    } catch (error: unknown) {
      throw isCancellation(error) ? cancelled() : toAppError(error);
    }
  }

  protected decode(text: string, endpoint: string): unknown {
    try {
      return JSON.parse(text) as unknown;
    } catch {
      throw malformedPayload(this.capabilities().family, endpoint, this.logger);
    }
  }
}
