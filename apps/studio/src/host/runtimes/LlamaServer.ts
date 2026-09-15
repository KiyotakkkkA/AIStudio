import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomBytes } from "node:crypto";
import { createServer } from "node:net";
import { AppError, AppErrorCode } from "@zvs/shared";
import type { Logger } from "../platform/logger.ts";

/**
 * `embedding` serves `/v1/embeddings`; `vision` serves `/v1/chat/completions` with an image in
 * the message. One server cannot do both — `--embeddings` puts llama.cpp in a pooling mode that
 * cannot generate — so the role is fixed when the process starts.
 */
export type LlamaServerRole = "embedding" | "vision";

export interface LlamaServerOptions {
  executablePath: string;
  modelPath: string;
  role?: LlamaServerRole;
  mmprojPath?: string;
  gpuLayers: number;
  contextSize?: number;
  batchSize?: number;
  threads?: number;
  idleMs?: number;
  startTimeoutMs?: number;
  logger?: Logger;
  spawn?: typeof spawn;
  fetch?: (url: string, init?: RequestInit) => Promise<Response>;
  freePort?: () => Promise<number>;
}

interface EmbeddingResponse {
  data?: unknown;
}

interface ChatResponse {
  choices?: unknown;
}

/**
 * Finds a port the OS is willing to hand out, then releases it. There is a window in which
 * something else could claim it; llama-server failing to bind is caught as a start failure and
 * surfaced, which is a better trade than managing a port range ourselves.
 */
export async function freeLoopbackPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.on("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      const port = typeof address === "object" && address !== null ? address.port : 0;
      probe.close(() => {
        if (port === 0) reject(new Error("Could not reserve a port"));
        else resolve(port);
      });
    });
  });
}

/**
 * One `llama-server` child, bound to loopback and gated by a random key, wrapping the OpenAI
 * embeddings endpoint it exposes. The process is expensive to start — the whole model is read
 * into memory — so it outlives a single call and stops itself once nothing has used it.
 */
export class LlamaServer {
  #child: ChildProcessWithoutNullStreams | undefined;
  #ready: Promise<string> | undefined;
  #baseUrl: string | undefined;
  #apiKey = "";
  #idleTimer: NodeJS.Timeout | undefined;
  #stderr = "";
  #stopped = false;

  constructor(private readonly options: LlamaServerOptions) {}

  get running(): boolean {
    return this.#child !== undefined && this.#child.exitCode === null;
  }

  get modelPath(): string {
    return this.options.modelPath;
  }

  get role(): LlamaServerRole {
    return this.options.role ?? "embedding";
  }

  async embed(texts: readonly string[], signal: AbortSignal): Promise<Float32Array[]> {
    if (texts.length === 0) return [];
    const payload = (await this.post(
      "/v1/embeddings",
      { input: [...texts], model: this.options.modelPath },
      "посчитать эмбеддинги",
      signal,
    )) as EmbeddingResponse;
    return readEmbeddings(payload);
  }

  /**
   * Reads an image with a vision model. The image rides as a data URI in the message, which is
   * what llama.cpp's multimodal endpoint accepts; nothing is written to disk on the way.
   */
  async describeImage(
    image: { readonly mediaType: string; readonly bytes: Uint8Array },
    prompt: string,
    signal: AbortSignal,
    maxTokens = 2048,
  ): Promise<string> {
    const dataUri = `data:${image.mediaType};base64,${Buffer.from(image.bytes).toString("base64")}`;
    const payload = (await this.post(
      "/v1/chat/completions",
      {
        model: this.options.modelPath,
        temperature: 0,
        max_tokens: maxTokens,
        messages: [
          {
            role: "user",
            content: [
              { type: "text", text: prompt },
              { type: "image_url", image_url: { url: dataUri } },
            ],
          },
        ],
      },
      "распознать страницу",
      signal,
    )) as ChatResponse;
    return readChatText(payload);
  }

  private async post(
    route: string,
    body: unknown,
    what: string,
    signal: AbortSignal,
  ): Promise<unknown> {
    const baseUrl = await this.start(signal);
    this.touch();
    const request = this.options.fetch ?? globalThis.fetch;
    const response = await request(`${baseUrl}${route}`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${this.#apiKey}`,
      },
      body: JSON.stringify(body),
      signal,
    });
    if (!response.ok) {
      const detail = await response.text().catch(() => "");
      throw new AppError(AppErrorCode.UNKNOWN, `Локальный движок не смог ${what}`, {
        details: { status: response.status, detail: detail.slice(0, 500) },
      });
    }
    return response.json();
  }

  start(signal?: AbortSignal): Promise<string> {
    if (this.#ready !== undefined && this.running) return this.#ready;
    this.#stopped = false;
    this.#ready = this.launch(signal).catch((error: unknown) => {
      this.#ready = undefined;
      this.stop();
      throw error;
    });
    return this.#ready;
  }

  stop(): void {
    this.#stopped = true;
    if (this.#idleTimer !== undefined) clearTimeout(this.#idleTimer);
    this.#idleTimer = undefined;
    const child = this.#child;
    this.#child = undefined;
    this.#ready = undefined;
    this.#baseUrl = undefined;
    if (child === undefined || child.exitCode !== null) return;
    child.kill();
    // A model still loading ignores SIGTERM until it reaches a poll point.
    const hard = setTimeout(() => child.kill("SIGKILL"), 3_000);
    hard.unref();
    child.once("exit", () => {
      clearTimeout(hard);
    });
  }

  private async launch(signal?: AbortSignal): Promise<string> {
    const port = await (this.options.freePort ?? freeLoopbackPort)();
    this.#apiKey = randomBytes(24).toString("hex");
    this.#stderr = "";
    const vision = this.role === "vision";
    const argv = [
      "--model",
      this.options.modelPath,
      ...(vision ? ["--mmproj", this.options.mmprojPath ?? ""] : ["--embeddings"]),
      "--host",
      "127.0.0.1",
      "--port",
      String(port),
      "--api-key",
      this.#apiKey,
      "--n-gpu-layers",
      String(this.options.gpuLayers),
      "--ctx-size",
      String(this.options.contextSize ?? 8192),
      "--batch-size",
      String(this.options.batchSize ?? 2048),
      ...(this.options.threads === undefined ? [] : ["--threads", String(this.options.threads)]),
    ];
    this.options.logger?.log("info", "runtimes", "Starting the local embedding server", {
      executable: this.options.executablePath,
      model: this.options.modelPath,
      gpuLayers: this.options.gpuLayers,
      port,
    });
    const child = (this.options.spawn ?? spawn)(this.options.executablePath, argv, {
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    }) as unknown as ChildProcessWithoutNullStreams;
    this.#child = child;
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    // llama-server logs to stderr; the tail is what a start failure has to explain itself with.
    child.stderr.on("data", (chunk: string) => {
      this.#stderr = `${this.#stderr}${chunk}`.slice(-4000);
    });
    child.on("error", (error) => {
      this.#stderr = `${this.#stderr}\n${String(error)}`.slice(-4000);
    });

    const baseUrl = `http://127.0.0.1:${String(port)}`;
    await this.waitForHealth(baseUrl, child, signal);
    this.#baseUrl = baseUrl;
    this.touch();
    return baseUrl;
  }

  private async waitForHealth(
    baseUrl: string,
    child: ChildProcessWithoutNullStreams,
    signal?: AbortSignal,
  ): Promise<void> {
    const request = this.options.fetch ?? globalThis.fetch;
    const deadline = Date.now() + (this.options.startTimeoutMs ?? 180_000);
    for (;;) {
      signal?.throwIfAborted();
      if (child.exitCode !== null || this.#stopped)
        throw new AppError(AppErrorCode.UNKNOWN, "Локальный движок завершился при запуске", {
          details: { exitCode: child.exitCode, stderr: this.#stderr.slice(-1000) },
        });
      try {
        const response = await request(`${baseUrl}/health`, {
          headers: { authorization: `Bearer ${this.#apiKey}` },
          signal: AbortSignal.timeout(2_000),
        });
        if (response.ok) return;
      } catch {
        // Not listening yet. Expected for as long as the weights are still being read.
      }
      if (Date.now() > deadline)
        throw new AppError(
          AppErrorCode.PROVIDER_UNREACHABLE,
          "Локальный движок не ответил при запуске",
          {
            details: { baseUrl, stderr: this.#stderr.slice(-1000) },
          },
        );
      await delay(250, signal);
    }
  }

  private touch(): void {
    const idle = this.options.idleMs ?? 5 * 60 * 1000;
    if (idle <= 0) return;
    if (this.#idleTimer !== undefined) clearTimeout(this.#idleTimer);
    this.#idleTimer = setTimeout(() => {
      this.options.logger?.log("info", "runtimes", "Stopping the idle embedding server", {
        model: this.options.modelPath,
        baseUrl: this.#baseUrl,
      });
      this.stop();
    }, idle);
    this.#idleTimer.unref();
  }
}

function readChatText(payload: ChatResponse): string {
  if (!Array.isArray(payload.choices) || payload.choices.length === 0)
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Движок вернул пустой ответ");
  const content = (payload.choices[0] as { message?: { content?: unknown } }).message?.content;
  if (typeof content === "string") return content;
  // Some builds answer with the content-part array they were given.
  if (Array.isArray(content))
    return content
      .map((part: { text?: unknown }) => (typeof part.text === "string" ? part.text : ""))
      .join("");
  throw new AppError(AppErrorCode.VALIDATION_FAILED, "Движок вернул ответ неизвестного вида");
}

function readEmbeddings(payload: EmbeddingResponse): Float32Array[] {
  if (!Array.isArray(payload.data))
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Движок вернул неожиданный ответ");
  return payload.data.map((entry: { embedding?: unknown }) => {
    const raw = entry.embedding;
    if (!Array.isArray(raw) || raw.some((value) => typeof value !== "number"))
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Движок вернул пустой вектор");
    return Float32Array.from(raw as number[]);
  });
}

function delay(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    const onAbort = (): void => {
      clearTimeout(timer);
      reject(new AppError(AppErrorCode.RUN_CANCELLED, "Запуск движка отменён"));
    };
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}
