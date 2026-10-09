import type { ChatToolCallDto, Json } from "@zvs/shared";
import { mdiWrenchOutline } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import {
  STATUS_TONE_BACKGROUNDS,
  STATUS_TONE_TEXT,
  type StatusTone,
} from "../../ui/atoms/statusTone";

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

export function toolArgumentsSummary(value: Json): string {
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 80 ? `${text.slice(0, 80)}…` : text;
}

export interface ToolCallApproval {
  readonly id: string;
  readonly busy: boolean;
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
        return (
          <li
            key={call.id}
            className={`overflow-hidden rounded-card border bg-main-850 ${
              approval ? "border-warn-line" : "border-main-750"
            }`}
          >
            <details open={approval !== undefined} className="group/call">
              <summary className="flex min-h-9 cursor-pointer list-none items-center gap-2.5 px-3 py-1.5 text-[12px] [&::-webkit-details-marker]:hidden">
                <span
                  aria-hidden="true"
                  className="flex size-6 shrink-0 items-center justify-center rounded-pill bg-accent-soft text-accent-medium"
                >
                  <Icon path={mdiWrenchOutline} size={13} />
                </span>
                <span className="shrink-0 font-mono text-[12px] font-medium text-main-100">
                  {call.key}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-main-500">
                  {toolArgumentsSummary(call.arguments)}
                </span>
                <span
                  className={`inline-flex shrink-0 items-center gap-1.5 text-[11px] font-medium ${STATUS_TONE_TEXT[status.tone]}`}
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
                    {call.durationMs < 1000
                      ? `${String(call.durationMs)} мс`
                      : `${(call.durationMs / 1000).toFixed(1)} с`}
                  </span>
                )}
              </summary>
              <div className="space-y-2.5 border-t border-main-750 px-3 pt-2.5 pb-3 text-[12px]">
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
                {approval && (
                  <div
                    role="status"
                    className="flex flex-wrap items-center gap-3 rounded-control border border-warn-line bg-warn-soft px-3 py-2.5"
                  >
                    <p className="min-w-0 flex-1 text-[12.5px] text-main-100">
                      Модель хочет вызвать <span className="font-mono">{call.key}</span>. Разрешить
                      этот вызов?
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
