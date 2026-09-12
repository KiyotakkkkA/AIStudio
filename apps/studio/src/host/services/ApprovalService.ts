import { AppError, AppErrorCode, type ApprovalRequestDto } from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { ApprovalDecision } from "../kernel/types.ts";
import type { PermissionService } from "./PermissionService.ts";
import { createId } from "../platform/ids.ts";

export class ApprovalService {
  private readonly waiting = new Map<string, (decision: ApprovalDecision) => void>();
  constructor(
    private readonly data: UnitOfWork,
    private readonly permissions: PermissionService,
    private readonly timeoutMs = 60_000,
    private readonly clock: () => number = Date.now,
  ) {
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 2_147_483_647)
      throw new Error("Invalid approval timeout");
  }
  create(runId: string, subject: string, scope: string): ApprovalRequestDto {
    const request = {
      id: createId(),
      runId,
      subject,
      scope,
      expiresAt: this.clock() + this.timeoutMs,
    };
    this.data.repositories.permissions.createApproval(request);
    return request;
  }
  wait(request: ApprovalRequestDto, signal: AbortSignal): Promise<ApprovalDecision> {
    const id = String(request.id);
    const row = this.data.repositories.permissions.approval(id);
    if (!row) return Promise.resolve("denied");
    if (row.decision) return Promise.resolve(row.decision);
    return new Promise((resolve) => {
      const finish = (decision: ApprovalDecision) => {
        clearTimeout(timer);
        signal.removeEventListener("abort", abort);
        this.waiting.delete(id);
        this.data.repositories.permissions.decide(id, decision);
        resolve(decision);
      };
      const abort = () => finish("denied");
      const timer = setTimeout(abort, Math.max(0, row.expiresAt - this.clock()));
      this.waiting.set(id, finish);
      signal.addEventListener("abort", abort, { once: true });
      if (signal.aborted) abort();
    });
  }
  decide(runId: string, id: string, decision: ApprovalDecision, always = false): void {
    const row = this.data.repositories.permissions.approval(id);
    if (!row || row.runId !== runId || row.decision || row.expiresAt <= this.clock())
      throw new AppError(AppErrorCode.CONFLICT, "Approval is no longer pending");
    const run = this.data.repositories.runs.get(runId);
    const scopes = [
      ...(run?.graph.permissionScopes ?? []),
      ...(row.scope === "global" ? [] : [row.scope]),
    ];
    if (
      decision === "approved" &&
      this.permissions.admitSubject(row.subject, { runId, scopes }) === "deny"
    )
      throw new AppError(AppErrorCode.PERMISSION_DENIED, "Permission revoked");
    this.data.transaction(() => {
      if (always && decision === "approved")
        this.permissions.grant(row.subject, row.scope, "auto", "user");
      this.data.repositories.permissions.decide(id, decision);
    });
    this.waiting.get(id)?.(decision);
  }
}
