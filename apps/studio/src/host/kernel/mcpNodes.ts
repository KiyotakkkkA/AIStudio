import { z } from "zod";
import { McpCallResultDto, type Json } from "@zvs/shared";
import type { NodeRegistry } from "./NodeRegistry.ts";

export interface McpNodeTarget {
  /** The registry key, which is also the node type — so admission finds the same ceiling. */
  key: string;
  serverId: string;
  /** The name the server itself knows the tool by, before namespacing. */
  tool: string;
  readOnly: boolean;
  timeoutMs?: number;
}

export type McpToolCaller = (
  target: Pick<McpNodeTarget, "serverId" | "tool" | "timeoutMs">,
  args: Readonly<Record<string, Json>>,
  signal: AbortSignal,
) => Promise<McpCallResultDto>;

export const McpNodeInput = z.record(z.string().max(200), z.json()).default({});

/**
 * A discovered tool becomes a node under its own registry key, so the kernel admits it through
 * the ceiling every other tool goes through — nothing about MCP gets its own permission path.
 * Registration replaces whatever a previous handshake left behind.
 */
export function registerMcpToolNode(
  nodes: NodeRegistry,
  target: McpNodeTarget,
  call: McpToolCaller,
): void {
  nodes.unregister(target.key);
  nodes.register({
    type: target.key,
    input: McpNodeInput,
    output: McpCallResultDto,
    permission: { tool: target.key },
    sideEffectFree: target.readOnly,
    async run(context, input) {
      context.signal.throwIfAborted();
      return call(
        {
          serverId: target.serverId,
          tool: target.tool,
          ...(target.timeoutMs === undefined ? {} : { timeoutMs: target.timeoutMs }),
        },
        input,
        context.signal,
      );
    },
  });
}
