import type { ChatToolCallDto, FileDiffDto, Json, ProcessPlanDto } from "@zvs/shared";
import { toolCallSummary } from "./toolCallSummary";
import {
  mdiBookOpenPageVariantOutline,
  mdiCodeBraces,
  mdiConsoleLine,
  mdiDatabaseSearchOutline,
  mdiFileDocumentEditOutline,
  mdiFileDocumentOutline,
  mdiWrenchOutline,
} from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import {
  STATUS_TONE_BACKGROUNDS,
  STATUS_TONE_TEXT,
  type StatusTone,
} from "../../ui/atoms/statusTone";
import FileChangeList from "./FileChangeList";
import ProcessPlanView from "./ProcessPlanView";
import {
  PROCESS_TOOL_CODE,
  PROCESS_TOOL_SHELL,
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

export const TOOL_CALL_STATUS: Record<
  ChatToolCallDto["status"],
  { readonly label: string; readonly tone: StatusTone }
> = {
  running: { label: "выполняется", tone: "accent" },
  "awaiting-approval": { label: "ждёт подтверждения", tone: "warn" },
  succeeded: { label: "готово", tone: "ok" },
  failed: { label: "ошибка", tone: "err" },
  refused: { label: "запрещён", tone: "idle" },
  denied: { label: "отклонён", tone: "warn" },
  cancelled: { label: "отменён", tone: "idle" },
};

export function toolArgumentsText(value: Json): string {
  if (typeof value === "string") return value;
  return JSON.stringify(value, null, 2);
}

function toolIcon(key: string): string {
  if (key === "knowledge.search") return mdiDatabaseSearchOutline;
  if (key === "knowledge.read") return mdiBookOpenPageVariantOutline;
  if (key === FILE_TOOL_APPLY) return mdiFileDocumentEditOutline;
  if (key === PROCESS_TOOL_CODE) return mdiCodeBraces;
  if (key === PROCESS_TOOL_SHELL) return mdiConsoleLine;
  if (key.startsWith("fs.")) return mdiFileDocumentOutline;
  return mdiWrenchOutline;
}

export interface ToolCallApproval {
  readonly id: string;
  readonly busy: boolean;
  readonly changes?: readonly FileDiffDto[];
  readonly process?: ProcessPlanDto;
}

export default function ChatToolCalls({
  calls,
  approvalFor,
  onDecide,
}: {
  readonly calls: readonly ChatToolCallDto[];
  readonly approvalFor?: (id: string | null) => ToolCallApproval | undefined;
  readonly onDecide?: (id: string, approve: boolean) => void;
}) {
  if (calls.length === 0) return null;
  return (
    <ul aria-label="Вызовы инструментов" className="space-y-1.5">
      {calls.map((call) => {
        const status = TOOL_CALL_STATUS[call.status];
        const approval =
          call.status === "awaiting-approval" ? approvalFor?.(call.approvalId) : undefined;
        const proposed = approval?.changes ?? [];
        const applied = call.key === FILE_TOOL_APPLY ? fileChanges(call.meta) : [];
        const proposedProcess = approval?.process;
        const finished = proposedProcess === undefined ? processResult(call.meta) : null;
        return (
          <li
            key={call.id}
            className={`overflow-hidden rounded-card bg-main-800 ${
              approval ? "shadow-[inset_0_0_0_1px_var(--color-warn-line)]" : ""
            }`}
          >
            <details open={approval !== undefined} className="group/call">
              <summary className="flex min-h-9 cursor-pointer list-none items-center gap-2.5 px-3 py-1.5 text-[12px] [&::-webkit-details-marker]:hidden">
                <span
                  aria-hidden="true"
                  className="flex size-6 shrink-0 items-center justify-center rounded-pill bg-accent-soft text-accent-medium"
                >
                  <Icon path={toolIcon(call.key)} size={13} />
                </span>
                <span className="shrink-0 font-mono text-[12px] font-medium text-main-100">
                  {call.key}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-main-500">
                  {toolCallSummary(call)}
                </span>
                <span
                  className={`inline-flex shrink-0 items-center gap-1.5 text-[11px] font-medium ${STATUS_TONE_TEXT[status.tone]} ${
                    call.status === "running" ? "zvs-shimmer-text" : ""
                  }`}
                >
                  <span
                    aria-hidden="true"
                    className={`size-1.5 rounded-full ${STATUS_TONE_BACKGROUNDS[status.tone]} ${
                      call.status === "running" ? "animate-pulse" : ""
                    }`}
                  />
                  {status.label}
                </span>
                {call.durationMs > 0 && (
                  <span className="shrink-0 font-mono text-[11px] text-main-500 tabular-nums">
                    {durationLabel(call.durationMs)}
                  </span>
                )}
              </summary>
              <div className="space-y-2.5 px-3 pt-1 pb-3 text-[12px]">
                <div>
                  <p className="mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                    Аргументы
                  </p>
                  <ScrollArea className="max-h-48 rounded-control bg-main-900 px-2.5 py-2">
                    <pre className="m-0 font-mono text-[11.5px] wrap-break-word whitespace-pre-wrap text-main-300">
                      {toolArgumentsText(call.arguments)}
                    </pre>
                  </ScrollArea>
                </div>
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
                    <p className="mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                      {CHANGES_TITLE}
                    </p>
                    <FileChangeList changes={applied} />
                  </div>
                )}
                {call.result !== null && (
                  <div>
                    <p className="mb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                      Результат{call.truncated ? " · сокращён" : ""}
                    </p>
                    <ScrollArea className="max-h-60 rounded-control bg-main-900 px-2.5 py-2">
                      <pre className="m-0 font-mono text-[11.5px] wrap-break-word whitespace-pre-wrap text-main-300">
                        {call.result}
                      </pre>
                    </ScrollArea>
                  </div>
                )}
                {call.error !== null && <p className="text-[12px] text-err">{call.error}</p>}
                {approval && proposed.length > 0 && (
                  <div role="status" className="space-y-2 rounded-control bg-warn-soft px-3 py-2.5">
                    <p className="m-0 text-[12.5px] text-main-100">{CHANGES_LEAD}</p>
                    <FileChangeList changes={proposed} expanded />
                    <div className="flex gap-2">
                      <Button
                        disabled={approval.busy}
                        tone="primary"
                        onClick={() => onDecide?.(approval.id, true)}
                      >
                        {APPROVE_CHANGES}
                      </Button>
                      <Button
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
                    <p className="m-0 text-[12.5px] text-main-100">
                      {processLead(proposedProcess)}
                    </p>
                    <ProcessPlanView plan={proposedProcess} expanded />
                    <div className="flex gap-2">
                      <Button
                        disabled={approval.busy}
                        tone="primary"
                        onClick={() => onDecide?.(approval.id, true)}
                      >
                        {RUN_PROCESS}
                      </Button>
                      <Button
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
                      Модель запрашивает вызов <span className="font-mono">{call.key}</span>.
                      Разрешить?
                    </p>
                    <div className="flex gap-2">
                      <Button
                        disabled={approval.busy}
                        tone="primary"
                        onClick={() => onDecide?.(approval.id, true)}
                      >
                        Разрешить один раз
                      </Button>
                      <Button
                        disabled={approval.busy}
                        onClick={() => onDecide?.(approval.id, false)}
                      >
                        Запретить
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            </details>
          </li>
        );
      })}
    </ul>
  );
}
