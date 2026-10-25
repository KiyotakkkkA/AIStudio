import { useId, useState, type CSSProperties } from "react";
import { AnimatePresence, motion } from "motion/react";
import type {
  ChatToolCallDto,
  FileDiffDto,
  Json,
  ProcessPlanDto,
  QuestionAnswerDto,
  QuestionRequestDto,
} from "@zvs/shared";
import { mdiCheck, mdiChevronDown, mdiClose, mdiMinus } from "@mdi/js";
import { toolCallSummary } from "./toolCallSummary";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import {
  STATUS_TONE_BACKGROUNDS,
  STATUS_TONE_TEXT,
  type StatusTone,
} from "../../ui/atoms/statusTone";
import Collapse from "../../ui/motion/Collapse";
import { useFx, useTransition } from "../../ui/motion/fx";
import ChatQuestionCard, { QuestionAnswers } from "./ChatQuestionCard";
import FileChangeList from "./FileChangeList";
import ProcessPlanView from "./ProcessPlanView";
import {
  REJECT_PROCESS,
  RUN_PROCESS,
  durationLabel,
  exitLabel,
  processLead,
  processResult,
} from "./processCopy";
import {
  APPROVE_CHANGES,
  CHANGES_LEAD,
  CHANGES_TITLE,
  FILE_TOOL_APPLY,
  REJECT_CHANGES,
  fileChanges,
} from "./fileChangesCopy";
import { ASK_TOOL, QUESTIONS_SKIPPED, QUESTIONS_UNANSWERED, questionRecord } from "./questionCopy";
import {
  ALLOW_ONCE,
  APPROVAL_PROMPT,
  APPROVAL_QUESTION,
  ARGUMENTS_TITLE,
  CATEGORY_BADGE,
  FORBID,
  TOOL_CALLS_LABEL,
  TOOL_KEY_LABEL,
  isBuiltinTool,
  toolPresentation,
} from "./toolCallCopy";
import { toolCallView } from "./toolCallView";
import { Eyebrow, RawArguments, ResultBlock, ToolCallViewBody } from "./ToolCallDetails";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";

export const TOOL_CALL_STATUS: Record<
  ChatToolCallDto["status"],
  { readonly label: string; readonly tone: StatusTone }
> = {
  running: { label: "выполняется", tone: "accent" },
  "awaiting-approval": { label: "ждёт подтверждения", tone: "warn" },
  "awaiting-answer": { label: "ждёт ответа", tone: "warn" },
  succeeded: { label: "готово", tone: "ok" },
  failed: { label: "ошибка", tone: "err" },
  refused: { label: "запрещён", tone: "idle" },
  denied: { label: "отклонён", tone: "warn" },
  cancelled: { label: "отменён", tone: "idle" },
};

const STATUS_ICON: Partial<Record<ChatToolCallDto["status"], string>> = {
  succeeded: mdiCheck,
  failed: mdiClose,
  refused: mdiMinus,
  denied: mdiMinus,
  cancelled: mdiMinus,
};

export function toolArgumentsText(value: Json): string {
  if (typeof value === "string") return value;
  return JSON.stringify(value, null, 2);
}

export interface ToolCallApproval {
  readonly id: string;
  readonly busy: boolean;
  readonly changes?: readonly FileDiffDto[];
  readonly process?: ProcessPlanDto;
}

export interface ToolCallQuestion {
  readonly request: QuestionRequestDto;
  readonly busy: boolean;
}

function StatusMark({ status }: { readonly status: ChatToolCallDto["status"] }) {
  const transition = useTransition("pop");
  const tone = TOOL_CALL_STATUS[status].tone;
  const icon = STATUS_ICON[status];
  const waiting = status === "awaiting-approval" || status === "awaiting-answer";
  return (
    <span aria-hidden="true" className="relative flex size-3.5 items-center justify-center">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={icon === undefined ? `dot-${tone}` : status}
          initial={{ scale: 0.3, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          exit={{ scale: 0.3, opacity: 0 }}
          transition={transition}
          className={`flex items-center justify-center ${STATUS_TONE_TEXT[tone]}`}
        >
          {icon === undefined ? (
            <span
              className={`block size-1.5 rounded-full ${STATUS_TONE_BACKGROUNDS[tone]} ${
                status === "running" ? "anim-on:animate-pulse" : ""
              } ${waiting ? "anim-full:animate-zvs-halo" : ""}`}
            />
          ) : (
            <Icon path={icon} size={13} />
          )}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

function RunningSweep() {
  return (
    <span
      aria-hidden="true"
      className="pointer-events-none absolute inset-x-0 bottom-0 h-px overflow-hidden"
    >
      <span className="block h-full w-2/5 animate-zvs-sweep bg-linear-to-r from-transparent via-accent-dark to-transparent" />
    </span>
  );
}

function ToolCallCard({
  call,
  approval,
  asking,
  live,
  onDecide,
  onAnswer,
  onSkip,
}: {
  readonly call: ChatToolCallDto;
  readonly approval: ToolCallApproval | undefined;
  readonly asking: ToolCallQuestion | undefined;
  readonly live: boolean;
  readonly onDecide?: ((id: string, approve: boolean) => void) | undefined;
  readonly onAnswer?: ((id: string, answers: QuestionAnswerDto[]) => void) | undefined;
  readonly onSkip?: ((id: string) => void) | undefined;
}) {
  const bodyId = useId();
  const blocking = approval !== undefined || asking !== undefined;
  const [manual, setManual] = useState<boolean | null>(null);
  const [wasBlocking, setWasBlocking] = useState(blocking);
  if (wasBlocking !== blocking) {
    setWasBlocking(blocking);
    setManual(null);
  }
  const open = manual ?? blocking;
  const { motion: level } = useFx();

  const status = TOOL_CALL_STATUS[call.status];
  const presentation = toolPresentation(call.key);
  const builtin = isBuiltinTool(call.key);
  const proposed = approval?.changes ?? [];
  const applied = call.key === FILE_TOOL_APPLY ? fileChanges(call.meta) : [];
  const proposedProcess = approval?.process;
  const finished = proposedProcess === undefined ? processResult(call.meta) : null;
  const record = call.key === ASK_TOOL ? questionRecord(call.meta) : null;
  const view =
    record === null && finished === null && proposedProcess === undefined
      ? toolCallView(call)
      : null;
  const searched = view?.kind === "search" && view.citations !== null;

  return (
    <motion.li
      layout={level === "full" ? "position" : false}
      initial={!live || level === "none" ? false : { opacity: 0, y: level === "full" ? 6 : 0 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.22 }}
      style={{ "--zvs-beam-color": "var(--color-warn)" } as CSSProperties}
      className={`relative overflow-hidden rounded-card bg-main-800 ${
        blocking ? "zvs-beam shadow-[inset_0_0_0_1px_var(--color-warn-line)]" : ""
      }`}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setManual(!open)}
        className="group/call flex min-h-9 w-full cursor-pointer items-center gap-2.5 px-3 py-1.5 text-left text-[12px] hover:bg-main-750/40"
      >
        <span
          aria-hidden="true"
          className={`flex size-6 shrink-0 items-center justify-center rounded-pill ${CATEGORY_BADGE[presentation.category]}`}
        >
          <Icon path={presentation.icon} size={13} />
        </span>
        <span
          title={builtin ? call.key : undefined}
          className={`shrink-0 text-[12.5px] font-medium text-main-100 ${builtin ? "" : "font-mono text-[12px]"}`}
        >
          {presentation.title}
        </span>
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-main-500">
          {toolCallSummary(call)}
        </span>
        <span
          className={`inline-flex shrink-0 items-center gap-1.5 text-[11px] font-medium ${STATUS_TONE_TEXT[status.tone]} ${
            call.status === "running" ? "zvs-shimmer-text" : ""
          }`}
        >
          <StatusMark status={call.status} />
          {status.label}
        </span>
        {call.durationMs > 0 && (
          <span className="shrink-0 font-mono text-[11px] text-main-500 tabular-nums">
            {durationLabel(call.durationMs)}
          </span>
        )}
        <Icon
          path={mdiChevronDown}
          size={15}
          className={`shrink-0 text-main-500 group-hover/call:text-main-200 anim-on:transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {call.status === "running" ? <RunningSweep /> : null}
      <Collapse open={open} id={bodyId}>
        <div className="space-y-2.5 px-3 pt-1 pb-3 text-[12px]">
          {asking && (
            <ChatQuestionCard
              request={asking.request}
              busy={asking.busy}
              onAnswer={(answers) => onAnswer?.(asking.request.id, answers)}
              onSkip={() => onSkip?.(asking.request.id)}
            />
          )}
          {record && !asking && !(live && call.status === "awaiting-answer") && (
            <QuestionAnswers
              record={record}
              note={
                record.skipped
                  ? QUESTIONS_SKIPPED
                  : call.status === "awaiting-answer" || call.status === "cancelled"
                    ? QUESTIONS_UNANSWERED
                    : null
              }
            />
          )}
          {view !== null && <ToolCallViewBody view={view} />}
          {view !== null && <RawArguments text={toolArgumentsText(call.arguments)} />}
          {record === null &&
            view === null &&
            finished === null &&
            proposedProcess === undefined && (
              <div>
                <Eyebrow>{ARGUMENTS_TITLE}</Eyebrow>
                <ScrollArea className="max-h-48 rounded-control bg-main-900 px-2.5 py-2">
                  <pre className="m-0 font-mono text-[11.5px] wrap-break-word whitespace-pre-wrap text-main-300">
                    {toolArgumentsText(call.arguments)}
                  </pre>
                </ScrollArea>
              </div>
            )}
          {finished && (
            <div className="space-y-2">
              <p className="m-0 font-mono text-[11.5px] text-main-300 tabular-nums">
                {exitLabel(finished)} · {durationLabel(finished.durationMs)}
              </p>
              <ProcessPlanView plan={finished.process} />
            </div>
          )}
          {applied.length > 0 && (
            <div>
              <Eyebrow>{CHANGES_TITLE}</Eyebrow>
              <FileChangeList changes={applied} />
            </div>
          )}
          {call.result !== null && record === null && !searched && (
            <ResultBlock result={call.result} truncated={call.truncated} />
          )}
          {call.error !== null && <p className="text-[12px] text-err">{call.error}</p>}
          {approval && proposed.length > 0 && (
            <div role="status" className="space-y-2 rounded-control bg-warn-soft px-3 py-2.5">
              <p className="m-0 text-[12.5px] text-main-100">{CHANGES_LEAD}</p>
              <FileChangeList changes={proposed} expanded />
              <div className="flex gap-2">
                <Button
                  size="sm"
                  disabled={approval.busy}
                  tone="primary"
                  onClick={() => onDecide?.(approval.id, true)}
                >
                  {APPROVE_CHANGES}
                </Button>
                <Button
                  size="sm"
                  disabled={approval.busy}
                  onClick={() => onDecide?.(approval.id, false)}
                >
                  {REJECT_CHANGES}
                </Button>
              </div>
            </div>
          )}
          {approval && proposedProcess && (
            <div role="status" className="space-y-2 rounded-control bg-warn-soft px-3 py-2.5">
              <p className="m-0 text-[12.5px] text-main-100">{processLead(proposedProcess)}</p>
              <ProcessPlanView plan={proposedProcess} expanded />
              <div className="flex gap-2">
                <Button
                  size="sm"
                  disabled={approval.busy}
                  tone="primary"
                  onClick={() => onDecide?.(approval.id, true)}
                >
                  {RUN_PROCESS}
                </Button>
                <Button
                  size="sm"
                  disabled={approval.busy}
                  onClick={() => onDecide?.(approval.id, false)}
                >
                  {REJECT_PROCESS}
                </Button>
              </div>
            </div>
          )}
          {approval && proposed.length === 0 && !proposedProcess && (
            <div
              role="status"
              className="flex flex-wrap items-center gap-3 rounded-control bg-warn-soft px-3 py-2.5"
            >
              <p className="min-w-0 flex-1 text-[12.5px] text-main-100">
                {APPROVAL_PROMPT}{" "}
                {builtin ? (
                  `«${presentation.title}»`
                ) : (
                  <span className="font-mono">{call.key}</span>
                )}
                . {APPROVAL_QUESTION}
              </p>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  disabled={approval.busy}
                  tone="primary"
                  onClick={() => onDecide?.(approval.id, true)}
                >
                  {ALLOW_ONCE}
                </Button>
                <Button
                  size="sm"
                  disabled={approval.busy}
                  onClick={() => onDecide?.(approval.id, false)}
                >
                  {FORBID}
                </Button>
              </div>
            </div>
          )}
          {builtin && (
            <p className="m-0 text-[11px] text-main-500">
              {TOOL_KEY_LABEL} <span className="font-mono">{call.key}</span>
            </p>
          )}
        </div>
      </Collapse>
    </motion.li>
  );
}

export default function ChatToolCalls({
  calls,
  approvalFor,
  onDecide,
  questionFor,
  onAnswer,
  onSkip,
}: {
  readonly calls: readonly ChatToolCallDto[];
  readonly approvalFor?: (id: string | null) => ToolCallApproval | undefined;
  readonly onDecide?: (id: string, approve: boolean) => void;
  readonly questionFor?: (id: string) => ToolCallQuestion | undefined;
  readonly onAnswer?: (id: string, answers: QuestionAnswerDto[]) => void;
  readonly onSkip?: (id: string) => void;
}) {
  if (calls.length === 0) return null;
  const live = questionFor !== undefined;
  return (
    <ul aria-label={TOOL_CALLS_LABEL} className="m-0 mb-5 list-none space-y-1.5 p-0">
      {calls.map((call) => {
        const approval =
          call.status === "awaiting-approval" ? approvalFor?.(call.approvalId) : undefined;
        const record = call.key === ASK_TOOL ? questionRecord(call.meta) : null;
        const asking =
          record !== null && call.status === "awaiting-answer"
            ? questionFor?.(record.request.id)
            : undefined;
        return (
          <ToolCallCard
            key={call.id}
            call={call}
            approval={approval}
            asking={asking}
            live={live}
            onDecide={onDecide}
            onAnswer={onAnswer}
            onSkip={onSkip}
          />
        );
      })}
    </ul>
  );
}
