import { spawn as spawnProcess, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import { AppError, AppErrorCode, type Json } from "@zvs/shared";
import type { Logger } from "../../platform/logger.ts";
import { createId } from "../../platform/ids.ts";
import { SidecarResponse, type JobProgress, type SidecarRequest } from "./protocol.ts";

export interface JobRunOptions {
  signal?: AbortSignal;
  onProgress?: (progress: JobProgress) => void;
}

export interface SidecarJobsPort {
  run(job: string, params: Json, options?: JobRunOptions): Promise<Json>;
}

export type SpawnSidecar = (
  command: string,
  args: readonly string[],
) => ChildProcessWithoutNullStreams;

export interface SidecarDriverOptions {
  binaryPath: string;
  args?: readonly string[];
  logger?: Logger;
  spawn?: SpawnSidecar;
  pingIntervalMs?: number;
  pingTimeoutMs?: number;
  cancelGraceMs?: number;
  backoffMs?: number;
  maxBackoffMs?: number;
}

interface Pending {
  resolve(value: Json): void;
  reject(error: unknown): void;
  onProgress?: (progress: JobProgress) => void;
}

const MAX_LINE_BYTES = 8 * 1024 * 1024;
const codes = new Set<string>(Object.values(AppErrorCode));

const defaultSpawn: SpawnSidecar = (command, args) =>
  spawnProcess(command, [...args], { stdio: ["pipe", "pipe", "pipe"], windowsHide: true });

function sidecarError(code: string, message: string): AppError {
  return new AppError(
    codes.has(code) ? (code as AppErrorCode) : AppErrorCode.NATIVE_ERROR,
    message,
  );
}

export class SidecarDriver implements SidecarJobsPort {
  private child: ChildProcessWithoutNullStreams | undefined;
  private starting: Promise<ChildProcessWithoutNullStreams> | undefined;
  private health: ReturnType<typeof setInterval> | undefined;
  private readonly jobs = new Map<string, Pending>();
  private readonly pings = new Map<string, () => void>();
  private buffer = "";
  private restarts = 0;
  private nextSpawnAt = 0;
  private closed = false;

  constructor(private readonly options: SidecarDriverOptions) {}

  private get pingTimeoutMs(): number {
    return this.options.pingTimeoutMs ?? 2_000;
  }

  private get cancelGraceMs(): number {
    return this.options.cancelGraceMs ?? 2_000;
  }

  private log(
    level: "debug" | "info" | "warn" | "error",
    message: string,
    fields: Record<string, unknown> = {},
  ): void {
    this.options.logger?.log(level, "sidecar", message, fields);
  }

  async run(job: string, params: Json, options: JobRunOptions = {}): Promise<Json> {
    if (this.closed) throw new AppError(AppErrorCode.CONFLICT, "Сервис задач остановлен");
    if (options.signal?.aborted)
      throw new AppError(AppErrorCode.RUN_CANCELLED, "Операция отменена");
    await this.ensure();
    const id = createId();
    const answer = new Promise<Json>((resolve, reject) => {
      this.jobs.set(id, { resolve, reject, onProgress: options.onProgress });
    });
    let grace: ReturnType<typeof setTimeout> | undefined;
    const cancel = (): void => {
      this.log("info", "Cancelling a sidecar job", { id, job });
      try {
        this.send({ type: "job.cancel", id });
      } catch (error) {
        this.log("warn", "Could not deliver a cancellation", { id, job, error: String(error) });
        return;
      }
      grace = setTimeout(() => {
        if (!this.jobs.has(id)) return;
        this.log("error", "A job ignored cancellation; killing the sidecar", {
          id,
          job,
          graceMs: this.cancelGraceMs,
        });
        this.kill();
      }, this.cancelGraceMs);
      grace.unref();
    };
    options.signal?.addEventListener("abort", cancel, { once: true });
    try {
      this.send({ type: "job.start", id, job, params });
      return await answer;
    } finally {
      options.signal?.removeEventListener("abort", cancel);
      clearTimeout(grace);
      this.jobs.delete(id);
    }
  }

  /** Round-trips a `ping`, rejecting once the sidecar stops answering in time. */
  async ping(): Promise<void> {
    const child = await this.ensure();
    const id = createId();
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      await new Promise<void>((resolve, reject) => {
        this.pings.set(id, resolve);
        timer = setTimeout(
          () =>
            reject(new AppError(AppErrorCode.SIDECAR_UNAVAILABLE, "Sidecar не ответил на ping")),
          this.pingTimeoutMs,
        );
        timer.unref();
        try {
          this.write(child, { type: "ping", id });
        } catch (error) {
          reject(error);
        }
      });
    } finally {
      clearTimeout(timer);
      this.pings.delete(id);
    }
  }

  async dispose(): Promise<void> {
    this.closed = true;
    const child = this.child;
    this.kill();
    this.child = undefined;
    if (child && child.exitCode === null && !child.killed)
      await new Promise((done) => child.once("exit", done));
  }

  private async ensure(): Promise<ChildProcessWithoutNullStreams> {
    if (this.child) return this.child;
    this.starting ??= this.spawn().finally(() => {
      this.starting = undefined;
    });
    return this.starting;
  }

  private async spawn(): Promise<ChildProcessWithoutNullStreams> {
    const { binaryPath } = this.options;
    if (!existsSync(binaryPath))
      throw new AppError(
        AppErrorCode.SIDECAR_UNAVAILABLE,
        `Не найден sidecar zvs-jobd: ${binaryPath}. Выполните pnpm build:rust из корня репозитория.`,
      );
    const wait = this.nextSpawnAt - Date.now();
    if (wait > 0) {
      this.log("info", "Waiting out the sidecar restart backoff", { waitMs: wait });
      await new Promise((done) => setTimeout(done, wait).unref());
    }
    if (this.closed) throw new AppError(AppErrorCode.CONFLICT, "Сервис задач остановлен");
    let child: ChildProcessWithoutNullStreams;
    try {
      child = (this.options.spawn ?? defaultSpawn)(binaryPath, this.options.args ?? []);
    } catch (cause) {
      throw new AppError(
        AppErrorCode.SIDECAR_UNAVAILABLE,
        `Не удалось запустить sidecar zvs-jobd: ${binaryPath}. Выполните pnpm build:rust из корня репозитория.`,
        { cause },
      );
    }
    this.child = child;
    this.buffer = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.receive(chunk));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      for (const line of chunk.split("\n"))
        if (line.trim()) this.log("warn", "Sidecar diagnostics", { raw: line.trim() });
    });
    child.stdin.on("error", (error) => this.log("warn", "Sidecar input failed", { raw: error }));
    child.once("error", (error) => {
      this.log("error", "The sidecar could not run", { raw: error });
      this.retire(child, String(error));
    });
    child.once("exit", (code, signal) =>
      this.retire(child, `Sidecar завершился (code=${String(code)}, signal=${String(signal)})`),
    );
    this.health = setInterval(
      () => void this.heartbeat(child),
      this.options.pingIntervalMs ?? 5_000,
    );
    this.health.unref();
    this.log("info", "Started the sidecar", { binaryPath, pid: child.pid ?? null });
    return child;
  }

  private async heartbeat(child: ChildProcessWithoutNullStreams): Promise<void> {
    if (this.closed || this.child !== child) return;
    try {
      await this.ping();
    } catch (error) {
      if (this.child !== child) return;
      this.log("error", "The sidecar stopped answering ping; killing it", { raw: error });
      child.kill("SIGKILL");
    }
  }

  private retire(child: ChildProcessWithoutNullStreams, reason: string): void {
    if (this.child !== child) return;
    this.child = undefined;
    clearInterval(this.health);
    this.health = undefined;
    const abandoned = [...this.jobs.values()];
    this.jobs.clear();
    this.pings.clear();
    for (const pending of abandoned)
      pending.reject(new AppError(AppErrorCode.SIDECAR_UNAVAILABLE, reason));
    if (this.closed) return;
    const backoffMs = Math.min(
      this.options.maxBackoffMs ?? 30_000,
      (this.options.backoffMs ?? 250) * 2 ** this.restarts,
    );
    this.restarts += 1;
    this.nextSpawnAt = Date.now() + backoffMs;
    this.log("warn", "The sidecar exited; it restarts on the next job", {
      reason,
      abandoned: abandoned.length,
      restarts: this.restarts,
      backoffMs,
    });
  }

  private kill(): void {
    clearInterval(this.health);
    this.health = undefined;
    this.child?.kill("SIGKILL");
  }

  private send(request: SidecarRequest): void {
    const child = this.child;
    if (!child) throw new AppError(AppErrorCode.SIDECAR_UNAVAILABLE, "Sidecar не запущен");
    this.write(child, request);
  }

  private write(child: ChildProcessWithoutNullStreams, request: SidecarRequest): void {
    child.stdin.write(`${JSON.stringify(request)}\n`);
  }

  private receive(chunk: string): void {
    this.buffer += chunk;
    if (this.buffer.length > MAX_LINE_BYTES) {
      this.log("error", "The sidecar sent an oversized line; killing it", {
        bytes: this.buffer.length,
      });
      this.buffer = "";
      this.child?.kill("SIGKILL");
      return;
    }
    let newline = this.buffer.indexOf("\n");
    while (newline >= 0) {
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (line) this.route(line);
      newline = this.buffer.indexOf("\n");
    }
  }

  private route(line: string): void {
    let message: SidecarResponse;
    try {
      message = SidecarResponse.parse(JSON.parse(line));
    } catch (error) {
      this.log("error", "Unreadable sidecar message", { raw: line, error: String(error) });
      return;
    }
    if (message.type === "ping") {
      this.restarts = 0;
      this.pings.get(message.id)?.();
      return;
    }
    const pending = this.jobs.get(message.id);
    if (!pending) {
      this.log("debug", "The sidecar answered an unknown request", { id: message.id });
      return;
    }
    if (message.type === "job.progress") {
      pending.onProgress?.({
        done: message.done,
        total: message.total,
        ...(message.message === undefined ? {} : { message: message.message }),
      });
      return;
    }
    this.jobs.delete(message.id);
    if (message.type === "job.done") pending.resolve(message.result);
    else pending.reject(sidecarError(message.code, message.message));
  }
}
