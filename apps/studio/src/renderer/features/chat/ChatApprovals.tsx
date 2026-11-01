import type { CSSProperties } from "react";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import type ChatStore from "./ChatStore";
import FadeIn from "../../ui/motion/FadeIn";
import { isBuiltinTool, toolTitle } from "./toolCallCopy";
import { subagentApprovalTitle } from "../subagents/subagentCopy";

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
              Разрешить{" "}
              {request.subject === "llm.generate"
                ? "генерацию ответа"
                : request.subject === "vector.search"
                  ? "поиск в подключённом хранилище"
                  : isBuiltinTool(request.subject)
                    ? `«${toolTitle(request.subject)}»`
                    : request.subject}{" "}
              для этого запроса?
            </p>
            <div className="flex gap-2">
              <Button
                size="sm"
                disabled={request.busy}
                tone="primary"
                onClick={() => void store.decideApproval(request.id, true)}
              >
                Разрешить один раз
              </Button>
              <Button
                size="sm"
                disabled={request.busy}
                onClick={() => void store.decideApproval(request.id, false)}
              >
                Запретить
              </Button>
            </div>
          </div>
        </FadeIn>
      ))}
    </>
  );
});
