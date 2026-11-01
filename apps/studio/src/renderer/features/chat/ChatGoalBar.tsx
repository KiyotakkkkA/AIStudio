import { observer } from "mobx-react-lite";
import { mdiFlagCheckered, mdiFlagOutline } from "@mdi/js";
import type { GoalStatus } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import type { StatusTone } from "../../ui/atoms/statusTone";
import type ChatStore from "./ChatStore";
import {
  GOAL_CONTINUE,
  GOAL_DISMISS,
  GOAL_DONE,
  GOAL_LABEL,
  GOAL_STOP,
  GOAL_STOPPING,
  goalMinutesText,
  goalRoundText,
  goalStatusText,
  goalTokensText,
} from "./goalCopy";

const TONE: Record<GoalStatus, StatusTone> = {
  active: "accent",
  paused: "warn",
  done: "ok",
  stopped: "idle",
  failed: "err",
};

export default observer(function ChatGoalBar({ store }: { readonly store: ChatStore }) {
  const goals = store.goals;
  const goal = goals.goal;
  if (goal === null || !goals.visible) return null;

  if (goal.status === "done") {
    return (
      <div
        role="status"
        className="flex min-w-0 items-center gap-2.5 py-1.5 pr-1.5 pl-3 text-[12.5px]"
      >
        <Icon path={mdiFlagCheckered} size={16} className="shrink-0 text-ok" />
        <span className="shrink-0 font-medium text-main-100">{GOAL_DONE}</span>
        <span className="min-w-0 flex-1 truncate text-main-400" title={goal.text}>
          {goal.text}
        </span>
        <Button tone="ghost" size="sm" onClick={goals.dismiss}>
          {GOAL_DISMISS}
        </Button>
      </div>
    );
  }

  const status = goalStatusText(goal);
  return (
    <section aria-label={GOAL_LABEL} className="px-3 py-2 text-[12.5px]">
      <div className="flex min-w-0 items-center gap-2.5">
        <Icon path={mdiFlagOutline} size={16} className="shrink-0 text-accent-medium" />
        <span className="min-w-0 flex-1 truncate font-medium text-main-100" title={goal.text}>
          {goal.text}
        </span>
        {goal.status === "active" ? (
          <Button
            tone="danger-soft"
            size="sm"
            disabled={goals.busy}
            onClick={() => void goals.stop()}
          >
            {goals.busy ? GOAL_STOPPING : GOAL_STOP}
          </Button>
        ) : null}
        {goal.status === "paused" ? (
          <>
            <Button tone="ghost" size="sm" disabled={goals.busy} onClick={() => void goals.stop()}>
              {GOAL_STOP}
            </Button>
            <Button
              tone="primary"
              size="sm"
              disabled={goals.busy || store.generating}
              onClick={() => void store.continueGoal()}
            >
              {GOAL_CONTINUE}
            </Button>
          </>
        ) : null}
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11.5px] text-main-400">
        <span
          className={`inline-flex min-w-0 items-center gap-1.5 font-medium text-main-200 ${
            goal.status === "active" ? "zvs-shimmer-text" : ""
          }`}
        >
          <StatusDot tone={TONE[goal.status]} />
          <span className="truncate">{status}</span>
        </span>
        <span className="tabular-nums">{goalRoundText(goal.usage.rounds, goal.budget.rounds)}</span>
        <span className="tabular-nums">
          {goalTokensText(goal.usage.tokens, goal.budget.tokens)}
        </span>
        <span className="tabular-nums">{goalMinutesText(goals.elapsedMs)}</span>
      </div>
      {goals.error === null ? null : (
        <p role="alert" className="mt-1 text-[11.5px] text-err">
          {goals.error}
        </p>
      )}
    </section>
  );
});
