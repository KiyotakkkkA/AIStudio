import { Fragment, useId, useState, type CSSProperties, type ReactNode } from "react";
import { observer } from "mobx-react-lite";
import { mdiChevronDown, mdiRobotOutline, mdiStopCircleOutline } from "@mdi/js";
import type { ChatToolCallDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import Collapse from "../../ui/motion/Collapse";
import ChatMarkdown from "./ChatMarkdown";
import { CATEGORY_BADGE, toolPresentation } from "./toolCallCopy";
import { subagentMeta, type SubagentCompactStep, type SubagentMeta } from "./SubagentSteps";
import {
  SUBAGENT_FAILED,
  SUBAGENT_GROUP,
  SUBAGENT_PARTIAL,
  SUBAGENT_PRESET_LABELS,
  SUBAGENT_QUEUED,
  SUBAGENT_REPORT,
  SUBAGENT_RUNNING,
  SUBAGENT_STEPS_HIDE,
  SUBAGENT_STEPS_SHOW,
  SUBAGENT_STOP,
  SUBAGENT_STOPPED,
  subagentApprovalTitle,
  subagentDuration,
  subagentGroupTitle,
  subagentTokens,
  subagentTools,
} from "../subagents/subagentCopy";
import type { ToolCallApproval } from "./ChatToolCalls";

export interface SubagentControls {
  readonly stepsFor: (runId: string | undefined) => readonly ChatToolCallDto[];
  readonly onStop: (runId: string) => void;
  readonly stopping: readonly string[];
  readonly renderStep: (call: ChatToolCallDto, approval: ToolCallApproval | undefined) => ReactNode;
  readonly approvalFor?: ((id: string | null) => ToolCallApproval | undefined) | undefined;
}

const STATUS_TONES: Record<SubagentMeta["status"], StatusTone> = {
  queued: "idle",
  running: "accent",
  ok: "ok",
  partial: "warn",
  cancelled: "idle",
  failed: "err",
};

function statusLabel(meta: SubagentMeta): string {
  switch (meta.status) {
    case "queued":
      return SUBAGENT_QUEUED;
    case "running":
      return SUBAGENT_RUNNING;
    case "cancelled":
      return SUBAGENT_STOPPED;
    case "failed":
      return SUBAGENT_FAILED;
    default:
      return "";
  }
}

function firstLine(text: string): string {
  return text.trim().split("\n")[0]?.trim() ?? "";
}

function CompactStep({ step }: { readonly step: SubagentCompactStep }) {
  const presentation = toolPresentation(step.key);
  return (
    <li className="flex min-w-0 items-center gap-2 px-1 py-0.5 text-[11.5px]">
      <span
        aria-hidden="true"
        className={`flex size-5 shrink-0 items-center justify-center rounded-pill ${CATEGORY_BADGE[presentation.category]}`}
      >
        <Icon path={presentation.icon} size={11} />
      </span>
      <span className="min-w-0 flex-1 truncate text-main-200">{presentation.title}</span>
      {step.error === undefined ? null : (
        <span className="max-w-[50%] truncate text-err" title={step.error}>
          {step.error}
        </span>
      )}
    </li>
  );
}

const SubagentCard = observer(function SubagentCard({
  call,
  controls,
  live,
}: {
  readonly call: ChatToolCallDto;
  readonly controls: SubagentControls | undefined;
  readonly live: boolean;
}) {
  const stepsId = useId();
  const [showSteps, setShowSteps] = useState<boolean | null>(null);
  const meta = subagentMeta(call);
  if (meta === null) return null;
  const running = meta.status === "running" || meta.status === "queued";
  const liveSteps = controls?.stepsFor(meta.runId) ?? [];
  const pending = liveSteps.filter((step) => step.status === "awaiting-approval");
  const open = showSteps ?? (running || pending.length > 0);
  const stepCount = running ? liveSteps.length : meta.toolCalls;
  const label = meta.label || firstLine(meta.task);
  const tone = STATUS_TONES[meta.status];
  const status = statusLabel(meta);
  const stopping = meta.runId !== undefined && controls?.stopping.includes(meta.runId) === true;
  return (
    <li
      style={{ "--zvs-beam-color": "var(--color-warn)" } as CSSProperties}
      className={`relative overflow-hidden rounded-card bg-main-800 ${pending.length > 0 ? "zvs-beam" : ""}`}
    >
      <div className="flex min-h-9 flex-wrap items-center gap-2.5 px-3 py-1.5 text-[12px]">
        <span
          aria-hidden="true"
          className={`flex size-6 shrink-0 items-center justify-center rounded-pill ${CATEGORY_BADGE.dialog}`}
        >
          <Icon path={mdiRobotOutline} size={13} />
        </span>
        <span className="shrink-0 text-[12.5px] font-medium text-main-100">
          {SUBAGENT_PRESET_LABELS[meta.preset]}
        </span>
        <span className="min-w-0 flex-1 truncate text-[12px] text-main-300" title={meta.task}>
          {label}
        </span>
        <Chip title={meta.model}>
          <span className="max-w-40 truncate font-mono">{meta.model}</span>
        </Chip>
        {meta.partial && !running ? <Chip tone="accent">{SUBAGENT_PARTIAL}</Chip> : null}
        {status === "" ? null : (
          <span
            className={`shrink-0 text-[11px] font-medium ${STATUS_TONE_TEXT[tone]} ${
              running ? "zvs-shimmer-text" : ""
            }`}
          >
            {status}
          </span>
        )}
        {running && meta.runId !== undefined && controls !== undefined && live ? (
          <Button
            size="sm"
            tone="ghost"
            disabled={stopping}
            onClick={() => {
              if (meta.runId !== undefined) controls.onStop(meta.runId);
            }}
          >
            <Icon path={mdiStopCircleOutline} size={14} />
            {SUBAGENT_STOP}
          </Button>
        ) : null}
      </div>
      <div className="space-y-2 px-3 pb-3 text-[12px]">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] text-main-500 tabular-nums">
          <button
            type="button"
            aria-expanded={open}
            aria-controls={stepsId}
            disabled={stepCount === 0}
            onClick={() => setShowSteps(!open)}
            className="inline-flex cursor-pointer items-center gap-1 text-main-400 hover:text-main-100 disabled:cursor-default disabled:hover:text-main-400"
          >
            {subagentTools(stepCount)}
            {stepCount > 0 ? (
              <>
                <span className="sr-only">{open ? SUBAGENT_STEPS_HIDE : SUBAGENT_STEPS_SHOW}</span>
                <Icon
                  path={mdiChevronDown}
                  size={13}
                  className={`anim-on:transition-transform ${open ? "rotate-180" : ""}`}
                />
              </>
            ) : null}
          </button>
          {!running && meta.tokens > 0 ? <span>{subagentTokens(meta.tokens)}</span> : null}
          {!running && meta.durationMs !== undefined ? (
            <span>{subagentDuration(meta.durationMs)}</span>
          ) : null}
        </div>
        <Collapse open={open} id={stepsId}>
          {liveSteps.length > 0 ? (
            <ul className="m-0 list-none space-y-1.5 p-0">
              {liveSteps.map((step) => {
                const approval =
                  step.status === "awaiting-approval"
                    ? controls?.approvalFor?.(step.approvalId)
                    : undefined;
                return (
                  <Fragment key={step.id}>
                    {approval === undefined ? null : (
                      <li className="list-none text-[11.5px] font-medium text-warn">
                        {subagentApprovalTitle(label)}
                      </li>
                    )}
                    {controls?.renderStep(step, approval)}
                  </Fragment>
                );
              })}
            </ul>
          ) : (
            <ul className="m-0 list-none p-0">
              {(meta.calls ?? []).map((step, index) => (
                <CompactStep key={`${step.key}-${String(index)}`} step={step} />
              ))}
            </ul>
          )}
        </Collapse>
        {!running && meta.report !== undefined && meta.report !== "" ? (
          <div className="rounded-control bg-main-850 px-3 py-2.5">
            <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
              {SUBAGENT_REPORT}
            </p>
            <ChatMarkdown content={meta.report} />
          </div>
        ) : null}
        {call.error !== null && meta.status === "failed" ? (
          <p className="m-0 text-[12px] text-err">{call.error}</p>
        ) : null}
      </div>
    </li>
  );
});

export function SubagentGroup({
  calls,
  controls,
  live,
}: {
  readonly calls: readonly ChatToolCallDto[];
  readonly controls: SubagentControls | undefined;
  readonly live: boolean;
}) {
  const cards = calls.map((call) => (
    <SubagentCard key={call.id} call={call} controls={controls} live={live} />
  ));
  if (calls.length === 1) return <>{cards}</>;
  return (
    <li className="list-none">
      <p className="mb-1.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        {SUBAGENT_GROUP} · {subagentGroupTitle(calls.length)}
      </p>
      <ul
        aria-label={SUBAGENT_GROUP}
        className="m-0 list-none space-y-1.5 border-l-2 border-line p-0 pl-2.5"
      >
        {cards}
      </ul>
    </li>
  );
}

export default SubagentCard;
