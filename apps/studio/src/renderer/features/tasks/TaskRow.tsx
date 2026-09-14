import { mdiRefresh, mdiStopCircleOutline } from "@mdi/js";
import type { RunSummaryDto } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/IconButton";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  formatElapsed,
  KIND_ICONS,
  KIND_LABELS,
  progressPercent,
  runElapsed,
  runNote,
  runSubline,
  shortRunId,
  STATUS_LABELS,
  STATUS_TONES,
} from "./runPresentation";

export interface TaskRowProps {
  readonly run: RunSummaryDto;
  readonly now: number;
  readonly selected: boolean;
  readonly busy: boolean;
  readonly onSelect: () => void;
  readonly onStop: () => void;
  readonly onRetry: () => void;
  readonly onReview: () => void;
}

const BAR_TONE: Record<string, string> = {
  warn: "bg-warn",
  err: "bg-err",
  ok: "bg-ok",
  accent: "bg-accent-dark",
  idle: "bg-main-600",
};

export default function TaskRow({
  run,
  now,
  selected,
  busy,
  onSelect,
  onStop,
  onRetry,
  onReview,
}: TaskRowProps) {
  const tone = STATUS_TONES[run.status];
  const note = runNote(run);
  const percent = progressPercent(run.progress);
  const live = run.status === "running" || run.status === "blocked";
  const stoppable = live || run.status === "queued";
  return (
    <div
      className={`flex items-center gap-3 border-b border-main-750 px-4 py-2.75 ${
        selected ? "bg-main-750 shadow-[inset_2px_0_0_var(--color-accent-dark)]" : "hover:bg-main-800"
      }`}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-current={selected ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span
          className={`flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 ${STATUS_TONE_TEXT[tone]}`}
        >
          <Icon path={KIND_ICONS[run.kind]} size={16} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.75">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate text-[12.5px] font-semibold text-main-100">{run.title}</span>
            <Chip>{KIND_LABELS[run.kind]}</Chip>
            <Chip title={run.id}>
              <span className="font-mono">{shortRunId(run.id)}</span>
            </Chip>
            {run.retryOfId === undefined ? null : <Chip title={run.retryOfId}>повтор</Chip>}
          </span>
          <span className="flex min-w-0 items-center gap-2.25">
            <span className="flex-none font-mono text-[10.5px] text-main-400">
              {runSubline(run)}
            </span>
            {note ? (
              <span className={`truncate text-[10.5px] ${STATUS_TONE_TEXT[tone]}`} title={note}>
                {note}
              </span>
            ) : (
              <span className="flex-none font-mono text-[10.5px] text-main-500">
                {run.startedAt === undefined
                  ? "ожидает запуска"
                  : `начат в ${new Date(run.startedAt).toLocaleTimeString("ru-RU", { hour12: false })}`}
              </span>
            )}
          </span>
          {live ? (
            <span className="mt-0.75 block h-0.75 w-full rounded-sm bg-main-700">
              <span
                className={`block h-0.75 rounded-sm ${BAR_TONE[tone] ?? "bg-accent-dark"}`}
                style={{ width: `${String(percent)}%` }}
              />
            </span>
          ) : null}
        </span>
      </button>
      <span className="w-14 flex-none text-right font-mono text-[11px] text-main-300">
        {run.status === "queued" ? "" : formatElapsed(runElapsed(run, now))}
      </span>
      <Chip tone="neutral" title={STATUS_LABELS[run.status]}>
        <StatusDot tone={tone} />
        <span className={STATUS_TONE_TEXT[tone]}>{STATUS_LABELS[run.status]}</span>
      </Chip>
      {run.status === "blocked" && run.approval !== undefined ? (
        <Button tone="primary" disabled={busy} onClick={onReview}>
          Решить
        </Button>
      ) : null}
      {run.status === "failed" || run.status === "interrupted" ? (
        <Button tone="secondary" disabled={busy} onClick={onRetry}>
          <Icon path={mdiRefresh} size={14} />
          Повторить
        </Button>
      ) : null}
      {stoppable ? (
        <IconButton
          path={mdiStopCircleOutline}
          aria-label={`Остановить ${run.title}`}
          title="Остановить"
          disabled={busy}
          onClick={onStop}
        />
      ) : null}
    </div>
  );
}
