import { mdiChevronDown, mdiChevronRight } from "@mdi/js";
import type { StepDto } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import JsonBlock from "./JsonBlock";
import { knowledgeSearchStep } from "./knowledgeStep";
import { formatDuration, formatTime, STEP_TONES } from "./runPresentation";

export interface RunStepDetailProps {
  readonly step: StepDto;
  readonly expanded: boolean;
  readonly pruned: boolean;
  readonly onToggle: () => void;
}

export default function RunStepDetail({ step, expanded, pruned, onToggle }: RunStepDetailProps) {
  const tone = STEP_TONES[step.status] ?? "idle";
  const search = knowledgeSearchStep(step);
  const duration = formatDuration(Math.max(0, (step.finishedAt ?? step.startedAt) - step.startedAt));
  return (
    <li className="list-none rounded-card bg-main-900 shadow-card">
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
        <span className="min-w-0 flex-1 truncate text-[12px] text-main-100">
          {step.nodeId}
          {search === null ? null : (
            <span className="text-main-400">
              {" "}
              · «{search.query}» → {search.passages.length}
            </span>
          )}
        </span>
        <Chip>
          <span className="font-mono">{step.type}</span>
        </Chip>
        {step.attempt > 1 ? <Chip>попытка {step.attempt}</Chip> : null}
        <span className="flex-none font-mono text-[11px] text-main-500">
          {formatTime(step.startedAt)} · {duration}
        </span>
      </button>
      {expanded ? (
        <div className="flex flex-col gap-2 border-t border-line px-2.75 py-2.5">
          {step.error === undefined ? null : (
            <p className={`m-0 text-[12px] ${STATUS_TONE_TEXT.err}`}>{step.error}</p>
          )}
          {pruned ? (
            <p className="m-0 text-[11px] text-main-500">
              Данные шага удалены политикой хранения: остались только статус и тайминги.
            </p>
          ) : (
            <>
              {search === null ? null : (
                <>
                  <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                    Запрос модели · найдено {search.passages.length}
                  </p>
                  <p className="m-0 text-[12px] text-main-100">«{search.query}»</p>
                  {search.passages.length === 0 ? (
                    <p className="m-0 text-[12px] text-main-500">
                      Ничего не прошло порог релевантности.
                    </p>
                  ) : (
                    <ol className="m-0 flex list-none flex-col gap-1.5 p-0">
                      {search.passages.map((passage, index) => (
                        <li
                          key={`${passage.path}:${String(index)}`}
                          className="rounded-control bg-main-850 px-2.5 py-2"
                        >
                          <div className="flex items-center gap-2 text-[12px]">
                            <span className="min-w-0 flex-1 truncate font-medium text-main-100">
                              {passage.path}
                              {passage.section === "" ? "" : ` · ${passage.section}`}
                            </span>
                            <span className="flex-none font-mono text-[11px] text-main-500 tabular-nums">
                              {(passage.score * 100).toFixed(0)}%
                            </span>
                          </div>
                          <p className="m-0 mt-1 line-clamp-2 text-[11.5px] text-main-400">
                            {passage.text}
                          </p>
                        </li>
                      ))}
                    </ol>
                  )}
                </>
              )}
              <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                Вход
              </p>
              <JsonBlock value={step.input} empty="без входных данных" />
              <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
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
