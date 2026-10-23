import { mdiShieldAlertOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import type ScenarioEditor from "./ScenarioEditor";
import { APPROVAL_PROMPT } from "./runCopy";

function ApprovalBar({ editor }: { readonly editor: ScenarioEditor }) {
  const run = editor.execution;
  const approval = run.approval;
  if (approval === null) return null;
  return (
    <div
      role="alert"
      className="absolute top-4 left-1/2 z-20 flex max-w-[min(640px,calc(100%-120px))] -translate-x-1/2 items-center gap-2.5 rounded-control border border-line bg-main-800 px-3 py-2 text-[12px] text-main-100 shadow-pop"
    >
      <Icon path={mdiShieldAlertOutline} size={16} />
      <span className="min-w-0 flex-1">
        {`${APPROVAL_PROMPT} «${editor.labelOf(approval.subject)}»`}
      </span>
      <Button
        type="button"
        tone="primary"
        disabled={run.deciding}
        onClick={() => void run.decide(true)}
      >
        Разрешить
      </Button>
      <Button
        type="button"
        tone="secondary"
        disabled={run.deciding}
        onClick={() => void run.decide(true, true)}
      >
        Разрешать всегда
      </Button>
      <Button
        type="button"
        tone="ghost"
        disabled={run.deciding}
        onClick={() => void run.decide(false)}
      >
        Отклонить
      </Button>
    </div>
  );
}

export default observer(ApprovalBar);
