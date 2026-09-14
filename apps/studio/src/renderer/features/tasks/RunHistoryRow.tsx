import type { RunSummaryDto } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  formatDateTime,
  formatElapsed,
  KIND_ICONS,
  KIND_LABELS,
  runElapsed,
  runNote,
  shortRunId,
  STATUS_LABELS,
  STATUS_TONES,
} from "./runPresentation";

export interface RunHistoryRowProps {
  readonly run: RunSummaryDto;
  readonly selected: boolean;
  readonly onSelect: () => void;
}

export default function RunHistoryRow({ run, selected, onSelect }: RunHistoryRowProps) {
  const tone = STATUS_TONES[run.status];
  const note = runNote(run);
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={selected ? "true" : undefined}
      className={`flex w-full items-center gap-3 border-b border-main-750 px-4 py-2.5 text-left ${
        selected ? "bg-main-750" : "hover:bg-main-800"
      }`}
    >
      <span
        className={`flex size-7 flex-none items-center justify-center rounded-lg bg-main-700 ${STATUS_TONE_TEXT[tone]}`}
      >
        <Icon path={KIND_ICONS[run.kind]} size={15} />
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-[12.5px] font-medium text-main-100">{run.title}</span>
          <Chip>{KIND_LABELS[run.kind]}</Chip>
          <Chip title={run.id}>
            <span className="font-mono">{shortRunId(run.id)}</span>
          </Chip>
        </span>
        <span className="flex min-w-0 items-center gap-2">
          <span className="flex-none font-mono text-[10.5px] text-main-500">
            {formatDateTime(run.createdAt)}
          </span>
          {note ? (
            <span className={`truncate text-[10.5px] ${STATUS_TONE_TEXT[tone]}`} title={note}>
              {note}
            </span>
          ) : null}
        </span>
      </span>
      <span className="w-14 flex-none text-right font-mono text-[11px] text-main-300">
        {run.finishedAt === undefined ? "" : formatElapsed(runElapsed(run, run.finishedAt))}
      </span>
      <Chip>
        <StatusDot tone={tone} />
        <span className={STATUS_TONE_TEXT[tone]}>{STATUS_LABELS[run.status]}</span>
      </Chip>
    </button>
  );
}
