import { AppError, AppErrorCode, type AdapterFamily } from "@zvs/shared";
import type { Logger } from "../../../platform/logger.ts";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import type { AiDriver } from "../ports.ts";
import type { Transport } from "../transport/Transport.ts";
import { OpenAiCompatibleAdapter, OPENAI_COMPATIBLE_CAPABILITIES } from "./openaiCompatible.ts";

export interface AdapterContext {
  transport: Transport;
  logger?: Logger;
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

function stub(
  family: AdapterFamily,
  capabilities: Omit<AdapterCapabilities, "family">,
): AdapterFamilyEntry {
  return {
    capabilities: { family, ...capabilities },
    implemented: false,
    build(): AiDriver {
      throw notImplemented(family);
    },
  };
}

export const ADAPTER_REGISTRY: Readonly<Record<AdapterFamily, AdapterFamilyEntry>> = {
  "openai-compatible": {
    capabilities: OPENAI_COMPATIBLE_CAPABILITIES,
    implemented: true,
    build(context: AdapterContext): AiDriver {
      const adapter = new OpenAiCompatibleAdapter(context);
      return { text: adapter, embedding: adapter, image: null };
    },
  },
  anthropic: stub("anthropic", {
    authModes: ["api"],
    streaming: true,
    liveModelList: true,
    embedding: false,
    image: false,
    honours: { temperature: true, topK: true, topP: true, maxOutputTokens: true },
  }),
  "qwen-web": stub("qwen-web", {
    authModes: ["account"],
    streaming: true,
    liveModelList: true,
    embedding: false,
    image: false,
    honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
  }),
  "deepseek-web": stub("deepseek-web", {
    authModes: ["account"],
    streaming: true,
    liveModelList: false,
    embedding: false,
    image: false,
    honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
  }),
};

export function adapterEntry(family: AdapterFamily): AdapterFamilyEntry {
  const entry = ADAPTER_REGISTRY[family];
  if (entry === undefined) throw notImplemented(family);
  return entry;
}

export function adapterCapabilities(family: AdapterFamily): AdapterCapabilities {
  return adapterEntry(family).capabilities;
}

export { OpenAiCompatibleAdapter, OPENAI_COMPATIBLE_CAPABILITIES };
