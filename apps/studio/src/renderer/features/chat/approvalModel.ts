import {
  APPS_APPROVAL_KIND,
  ComputerAppDto,
  FilesApprovalDto,
  ProcessApprovalDto,
  type ApprovalRequestDto,
  type ExecMode,
  type FileDiffDto,
  type ProcessPlanDto,
} from "@zvs/shared";
import { mailPreview, type MailPreview } from "../mail/mailCopy";

export interface ApprovalView {
  readonly id: string;
  readonly runId: string;
  readonly subject: string;
  readonly changes: FileDiffDto[];
  readonly process?: ProcessPlanDto;
  readonly mail?: MailPreview;
  readonly apps?: ComputerAppDto[];
  readonly detail?: string;
}

export function approvalView(request: ApprovalRequestDto): ApprovalView | null {
  const id = request.id;
  const subject = request.subject;
  const runId = request.runId;
  if (typeof id !== "string" || typeof subject !== "string" || typeof runId !== "string") {
    return null;
  }
  const files = FilesApprovalDto.safeParse(request);
  const process = ProcessApprovalDto.safeParse(request);
  const mail = mailPreview(request);
  const apps =
    request.kind === APPS_APPROVAL_KIND ? ComputerAppDto.array().safeParse(request.apps) : null;
  const detail = typeof request.detail === "string" ? request.detail : undefined;
  return {
    id,
    runId,
    subject,
    changes: files.success ? files.data.changes : [],
    ...(process.success ? { process: process.data.process } : {}),
    ...(mail === null ? {} : { mail }),
    ...(apps?.success === true
      ? { apps: apps.data, ...(detail === undefined ? {} : { detail }) }
      : {}),
  };
}

export function alwaysAllowable(
  approval: { readonly changes?: readonly FileDiffDto[]; readonly process?: ProcessPlanDto },
  mode: ExecMode,
): boolean {
  const process = approval.process;
  if (process !== undefined) {
    return (
      (mode === "safety-ask" || mode === "auto-bypass") &&
      process.language === undefined &&
      process.scriptString !== true &&
      process.argv.length > 0
    );
  }
  return mode === "auto-bypass" && (approval.changes?.length ?? 0) > 0;
}
