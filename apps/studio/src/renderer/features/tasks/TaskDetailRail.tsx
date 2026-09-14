import { observer } from "mobx-react-lite";
import { mdiHistory } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useNavigate } from "react-router-dom";
import { ROUTES } from "../../app/routes";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import EmptyState from "../../ui/molecules/EmptyState";
import JsonBlock from "./JsonBlock";
import RunStepTree from "./RunStepTree";
import {
  formatDateTime,
  formatElapsed,
  KIND_LABELS,
  runElapsed,
  shortRunId,
  STATUS_LABELS,
} from "./runPresentation";
import type TaskStore from "./TaskStore";

export default observer(function TaskDetailRail({ store }: { readonly store: TaskStore }) {
  const navigate = useNavigate();
  const run = store.selected;
  return (
    <aside
      aria-label="Детали запуска"
      className="flex w-80 flex-none flex-col border-l border-main-750 bg-main-900"
    >
      {run === null ? (
        <div className="flex flex-1 items-center justify-center p-4">
          <EmptyState
            icon={mdiHistory}
            title="Запуск не выбран"
            description="Выберите строку, чтобы увидеть дерево шагов, тайминги и входные данные."
          />
        </div>
      ) : (
        <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 p-3.5">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <h2 className="m-0 min-w-0 flex-1 truncate text-[13.5px] font-semibold text-main-50">
                {run.title}
              </h2>
              <Chip title={run.id}>
                <span className="font-mono">{shortRunId(run.id)}</span>
              </Chip>
            </div>
            <p className="m-0 font-mono text-[10.5px] text-main-500">
              {KIND_LABELS[run.kind]} · {formatDateTime(run.startedAt ?? run.createdAt)}
            </p>
          </div>

          <dl className="m-0 flex flex-col gap-1.5 rounded-card border border-main-750 bg-main-800 p-2.75 text-[11.5px]">
            <div className="flex justify-between gap-2">
              <dt className="text-main-400">Статус</dt>
              <dd className="m-0 font-mono">{STATUS_LABELS[run.status]}</dd>
            </div>
            <div className="flex justify-between gap-2">
              <dt className="text-main-400">Прошло</dt>
              <dd className="m-0 font-mono">{formatElapsed(runElapsed(run, store.now))}</dd>
            </div>
            <div className="flex justify-between gap-2">
              <dt className="text-main-400">Шаги</dt>
              <dd className="m-0 font-mono">
                {run.progress.done} из {run.progress.total}
              </dd>
            </div>
            {run.retryOfId === undefined ? null : (
              <div className="flex justify-between gap-2">
                <dt className="text-main-400">Повтор запуска</dt>
                <dd className="m-0 font-mono" title={run.retryOfId}>
                  {shortRunId(run.retryOfId)}
                </dd>
              </div>
            )}
          </dl>

          {run.approval === undefined ? null : (
            <div className="flex flex-col gap-2 rounded-card border border-warn/40 bg-main-800 p-2.75">
              <p className="m-0 text-[11.5px] text-warn">
                Требуется решение: {run.approval.subject}
              </p>
              <p className="m-0 font-mono text-[10.5px] text-main-500">
                область {run.approval.scope} · до {formatDateTime(run.approval.expiresAt)}
              </p>
              <div className="flex gap-2">
                <Button
                  tone="primary"
                  disabled={store.busyId !== null}
                  onClick={() => {
                    if (run.approval) void store.decide(run.id, run.approval.id, true);
                  }}
                >
                  Разрешить
                </Button>
                <Button
                  tone="ghost"
                  disabled={store.busyId !== null}
                  onClick={() => {
                    if (run.approval) void store.decide(run.id, run.approval.id, false);
                  }}
                >
                  Отказать
                </Button>
              </div>
            </div>
          )}

          <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
            Шаги
          </p>
          <div className="rounded-card border border-main-750 bg-main-800 p-2.75">
            <RunStepTree graph={store.graph} steps={store.steps} now={store.now} />
          </div>

          <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
            Входные данные
          </p>
          <JsonBlock value={store.payload} empty="запуск стартовал без входных данных" />

          <div className="flex-1" />
          <div className="flex gap-2">
            <Button
              tone="secondary"
              onClick={() => void navigate(`${ROUTES.runs}?run=${run.id}`)}
            >
              Полный журнал
            </Button>
            <Button
              tone="danger"
              disabled={store.busyId !== null || run.finishedAt !== undefined}
              onClick={() => void store.stop(run.id)}
            >
              Остановить
            </Button>
          </div>
        </ScrollArea>
      )}
    </aside>
  );
});
