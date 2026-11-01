import { useState } from "react";
import { observer } from "mobx-react-lite";
import { mdiCheck, mdiChevronDown, mdiClose } from "@mdi/js";
import type { AgenticPlanStepDto } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import type ChatStore from "./ChatStore";
import { GOAL_PLAN, GOAL_PLAN_LABEL, planProgressText } from "./goalCopy";

const TITLE_TONE: Record<AgenticPlanStepDto["status"], string> = {
  done: "text-main-300",
  failed: "text-main-300",
  running: "font-semibold text-main-50",
  queued: "text-main-400",
};

function StepBadge({
  status,
  number,
}: {
  readonly status: AgenticPlanStepDto["status"];
  readonly number: number;
}) {
  const base =
    "mt-px flex size-4.5 flex-none items-center justify-center rounded-full font-mono text-[9.5px] font-semibold";
  if (status === "done")
    return (
      <span className={`${base} bg-ok-soft text-ok`}>
        <Icon path={mdiCheck} size={11} />
      </span>
    );
  if (status === "failed")
    return (
      <span className={`${base} bg-err-soft text-err`}>
        <Icon path={mdiClose} size={11} />
      </span>
    );
  if (status === "running")
    return <span className={`${base} bg-accent-dark text-on-accent`}>{number}</span>;
  return <span className={`${base} border-[1.5px] border-main-600 text-main-500`}>{number}</span>;
}

export default observer(function GoalPlan({ store }: { readonly store: ChatStore }) {
  const [open, setOpen] = useState(true);
  const plan = store.goals.plan;
  if (plan.length === 0) return null;
  return (
    <section className="flex flex-col gap-2">
      <button
        type="button"
        aria-expanded={open}
        aria-controls="context-goal-plan"
        onClick={() => setOpen(!open)}
        className="-mx-1.5 flex cursor-pointer items-center gap-2 rounded-control px-1.5 py-1 text-left text-main-500 hover:bg-main-700 hover:text-main-300"
      >
        <span className="min-w-0 flex-1 text-[10.5px] font-semibold tracking-[0.12em] uppercase">
          {GOAL_PLAN}
        </span>
        <Chip>{planProgressText(store.goals.planDone, plan.length)}</Chip>
        <Icon
          path={mdiChevronDown}
          size={15}
          className={`flex-none transition-transform ${open ? "" : "-rotate-90"}`}
        />
      </button>
      {open ? (
        <ol
          id="context-goal-plan"
          aria-label={GOAL_PLAN_LABEL}
          className="m-0 flex list-none flex-col gap-1 p-0"
        >
          {plan.map((step, index) => (
            <li
              key={step.id}
              aria-current={step.status === "running" ? "step" : undefined}
              className={`flex items-start gap-2.5 rounded-control px-3 py-2 ${
                step.status === "running" ? "bg-accent-soft" : "bg-main-900"
              }`}
            >
              <StepBadge status={step.status} number={index + 1} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span
                  className={`text-[12.5px] leading-snug wrap-break-word ${TITLE_TONE[step.status]}`}
                >
                  {step.title}
                </span>
                {step.summary === null ? null : (
                  <span
                    className="mt-0.5 line-clamp-2 text-[11px] text-main-500"
                    title={step.summary}
                  >
                    {step.summary}
                  </span>
                )}
              </span>
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
});
