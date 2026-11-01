import { Fragment, useId, useState, type ReactNode } from "react";
import { observer } from "mobx-react-lite";
import {
  mdiChevronDown,
  mdiRobotOutline,
  mdiShieldAlertOutline,
  mdiStopCircleOutline,
} from "@mdi/js";
import type { ChatToolCallDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import Collapse from "../../ui/motion/Collapse";
import ChatMarkdown from "./ChatMarkdown";
import { TOOL_CALLS_LABEL, toolPresentation } from "./toolCallCopy";
import { PipelineItem, PipelineList } from "./Pipeline";
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
      <Icon path={presentation.icon} size={13} className="shrink-0 text-main-500" />
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
  const bodyId = useId();
  const [showSteps, setShowSteps] = useState<boolean | null>(null);
  const [manual, setManual] = useState<boolean | null>(null);
  const meta = subagentMeta(call);
  const running = meta !== null && (meta.status === "running" || meta.status === "queued");
  const liveSteps = controls?.stepsFor(meta?.runId) ?? [];
  const pending = liveSteps.filter((step) => step.status === "awaiting-approval");
  const active = running || pending.length > 0;
  const [wasActive, setWasActive] = useState(active);
  if (wasActive !== active) {
    setWasActive(active);
    setManual(null);
  }
  const open = manual ?? active;
  const stepsOpen = showSteps ?? active;
  if (meta === null) return null;
  const stepCount = running ? liveSteps.length : meta.toolCalls;
  const label = meta.label || firstLine(meta.task);
  const tone = STATUS_TONES[meta.status];
  const status = statusLabel(meta);
  const stopping = meta.runId !== undefined && controls?.stopping.includes(meta.runId) === true;
  return (
    <PipelineItem icon={mdiRobotOutline} tail={open}>
      <div className="flex min-h-8 items-center gap-2.5 py-1 text-[12px]">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={() => setManual(!open)}
          className="group/agent flex min-w-0 flex-1 cursor-pointer flex-wrap items-center gap-2.5 text-left"
        >
          <span className="shrink-0 text-[12.5px] font-medium text-main-100 transition-colors group-hover/agent:text-main-50">
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
          <Icon
            path={mdiChevronDown}
            size={15}
            className={`shrink-0 text-main-500 group-hover/agent:text-main-200 anim-on:transition-transform ${
              open ? "rotate-180" : ""
            }`}
          />
        </button>
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
      <Collapse open={open} id={bodyId}>
        <div className="space-y-2 pt-1 pb-2 text-[12px]">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] text-main-500 tabular-nums">
            <button
              type="button"
              aria-expanded={stepsOpen}
              aria-controls={stepsId}
              disabled={stepCount === 0}
              onClick={() => setShowSteps(!stepsOpen)}
              className="inline-flex cursor-pointer items-center gap-1 text-main-400 hover:text-main-100 disabled:cursor-default disabled:hover:text-main-400"
            >
              {subagentTools(stepCount)}
              {stepCount > 0 ? (
                <>
                  <span className="sr-only">
                    {stepsOpen ? SUBAGENT_STEPS_HIDE : SUBAGENT_STEPS_SHOW}
                  </span>
                  <Icon
                    path={mdiChevronDown}
                    size={13}
                    className={`anim-on:transition-transform ${stepsOpen ? "rotate-180" : ""}`}
                  />
                </>
              ) : null}
            </button>
            {!running && meta.tokens > 0 ? <span>{subagentTokens(meta.tokens)}</span> : null}
            {!running && meta.durationMs !== undefined ? (
              <span>{subagentDuration(meta.durationMs)}</span>
            ) : null}
          </div>
          <Collapse open={stepsOpen} id={stepsId}>
            {liveSteps.length > 0 ? (
              <PipelineList label={TOOL_CALLS_LABEL}>
                {liveSteps.map((step) => {
                  const approval =
                    step.status === "awaiting-approval"
                      ? controls?.approvalFor?.(step.approvalId)
                      : undefined;
                  return (
                    <Fragment key={step.id}>
                      {approval === undefined ? null : (
                        <PipelineItem icon={mdiShieldAlertOutline}>
                          <p className="m-0 flex min-h-8 items-center py-1 text-[11.5px] font-medium text-warn">
                            {subagentApprovalTitle(label)}
                          </p>
                        </PipelineItem>
                      )}
                      {controls?.renderStep(step, approval)}
                    </Fragment>
                  );
                })}
              </PipelineList>
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
      </Collapse>
    </PipelineItem>
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
  const groupId = useId();
  const [manual, setManual] = useState<boolean | null>(null);
  const active = calls.some((call) => {
    const meta = subagentMeta(call);
    return meta !== null && (meta.status === "running" || meta.status === "queued");
  });
  const open = manual ?? active;
  const cards = calls.map((call) => (
    <SubagentCard key={call.id} call={call} controls={controls} live={live} />
  ));
  if (calls.length === 1) return <>{cards}</>;
  return (
    <PipelineItem icon={mdiRobotOutline} tail={open}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={groupId}
        onClick={() => setManual(!open)}
        className="group/sg flex min-h-8 w-full cursor-pointer items-center gap-2.5 py-1 text-left"
      >
        <span className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase transition-colors group-hover/sg:text-main-300">
          {SUBAGENT_GROUP} · {subagentGroupTitle(calls.length)}
        </span>
        <Icon
          path={mdiChevronDown}
          size={15}
          className={`shrink-0 text-main-500 group-hover/sg:text-main-200 anim-on:transition-transform ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>
      <Collapse open={open} id={groupId}>
        <PipelineList label={SUBAGENT_GROUP}>{cards}</PipelineList>
      </Collapse>
    </PipelineItem>
  );
}

export default SubagentCard;
