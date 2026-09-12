import type { z } from "zod";
import type { ApprovalRequestDto, HostEvent, RunId } from "@zvs/shared";
import type { ProviderRegistry } from "../drivers/ai/ProviderRegistry.ts";
import type { VectorStoreService } from "../services/VectorStoreService.ts";

export type PermissionRequirement = { tool: string } | { kind: "none" };
export type ApprovalDecision = "approved" | "denied";
export interface KernelServices {
  providers?: Pick<ProviderRegistry, "ephemeralDriver">;
  vectorStores?: Pick<VectorStoreService, "search">;
}
export interface StepContext {
  runId: RunId;
  signal: AbortSignal;
  emit(event: HostEvent): void;
  requestApproval(req: ApprovalRequestDto): Promise<ApprovalDecision>;
  services: KernelServices;
}
export interface NodeDef<I, O> {
  type: string;
  input: z.ZodType<I>;
  output: z.ZodType<O>;
  permission: PermissionRequirement;
  sideEffectFree: boolean;
  run(ctx: StepContext, input: I): Promise<O>;
}
