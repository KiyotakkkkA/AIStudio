import type { z } from "zod";
import type { ApprovalRequestDto, HostEventDraft, RunId } from "@zvs/shared";
import type { ProviderRegistry } from "../drivers/ai/ProviderRegistry.ts";
import type { VectorStoreService } from "../services/VectorStoreService.ts";
import type { SidecarJobsPort } from "../drivers/sidecar/SidecarDriver.ts";

export type PermissionRequirement =
  { tool: string; tier?: "auto" | "ask" | "off" } | { kind: "none" };
export type ApprovalDecision = "approved" | "denied";
export interface KernelServices {
  providers?: Pick<ProviderRegistry, "ephemeralDriver" | "text">;
  vectorStores?: Pick<VectorStoreService, "search">;
  jobs?: SidecarJobsPort;
}
export interface StepContext {
  runId: RunId;
  signal: AbortSignal;
  emit(event: HostEventDraft): void;
  requestApproval(req: ApprovalRequestDto): Promise<ApprovalDecision>;
  services: KernelServices;
}
export type NodeDef<I, O> = {
  type: string;
  input: z.ZodType<I>;
  output: z.ZodType<O>;
  permission: PermissionRequirement;
  run(ctx: StepContext, input: I): Promise<O>;
} & (
  { sideEffect: boolean; sideEffectFree?: never } | { sideEffectFree: boolean; sideEffect?: never }
);
