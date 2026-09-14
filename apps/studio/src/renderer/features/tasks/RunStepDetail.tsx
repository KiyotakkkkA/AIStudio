import { mdiChevronDown, mdiChevronRight } from "@mdi/js";
import type { StepDto } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import JsonBlock from "./JsonBlock";
import { formatDuration, formatTime, STEP_TONES } from "./runPresentation";

export interface RunStepDetailProps {
  readonly step: StepDto;
  readonly expanded: boolean;
  readonly pruned: boolean;
  readonly onToggle: () => void;
}

export default function RunStepDetail({ step, expanded, pruned, onToggle }: RunStepDetailProps) {
  const tone = STEP_TONES[step.status] ?? "idle";
  const duration = formatDuration(Math.max(0, (step.finishedAt ?? step.startedAt) - step.startedAt));
  return (
    <li className="list-none rounded-card border border-main-750 bg-main-800">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        className="flex w-full items-center gap-2.5 px-2.75 py-2 text-left"
      >
        <Icon
          path={expanded ? mdiChevronDown : mdiChevronRight}
          size={15}
          className="flex-none text-main-500"
        />
        <StatusDot tone={tone} />
        <span className="min-w-0 flex-1 truncate text-[12px] text-main-100">{step.nodeId}</span>
        <Chip>
          <span className="font-mono">{step.type}</span>
        </Chip>
        {step.attempt > 1 ? <Chip>попытка {step.attempt}</Chip> : null}
        <span className="flex-none font-mono text-[10.5px] text-main-500">
          {formatTime(step.startedAt)} · {duration}
        </span>
      </button>
      {expanded ? (
        <div className="flex flex-col gap-2 border-t border-main-750 px-2.75 py-2.5">
          {step.error === undefined ? null : (
            <p className={`m-0 text-[11.5px] ${STATUS_TONE_TEXT.err}`}>{step.error}</p>
          )}
          {pruned ? (
            <p className="m-0 text-[11px] text-main-500">
              Данные шага удалены политикой хранения: остались только статус и тайминги.
            </p>
          ) : (
            <>
              <p className="m-0 text-[10.5px] font-medium tracking-[0.08em] text-main-400 uppercase">
                Вход
              </p>
              <JsonBlock value={step.input} empty="без входных данных" />
              <p className="m-0 text-[10.5px] font-medium tracking-[0.08em] text-main-400 uppercase">
                Выход
              </p>
              <JsonBlock value={step.output} empty="без результата" />
            </>
          )}
        </div>
      ) : null}
    </li>
  );
}
