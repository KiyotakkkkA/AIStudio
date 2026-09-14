import { observer } from "mobx-react-lite";
import { mdiHistory } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../../ui/atoms/Chip";
import EmptyState from "../../ui/molecules/EmptyState";
import JsonBlock from "./JsonBlock";
import RunStepDetail from "./RunStepDetail";
import {
  formatDateTime,
  formatElapsed,
  KIND_LABELS,
  runElapsed,
  shortRunId,
  STATUS_LABELS,
} from "./runPresentation";
import type RunHistoryStore from "./RunHistoryStore";

export default observer(function RunDetailPanel({
  store,
}: {
  readonly store: RunHistoryStore;
}) {
  const detail = store.detail;
  if (detail === null)
    return (
      <section
        aria-label="Детали запуска"
        className="flex w-105 flex-none items-center justify-center border-l border-main-750 bg-main-900 p-4"
      >
        <EmptyState
          icon={mdiHistory}
          title={store.loadingDetail ? "Загрузка запуска…" : "Запуск не выбран"}
          description="Выберите запуск, чтобы увидеть его шаги с входом, выходом и ошибками, а также журнал."
        />
      </section>
    );
  const summary = detail.summary;
  return (
    <section
      aria-label="Детали запуска"
      className="flex w-105 flex-none flex-col border-l border-main-750 bg-main-900"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 p-3.5">
        <div className="flex items-center gap-2">
          <h2 className="m-0 min-w-0 flex-1 truncate text-[13.5px] font-semibold text-main-50">
            {summary.title}
          </h2>
          <Chip title={summary.id}>
            <span className="font-mono">{shortRunId(summary.id)}</span>
          </Chip>
        </div>
        <dl className="m-0 flex flex-col gap-1.5 rounded-card border border-main-750 bg-main-800 p-2.75 text-[11.5px]">
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Вид</dt>
            <dd className="m-0 font-mono">{KIND_LABELS[summary.kind]}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Статус</dt>
            <dd className="m-0 font-mono">{STATUS_LABELS[summary.status]}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Начат</dt>
            <dd className="m-0 font-mono">
              {formatDateTime(summary.startedAt ?? summary.createdAt)}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Длительность</dt>
            <dd className="m-0 font-mono">
              {summary.finishedAt === undefined
                ? "выполняется"
                : formatElapsed(runElapsed(summary, summary.finishedAt))}
            </dd>
          </div>
          {summary.prunedAt === undefined ? null : (
            <div className="flex justify-between gap-2">
              <dt className="text-main-400">Данные шагов удалены</dt>
              <dd className="m-0 font-mono">{formatDateTime(summary.prunedAt)}</dd>
            </div>
          )}
        </dl>
        {summary.error === undefined ? null : (
          <p className="m-0 rounded-card border border-err-border p-2.75 text-[11.5px] text-err">
            {summary.error}
          </p>
        )}

        <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Шаги ({detail.steps.length})
        </p>
        {detail.steps.length === 0 ? (
          <p className="m-0 text-[11.5px] text-main-500">Шагов не записано.</p>
        ) : (
          <ol className="m-0 flex flex-col gap-1.5 p-0">
            {detail.steps.map((step) => (
              <RunStepDetail
                key={step.id}
                step={step}
                expanded={store.expandedStepId === step.id}
                pruned={summary.prunedAt !== undefined}
                onToggle={() => store.toggleStep(step.id)}
              />
            ))}
          </ol>
        )}

        <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Входные данные
        </p>
        <JsonBlock value={detail.run.input} empty="запуск стартовал без входных данных" />

        <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Журнал ({detail.logs.length})
        </p>
        {detail.logs.length === 0 ? (
          <p className="m-0 text-[11.5px] text-main-500">
            {summary.prunedAt === undefined
              ? "Журнал пуст."
              : "Журнал удалён политикой хранения."}
          </p>
        ) : (
          <pre className="m-0 max-h-80 overflow-auto rounded-card border border-main-750 bg-main-900 p-2.5 font-mono text-[11px] leading-[1.7] text-main-300">
            {detail.logs.map((line) => JSON.stringify(line)).join("\n")}
          </pre>
        )}
      </ScrollArea>
    </section>
  );
});
