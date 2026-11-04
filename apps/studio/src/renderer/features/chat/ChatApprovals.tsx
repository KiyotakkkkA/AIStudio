import type { CSSProperties } from "react";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import type ChatStore from "./ChatStore";
import FadeIn from "../../ui/motion/FadeIn";
import { ALLOW_ONCE, FORBID, approvalQuestion } from "./toolCallCopy";
import { subagentApprovalTitle } from "../subagents/subagentCopy";
import MailDraftView from "../mail/MailDraftView";

export default observer(function ChatApprovals({ store }: { readonly store: ChatStore }) {
  return (
    <>
      {store.standaloneApprovals.map((request) => (
        <FadeIn key={request.id}>
          <div
            role="status"
            style={{ "--zvs-beam-color": "var(--color-warn)" } as CSSProperties}
            className="zvs-beam flex flex-wrap items-center gap-3 bg-warn-soft px-3.5 py-3"
          >
            <p className="min-w-0 flex-1 text-[13px] text-main-100">
              {request.subagent === null ? null : (
                <span className="block text-[12px] font-medium text-warn">
                  {subagentApprovalTitle(request.subagent.label)}
                </span>
              )}
              {approvalQuestion(request.subject)}
            </p>
            {request.mail === undefined ? null : (
              <div className="w-full">
                <MailDraftView preview={request.mail} expanded />
              </div>
            )}
            <div className="flex gap-2">
              <Button
                size="sm"
                disabled={request.busy}
                tone="primary"
                onClick={() => void store.decideApproval(request.id, true)}
              >
                {ALLOW_ONCE}
              </Button>
              <Button
                size="sm"
                disabled={request.busy}
                onClick={() => void store.decideApproval(request.id, false)}
              >
                {FORBID}
              </Button>
            </div>
          </div>
        </FadeIn>
      ))}
    </>
  );
});
