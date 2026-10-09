import {
  AppError,
  AppErrorCode,
  RunId,
  ToolInvokeInput,
  ToolInvokeResultDto,
  toolNeedsConfirmation,
  type Json,
  type ToolInvokeRequest,
} from "@zvs/shared";
import type { NodeRegistry } from "../kernel/NodeRegistry.ts";
import type { RunService } from "../services/RunService.ts";
import type { ToolRegistry } from "./ToolRegistry.ts";

export interface ToolInvokerOptions {
  tools: Pick<ToolRegistry, "get">;
  nodes: Pick<NodeRegistry, "list">;
  runs: Pick<RunService, "start" | "subscribe" | "approve" | "wait" | "get" | "steps">;
  clock?: () => number;
}

const NODE_ID = "tool";

export class ToolInvoker {
  constructor(private readonly options: ToolInvokerOptions) {}

  async invoke(raw: ToolInvokeRequest): Promise<ToolInvokeResultDto> {
    const request = ToolInvokeInput.parse(raw);
    const tool = this.options.tools.get(request.id);
    if (!tool.available)
      throw new AppError(
        tool.source === "mcp" ? AppErrorCode.MCP_UNAVAILABLE : AppErrorCode.CONFLICT,
        "Инструмент сейчас недоступен: его источник отключён или удалён",
        { details: { key: tool.key } },
      );
    if (!tool.enabled || tool.tier === "off")
      throw new AppError(
        AppErrorCode.PERMISSION_DENIED,
        "Инструмент выключен: разрешение «off» запрещает любой вызов",
        { details: { key: tool.key } },
      );
    if (toolNeedsConfirmation(tool) && !request.confirmed)
      throw new AppError(AppErrorCode.APPROVAL_DENIED, "Вызов требует явного подтверждения", {
        details: { key: tool.key },
      });

    const type = this.nodeType(tool.key);
    const startedAt = this.now();
    const { runs } = this.options;
    const handle = runs.start({
      kind: "job",
      title: `Проба инструмента ${tool.key}`,
      graph: {
        nodes: [
          {
            id: NODE_ID,
            type,
            dependencies: [],
            input: request.input,
            bindings: {},
            retry: { maxAttempts: 1, backoffMs: 0 },
          },
        ],
      },
      input: null,
      concurrency: 1,
    });
    const unsubscribe = runs.subscribe(handle.id, (event) => {
      if (event.type !== "approval" || !request.confirmed) return;
      const approvalId = event.request.id;
      if (typeof approvalId !== "string") return;
      try {
        runs.approve(handle.id, approvalId);
      } catch {
        return;
      }
    });
    try {
      await runs.wait(handle.id);
    } finally {
      unsubscribe();
    }

    const run = runs.get(handle.id);
    const step = runs
      .steps(handle.id)
      .filter((entry) => entry.nodeId === NODE_ID)
      .at(-1);
    const output: Json = step?.output ?? null;
    const reported = reportedError(output);
    const succeeded = run.status === "succeeded";
    return ToolInvokeResultDto.parse({
      runId: RunId.parse(handle.id),
      ok: succeeded && reported === null,
      durationMs: Math.max(
        0,
        Math.round(
          step === undefined
            ? this.now() - startedAt
            : (step.finishedAt ?? this.now()) - step.startedAt,
        ),
      ),
      output,
      error: succeeded ? reported : (step?.error ?? run.error ?? "Инструмент завершился с ошибкой"),
    });
  }

  private nodeType(key: string): string {
    const candidates = this.options.nodes
      .list()
      .filter((node) => "tool" in node.permission && node.permission.tool === key);
    const chosen = candidates.find((node) => node.type === key) ?? candidates[0];
    if (chosen === undefined)
      throw new AppError(
        AppErrorCode.CONFLICT,
        "Инструмент нельзя вызвать: источник не предоставил для него исполняемый узел",
        { details: { key } },
      );
    return chosen.type;
  }

  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
}

function reportedError(output: Json): string | null {
  if (output === null || typeof output !== "object" || Array.isArray(output)) return null;
  if (output.isError !== true) return null;
  const text = typeof output.text === "string" ? output.text.trim() : "";
  return text.length > 0 ? text : "Инструмент вернул ошибку";
}
