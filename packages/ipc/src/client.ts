import {
  AppErrorCode,
  Result,
  type ChannelName,
  type ChannelSchemas,
  type ContractShape,
  type InputOf,
  type OutputOf,
} from "@zvs/shared";
import { IpcError } from "./IpcError.js";

export interface IpcBridge {
  call(channel: string, payload: unknown): Promise<unknown>;
}

export interface IpcClientOptions {
  validateResponses?: boolean;
}

export interface IpcClient<C extends ContractShape> {
  call<K extends ChannelName<C>>(channel: K, input: InputOf<C, K>): Promise<OutputOf<C, K>>;
}

const REQUEST_MESSAGE = "Некорректные данные запроса";
const RESPONSE_MESSAGE = "Некорректный ответ хоста";

export function createIpcClient<C extends ContractShape>(
  contract: C,
  bridge: IpcBridge,
  options: IpcClientOptions = {},
): IpcClient<C> {
  const validateResponses = options.validateResponses ?? true;
  const envelopes = new Map<string, ReturnType<typeof Result>>();

  return {
    async call<K extends ChannelName<C>>(
      channel: K,
      input: InputOf<C, K>,
    ): Promise<OutputOf<C, K>> {
      const schemas = contract[channel] as ChannelSchemas;
      const parsedInput = schemas.input.safeParse(input);
      if (!parsedInput.success) {
        throw new IpcError(AppErrorCode.VALIDATION_FAILED, REQUEST_MESSAGE, channel);
      }
      const response = await bridge.call(channel, parsedInput.data);
      if (!validateResponses) {
        const envelope = response as Result<OutputOf<C, K>>;
        if (!envelope.ok) {
          throw new IpcError(
            envelope.error.code,
            envelope.error.message,
            channel,
            envelope.error.details,
          );
        }
        return envelope.data;
      }
      let envelopeSchema = envelopes.get(channel);
      if (!envelopeSchema) {
        envelopeSchema = Result(schemas.output);
        envelopes.set(channel, envelopeSchema);
      }
      const parsed = envelopeSchema.safeParse(response);
      if (!parsed.success) {
        throw new IpcError(AppErrorCode.UNKNOWN, RESPONSE_MESSAGE, channel);
      }
      if (!parsed.data.ok) {
        const { code, message, details } = parsed.data.error;
        throw new IpcError(code, message, channel, details);
      }
      return parsed.data.data as OutputOf<C, K>;
    },
  };
}
