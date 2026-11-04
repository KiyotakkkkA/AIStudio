import { z } from "zod";
import type { BrowserAppearance } from "./browser.js";
import { FolderAccessAnswer, FolderGrantChatDto, FolderGrantScope } from "./folders.js";
import type { ChatToolCallStatus } from "./chat.js";
import { RunId } from "./primitives/branded.js";

const approvalId = z.string().uuid();

export const APPS_APPROVAL_KIND = "apps";

export const ComputerAppDto = z.object({
  app: z.string().min(1).max(128),
  path: z.string().max(32767),
  icon: z.string().max(262_144).optional(),
});
export type ComputerAppDto = z.infer<typeof ComputerAppDto>;

export const AppGrantDto = z
  .object({
    id: z.string().uuid(),
    app: z.string().min(1).max(128),
    path: z.string().max(32767),
    icon: z.string().max(262_144).nullable(),
    scope: FolderGrantScope,
    conversation: FolderGrantChatDto.nullable(),
    grantedAt: z.number().int(),
    lastUsedAt: z.number().int().nullable(),
  })
  .refine((grant) => (grant.scope === "conversation") === (grant.conversation !== null), {
    path: ["conversation"],
  });
export type AppGrantDto = z.infer<typeof AppGrantDto>;

export const AppGrantRef = z.object({ id: z.string().uuid() });
export type AppGrantRef = z.infer<typeof AppGrantRef>;

export const AnswerAppRequestInput = z.object({
  id: RunId,
  approvalId: z.string().uuid(),
  answer: FolderAccessAnswer,
});
export type AnswerAppRequestInput = z.infer<typeof AnswerAppRequestInput>;

export const ComputerCommand = z.discriminatedUnion("action", [
  z.object({ action: z.literal("state") }),
  z.object({ action: z.literal("stop") }),
  z.object({ action: z.literal("approve"), id: approvalId }),
  z.object({ action: z.literal("deny"), id: approvalId }),
  z.object({ action: z.literal("answer"), id: approvalId, answer: FolderAccessAnswer }),
  z
    .object({ action: z.literal("say"), text: z.string().max(100_000) })
    .refine((command) => command.text.trim().length > 0, { path: ["text"] }),
  z.object({ action: z.literal("engage") }),
  z.object({ action: z.literal("release") }),
  z.object({ action: z.literal("collapse"), collapsed: z.boolean() }),
]);
export type ComputerCommand = z.infer<typeof ComputerCommand>;

export type ComputerStatus = "idle" | "starting" | "working" | "awaiting" | "stopping";
export type ComputerActionState = "running" | "waiting" | "done" | "failed" | "denied";

export interface ComputerAction {
  id: number;
  tool: string;
  label: string;
  state: ComputerActionState;
  startedAt: number;
  error?: string;
}

export interface ComputerApproval {
  id: string;
  kind: "access" | "action";
  tool: string;
  action: string;
  apps: ComputerAppDto[];
  detail?: string;
}

export interface ComputerDisplay {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ComputerState {
  active: boolean;
  status: ComputerStatus;
  conversationId?: string;
  title?: string;
  display?: ComputerDisplay;
  apps: string[];
  actions: ComputerAction[];
  approval?: ComputerApproval;
  collapsed: boolean;
  appearance: BrowserAppearance;
  startedAt?: number;
  reply: string;
  calls: ComputerCall[];
  requests: ComputerRequest[];
}

export interface ComputerCall {
  id: string;
  key: string;
  status: ChatToolCallStatus;
  summary?: string;
}

export interface ComputerRequest {
  id: string;
  kind: string;
  tool: string;
  action: string;
  reason?: string;
  detail?: string;
}

export interface ComputerPointer {
  x: number;
  y: number;
  down: boolean;
}
