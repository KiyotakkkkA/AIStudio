import type { AdapterCapabilities } from "../../apps/studio/src/host/drivers/ai/AdapterCapabilities.ts";
import { cancelled } from "../../apps/studio/src/host/drivers/ai/errors.ts";
import type {
  AiDriver,
  DiscoveredModel,
  EmbeddingDriver,
  GeneratedImage,
  GenerateRequest,
  GenerateResult,
  ImageDriver,
  ImageRequest,
  TextDelta,
  TextGenerationDriver,
} from "../../apps/studio/src/host/drivers/ai/ports.ts";

export const FAKE_CAPABILITIES: AdapterCapabilities = {
  family: "openai-compatible",
  authModes: ["api", "account"],
  streaming: true,
  liveModelList: true,
  embedding: true,
  image: true,
  honours: { temperature: true, topK: false, topP: true, maxOutputTokens: true },
};

export interface FakeDriverScript {
  capabilities?: AdapterCapabilities;
  deltas?: readonly string[];
  text?: string;
  models?: readonly DiscoveredModel[];
  vectors?: readonly number[][];
  images?: readonly GeneratedImage[];
  failWith?: Error;
  failAfter?: number;
  delayMs?: number;
  manual?: boolean;
}

export interface FakeDriver extends AiDriver {
  readonly text: TextGenerationDriver;
  readonly embedding: EmbeddingDriver;
  readonly image: ImageDriver;
  readonly requests: readonly GenerateRequest[];
  readonly emitted: number;
  script(script: FakeDriverScript): FakeDriver;
  whenHolding(): Promise<void>;
  release(): void;
  reset(): void;
}

export function createFakeDriver(initial: FakeDriverScript = {}): FakeDriver {
  let script: FakeDriverScript = { deltas: ["Hello", " world"], ...initial };
  const requests: GenerateRequest[] = [];
  let emitted = 0;
  let gate: (() => void) | null = null;
  let waiters: (() => void)[] = [];

  const hold = async (signal: AbortSignal): Promise<void> => {
    const delay = script.delayMs ?? 0;
    if (delay === 0 && script.manual !== true) {
      if (signal.aborted) throw cancelled();
      return;
    }
    await new Promise<void>((resolve, reject) => {
      const timer = delay > 0 ? setTimeout(() => finish(), delay) : undefined;
      const finish = (): void => {
        clearTimeout(timer);
        signal.removeEventListener("abort", onAbort);
        gate = null;
        resolve();
      };
      const onAbort = (): void => {
        clearTimeout(timer);
        gate = null;
        reject(cancelled());
      };
      gate = finish;
      for (const waiter of waiters.splice(0)) waiter();
      if (signal.aborted) {
        onAbort();
        return;
      }
      signal.addEventListener("abort", onAbort, { once: true });
    });
  };

  const deltas = (): readonly string[] => script.deltas ?? [];

  const capabilities = (): AdapterCapabilities => script.capabilities ?? FAKE_CAPABILITIES;

  const listModels = async (signal: AbortSignal): Promise<DiscoveredModel[]> => {
    await hold(signal);
    if (script.failWith !== undefined) throw script.failWith;
    return [...(script.models ?? [])];
  };

  const text: TextGenerationDriver = {
    capabilities,
    listModels,

    async generate(request: GenerateRequest, signal: AbortSignal): Promise<GenerateResult> {
      requests.push(request);
      await hold(signal);
      if (script.failWith !== undefined) throw script.failWith;
      return {
        model: request.model,
        text: script.text ?? deltas().join(""),
        reasoning: null,
        finishReason: "stop",
        usage: null,
      };
    },

    stream(request: GenerateRequest, signal: AbortSignal): AsyncIterable<TextDelta> {
      requests.push(request);
      return {
        async *[Symbol.asyncIterator](): AsyncIterator<TextDelta> {
          const chunks = deltas();
          for (let index = 0; index < chunks.length; index += 1) {
            await hold(signal);
            if (script.failAfter === index && script.failWith !== undefined) throw script.failWith;
            const chunk = chunks[index];
            if (chunk === undefined) continue;
            emitted += 1;
            yield { text: chunk, kind: "text" };
          }
          if (script.failAfter === chunks.length && script.failWith !== undefined) {
            throw script.failWith;
          }
        },
      };
    },
  };

  const embedding: EmbeddingDriver = {
    capabilities,
    listModels,
    dimensions: () => script.vectors?.[0]?.length ?? null,

    async embed(texts: readonly string[], _model: string, signal: AbortSignal) {
      await hold(signal);
      if (script.failWith !== undefined) throw script.failWith;
      const vectors = script.vectors ?? [];
      return texts.map((_value, index) => Float32Array.from(vectors[index] ?? [0]));
    },
  };

  const image: ImageDriver = {
    capabilities,
    listModels,

    async generateImages(request: ImageRequest, signal: AbortSignal): Promise<GeneratedImage[]> {
      await hold(signal);
      if (script.failWith !== undefined) throw script.failWith;
      return [...(script.images ?? [])].slice(0, request.count ?? 1);
    },
  };

  return {
    text,
    embedding,
    image,

    get requests() {
      return requests;
    },

    get emitted() {
      return emitted;
    },

    script(next: FakeDriverScript) {
      script = { ...script, ...next };
      return this;
    },

    whenHolding() {
      if (gate !== null) return Promise.resolve();
      return new Promise<void>((resolve) => waiters.push(resolve));
    },

    release() {
      gate?.();
    },

    reset() {
      requests.length = 0;
      emitted = 0;
      gate = null;
      waiters = [];
    },
  };
}
