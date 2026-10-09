import type { z } from "zod";
import type { ApprovalRequestDto, HostEventDraft, Json, RunId } from "@zvs/shared";
import type { ProviderRegistry } from "../drivers/ai/ProviderRegistry.ts";
import type { VectorStoreService } from "../services/VectorStoreService.ts";
import type { SidecarJobsPort } from "../drivers/sidecar/SidecarDriver.ts";
import type { IndexingService } from "../indexing/IndexingService.ts";
import type { DownloadService } from "../downloads/DownloadService.ts";
import type { GraphService } from "../services/GraphService.ts";

export type PermissionRequirement =
  { tool: string; tier?: "auto" | "ask" | "off" } | { kind: "none" };
export type ApprovalDecision = "approved" | "denied";
export type AdmissionOrigin = "graph" | "model";
export interface ToolStepRequest {
  key: string;
  input: Json;
  label: string;
  minimumTier?: "auto" | "ask";
  onApproval?: (request: ApprovalRequestDto, decision: ApprovalDecision | null) => void;
}
export interface ToolStepResult {
  stepId: string;
  output: Json;
}
export interface KernelServices {
  providers?: Pick<ProviderRegistry, "ephemeralDriver" | "text">;
  vectorStores?: Pick<VectorStoreService, "search"> &
    Partial<Pick<VectorStoreService, "chunks" | "relatedChunks" | "list">>;
  graph?: Pick<GraphService, "neighbors">;
  jobs?: SidecarJobsPort;
  indexing?: Pick<IndexingService, "index">;
  downloads?: Pick<DownloadService, "execute">;
}
export interface StepContext {
  runId: RunId;
  signal: AbortSignal;
  emit(event: HostEventDraft): void;
  requestApproval(req: ApprovalRequestDto): Promise<ApprovalDecision>;
  invokeTool?(request: ToolStepRequest): Promise<ToolStepResult>;
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
