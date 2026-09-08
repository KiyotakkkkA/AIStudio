import { z } from "zod";
import {
  AppErrorCode,
  err,
  isAppError,
  ok,
  type ChannelName,
  type ChannelSchemas,
  type ContractShape,
  type ErrorDetails,
  type InputOf,
  type OutputOf,
  type Result,
} from "@zvs/shared";

export interface IpcMainLike {
  handle(
    channel: string,
    listener: (event: unknown, ...args: unknown[]) => unknown | Promise<unknown>,
  ): void;
  removeHandler(channel: string): void;
}

export interface IpcServerLogger {
  log(
    level: "debug" | "info" | "warn" | "error",
    scope: string,
    message: string,
    fields?: Record<string, unknown>,
  ): void;
}

export type IpcHandler<C extends ContractShape, K extends ChannelName<C>> = (
  input: InputOf<C, K>,
) => OutputOf<C, K> | Promise<OutputOf<C, K>>;

export type IpcHandlers<C extends ContractShape> = {
  [K in ChannelName<C>]: IpcHandler<C, K>;
};

export interface IpcServerOptions {
  ipcMain: IpcMainLike;
  logger: IpcServerLogger;
  validateOutput?: boolean;
}

export interface IpcServer {
  readonly channels: readonly string[];
  dispose(): void;
}

const VALIDATION_MESSAGE = "Некорректные данные запроса";
const INTERNAL_MESSAGE = "Внутренняя ошибка";

function describe(error: unknown): string {
  if (error instanceof Error) return error.stack ?? `${error.name}: ${error.message}`;
  return String(error);
}

function failure(code: AppErrorCode, message: string, details?: ErrorDetails) {
  try {
    return err(code, message, details);
  } catch {
    return err(code, message);
  }
}

function toFailure(error: unknown) {
  if (isAppError(error)) return failure(error.code, error.message, error.details);
  if (error instanceof z.ZodError)
    return failure(AppErrorCode.VALIDATION_FAILED, VALIDATION_MESSAGE);
  return failure(AppErrorCode.UNKNOWN, INTERNAL_MESSAGE);
}

export function createIpcServer<C extends ContractShape>(
  contract: C,
  handlers: IpcHandlers<C>,
  options: IpcServerOptions,
): IpcServer {
  const { ipcMain, logger } = options;
  const validateOutput = options.validateOutput ?? true;
  const channels = Object.keys(contract) as ChannelName<C>[];

  for (const channel of channels) {
    const schemas = contract[channel] as ChannelSchemas;
    const handler = handlers[channel];
    ipcMain.handle(channel, async (_event, ...args): Promise<Result<unknown>> => {
      const parsedInput = schemas.input.safeParse(args[0]);
      if (!parsedInput.success) {
        logger.log("warn", "ipc", "Rejected invalid input", {
          channel,
          issues: parsedInput.error.issues.length,
        });
        return failure(AppErrorCode.VALIDATION_FAILED, VALIDATION_MESSAGE);
      }
      try {
        const output = await handler(parsedInput.data as InputOf<C, ChannelName<C>>);
        if (validateOutput) {
          const parsedOutput = schemas.output.safeParse(output);
          if (!parsedOutput.success) {
            logger.log("error", "ipc", "Handler output does not match the contract", {
              channel,
              issues: parsedOutput.error.issues.length,
            });
            return failure(AppErrorCode.UNKNOWN, INTERNAL_MESSAGE);
          }
          return ok(parsedOutput.data);
        }
        return ok(output);
      } catch (error: unknown) {
        const result = toFailure(error);
        logger.log("error", "ipc", "Handler failed", {
          channel,
          code: result.error.code,
          error: describe(error),
        });
        return result;
      }
    });
  }

  return {
    channels,
    dispose() {
      for (const channel of channels) ipcMain.removeHandler(channel);
    },
  };
}
