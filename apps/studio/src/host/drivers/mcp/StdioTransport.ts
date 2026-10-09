import {
  spawn as spawnProcess,
  spawnSync,
  type ChildProcessWithoutNullStreams,
} from "node:child_process";
import { AppError, AppErrorCode } from "@zvs/shared";
import type { JsonRpcOutgoing } from "./protocol.ts";
import type { McpTransport, McpTransportHandlers } from "./transport.ts";
import { windowsLaunch } from "./windowsCommand.ts";

export type SpawnServer = (
  command: string,
  args: readonly string[],
  options: { env: NodeJS.ProcessEnv; cwd?: string },
) => ChildProcessWithoutNullStreams;

export interface StdioTransportOptions {
  command: string;
  args?: readonly string[];
  /**
   * Already resolved: a secret reference became plaintext one frame ago, in the service that
   * launched this. It is handed to the child and to nothing else — never logged, never stored.
   */
  env?: Readonly<Record<string, string>>;
  cwd?: string;
  spawn?: SpawnServer;
}

const MAX_LINE_BYTES = 16 * 1024 * 1024;

const defaultSpawn: SpawnServer = (command, args, options) => {
  const launch =
    process.platform === "win32"
      ? windowsLaunch(command, args, options.env, undefined, options.cwd)
      : { file: command, args, verbatim: false };
  return spawnProcess(launch.file, [...launch.args], {
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
    windowsVerbatimArguments: launch.verbatim,
    env: options.env,
    ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
  });
};

function killTree(child: ChildProcessWithoutNullStreams): void {
  if (process.platform === "win32" && child.pid !== undefined) {
    spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { windowsHide: true });
    return;
  }
  child.kill("SIGKILL");
}

export class StdioTransport implements McpTransport {
  readonly kind = "stdio" as const;
  #child: ChildProcessWithoutNullStreams | undefined;
  #handlers: McpTransportHandlers | undefined;
  #buffer = "";
  #closing = false;

  constructor(private readonly options: StdioTransportOptions) {}

  async open(handlers: McpTransportHandlers): Promise<void> {
    this.#handlers = handlers;
    let child: ChildProcessWithoutNullStreams;
    try {
      child = (this.options.spawn ?? defaultSpawn)(this.options.command, this.options.args ?? [], {
        env: { ...process.env, ...this.options.env },
        ...(this.options.cwd === undefined ? {} : { cwd: this.options.cwd }),
      });
    } catch (cause) {
      throw this.#cannotStart(cause);
    }
    this.#child = child;
    this.#buffer = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.#receive(chunk));
    // Servers use stderr for diagnostics, so it is a log source rather than a failure signal.
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      for (const line of chunk.split("\n"))
        if (line.trim()) handlers.log("debug", line.trim().slice(0, 1000));
    });
    child.stdin.on("error", (error) => handlers.log("warn", `stdin: ${String(error)}`));
    await new Promise<void>((resolve, reject) => {
      const started = (): void => {
        child.off("error", failed);
        child.once("error", (error) => this.#end(`${this.options.command}: ${String(error)}`));
        child.once("exit", (code, signal) =>
          this.#end(
            this.#closing
              ? "stopped"
              : `${this.options.command} завершился (code=${String(code)}, signal=${String(signal)})`,
          ),
        );
        resolve();
      };
      const failed = (error: Error): void => {
        child.off("spawn", started);
        this.#child = undefined;
        this.#handlers = undefined;
        reject(this.#cannotStart(error));
      };
      child.once("spawn", started);
      child.once("error", failed);
    });
  }

  async send(message: JsonRpcOutgoing): Promise<void> {
    const child = this.#child;
    if (!child || child.exitCode !== null)
      throw new AppError(AppErrorCode.MCP_UNAVAILABLE, "MCP-сервер не запущен");
    await new Promise<void>((resolve, reject) => {
      child.stdin.write(`${JSON.stringify(message)}\n`, (error) =>
        error ? reject(error) : resolve(),
      );
    });
  }

  async close(): Promise<void> {
    this.#closing = true;
    const child = this.#child;
    if (!child || child.exitCode !== null) return;
    child.stdin.end();
    // A server that ignores a closed stdin gets a moment, then the signal.
    const exited = new Promise<void>((resolve) => child.once("exit", () => resolve()));
    const grace = setTimeout(() => killTree(child), 2_000);
    grace.unref();
    try {
      await exited;
    } finally {
      clearTimeout(grace);
    }
  }

  #cannotStart(cause: unknown): AppError {
    const args = (this.options.args ?? []).join(" ");
    const code = (cause as NodeJS.ErrnoException | undefined)?.code;
    const reason =
      code === "ENOENT"
        ? ` — команда «${this.options.command}» не найдена (ENOENT)`
        : code === undefined
          ? ""
          : ` — ${code}`;
    return new AppError(
      AppErrorCode.MCP_UNAVAILABLE,
      `Не удалось запустить MCP-сервер: ${[this.options.command, args].join(" ").trim()}${reason}`,
      { cause, details: { command: this.options.command } },
    );
  }

  #end(reason: string): void {
    const handlers = this.#handlers;
    this.#handlers = undefined;
    this.#child = undefined;
    handlers?.closed(reason);
  }

  #receive(chunk: string): void {
    this.#buffer += chunk;
    if (this.#buffer.length > MAX_LINE_BYTES) {
      this.#buffer = "";
      this.#handlers?.log("error", "Сервер прислал строку сверх допустимого размера");
      if (this.#child) killTree(this.#child);
      return;
    }
    let newline = this.#buffer.indexOf("\n");
    while (newline >= 0) {
      const line = this.#buffer.slice(0, newline).trim();
      this.#buffer = this.#buffer.slice(newline + 1);
      if (line) this.#decode(line);
      newline = this.#buffer.indexOf("\n");
    }
  }

  #decode(line: string): void {
    try {
      this.#handlers?.message(JSON.parse(line));
    } catch {
      // Host-side only: a malformed frame may quote whatever the server was handling.
      this.#handlers?.log("error", `Нечитаемый ответ: ${line.slice(0, 500)}`);
    }
  }
}
