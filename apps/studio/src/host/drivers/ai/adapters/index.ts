import { AppError, AppErrorCode, type AdapterFamily } from "@zvs/shared";
import type { Logger } from "../../../platform/logger.ts";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import type { AiDriver } from "../ports.ts";
import type { Transport } from "../transport/Transport.ts";
import { DeepSeekWebAdapter, DEEPSEEK_WEB_CAPABILITIES } from "./deepseekWeb.ts";
import { LocalEmbeddingAdapter, LOCAL_CAPABILITIES, type LocalModelStore } from "./local.ts";
import { OpenAiCompatibleAdapter, OPENAI_COMPATIBLE_CAPABILITIES } from "./openaiCompatible.ts";
import { QwenWebAdapter, QWEN_WEB_CAPABILITIES } from "./qwenWeb.ts";

export interface AdapterContext {
  transport?: Transport;
  logger?: Logger;
  localModels?: LocalModelStore;
}

export interface NetworkAdapterContext extends AdapterContext {
  transport: Transport;
}

export function requireTransport(context: AdapterContext): NetworkAdapterContext {
  if (context.transport === undefined)
    throw new AppError(AppErrorCode.CONFLICT, "Адаптеру нужен транспорт");
  return { ...context, transport: context.transport };
}

export interface AdapterFamilyEntry {
  readonly capabilities: AdapterCapabilities;
  readonly implemented: boolean;
  build(context: AdapterContext): AiDriver;
}

export function notImplemented(family: AdapterFamily): AppError {
  return new AppError(AppErrorCode.UNKNOWN, `adapter ${family} is not implemented yet`, {
    details: { family },
  });
}

export const ADAPTER_REGISTRY: Readonly<Record<AdapterFamily, AdapterFamilyEntry>> = {
  "openai-compatible": {
    capabilities: OPENAI_COMPATIBLE_CAPABILITIES,
    implemented: true,
    build(context: AdapterContext): AiDriver {
      const adapter = new OpenAiCompatibleAdapter(requireTransport(context));
      return { text: adapter, embedding: adapter, image: null };
    },
  },
  "qwen-web": {
    capabilities: QWEN_WEB_CAPABILITIES,
    implemented: true,
    build(context: AdapterContext): AiDriver {
      return { text: new QwenWebAdapter(requireTransport(context)), embedding: null, image: null };
    },
  },
  "deepseek-web": {
    capabilities: DEEPSEEK_WEB_CAPABILITIES,
    implemented: true,
    build(context: AdapterContext): AiDriver {
      return {
        text: new DeepSeekWebAdapter(requireTransport(context)),
        embedding: null,
        image: null,
      };
    },
  },
  local: {
    capabilities: LOCAL_CAPABILITIES,
    implemented: true,
    build(context: AdapterContext): AiDriver {
      if (context.localModels === undefined)
        throw new AppError(AppErrorCode.CONFLICT, "Каталог локальных моделей недоступен");
      return {
        text: null,
        embedding: new LocalEmbeddingAdapter(context.localModels, context.logger),
        image: null,
      };
    },
  },
};

export function adapterEntry(family: AdapterFamily): AdapterFamilyEntry {
  const entry = ADAPTER_REGISTRY[family];
  if (entry === undefined) throw notImplemented(family);
  return entry;
}

export function adapterCapabilities(family: AdapterFamily): AdapterCapabilities {
  return adapterEntry(family).capabilities;
}

export { DeepSeekWebAdapter, DEEPSEEK_WEB_CAPABILITIES };
export { OpenAiCompatibleAdapter, OPENAI_COMPATIBLE_CAPABILITIES };
export { QwenWebAdapter, QWEN_WEB_CAPABILITIES };
export { LocalEmbeddingAdapter, LOCAL_CAPABILITIES };
export type { LocalModelStore };
