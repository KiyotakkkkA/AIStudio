import type { BrowserAgentApproval, BrowserCommand } from "@zvs/shared";
import { mdiAlertOutline } from "@mdi/js";
import Icon from "./Icon";
import {
  ALWAYS,
  APPROVE_ONCE,
  DENY,
  approvalDetail,
  approvalTitle,
  offersAlways,
} from "./agentCopy";

interface ApprovalBarProps {
  readonly approval: BrowserAgentApproval;
  readonly busy: boolean;
  readonly send: (command: BrowserCommand) => void;
}

export default function ApprovalBar({ approval, busy, send }: ApprovalBarProps) {
  return (
    <div className="approval-bar" role="alertdialog" aria-labelledby="approval-title">
      <span className="approval-mark" aria-hidden="true">
        <Icon path={mdiAlertOutline} size={19} />
      </span>
      <div className="approval-text">
        <strong id="approval-title">{approvalTitle(approval)}</strong>
        <code>{approvalDetail(approval)}</code>
      </div>
      <button
        type="button"
        className="ghost"
        disabled={busy}
        onClick={() => send({ action: "agentDeny", id: approval.id })}
      >
        {DENY}
      </button>
      {offersAlways(approval) ? (
        <button
          type="button"
          disabled={busy}
          onClick={() => send({ action: "agentAlways", id: approval.id })}
        >
          {ALWAYS}
        </button>
      ) : null}
      <button
        type="button"
        className="primary"
        disabled={busy}
        onClick={() => send({ action: "agentApprove", id: approval.id })}
      >
        {APPROVE_ONCE}
      </button>
    </div>
  );
}
