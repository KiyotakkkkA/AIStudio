import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiHistory } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useSearchParams } from "react-router-dom";
import { RunId, RunKind, RunStatus } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/Button";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import RunDetailPanel from "./RunDetailPanel";
import RunHistoryRow from "./RunHistoryRow";
import TaskFilterButton from "./TaskFilterButton";
import { KIND_FILTER_LABELS, STATUS_LABELS, STATUS_TONES } from "./runPresentation";
import type { RunRangeKey } from "./RunHistoryStore";

const RANGES: readonly { key: RunRangeKey; label: string }[] = [
  { key: "all", label: "За всё время" },
  { key: "today", label: "За сутки" },
  { key: "week", label: "За 7 дней" },
  { key: "month", label: "За 30 дней" },
];

export default observer(function RunsWorkspace() {
  const { runs: store } = useStore();
  const [params] = useSearchParams();
  const requested = params.get("run");

  useEffect(() => {
    void store.mount();
    return store.dispose;
  }, [store]);

  useEffect(() => {
    const parsed = RunId.safeParse(requested);
    if (parsed.success) void store.select(parsed.data);
  }, [store, requested]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex h-header flex-none items-center gap-3 border-b border-main-750 bg-main-900 px-5">
        <Icon path={mdiHistory} size={20} className="flex-none text-accent-dark" />
        <h1 className="m-0 flex-none text-xl/7 font-semibold tracking-[-0.01em] text-main-50">
          Запуски и логи
        </h1>
        <span className="ml-1.5 truncate text-xs/4 text-main-400">
          {store.counts.total} запусков в истории
        </span>
        <div className="flex-1" />
        <div className="w-64">
          <TextInput
            preset="search"
            aria-label="Поиск по запускам и шагам"
            placeholder="Поиск по названию, id и шагам…"
            value={store.query}
            onChange={(event) => store.setQuery(event.target.value)}
          />
        </div>
        <Button tone="ghost" disabled={!store.filtered} onClick={store.reset}>
          Сбросить
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside
          aria-label="Фильтры истории"
          className="flex w-54 flex-none flex-col gap-0.5 border-r border-main-750 p-2.5"
        >
          <p className="px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
            Период
          </p>
          {RANGES.map((range) => (
            <TaskFilterButton
              key={range.key}
              active={store.range === range.key}
              label={range.label}
              count={range.key === "all" ? store.counts.total : store.runs.length}
              onClick={() => store.setRange(range.key)}
            />
          ))}
          <p className="mt-3.5 px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
            Статус
          </p>
          {RunStatus.options.map((status) => (
            <TaskFilterButton
              key={status}
              active={store.statuses.includes(status)}
              label={STATUS_LABELS[status]}
              tone={STATUS_TONES[status]}
              count={store.counts.byStatus[status]}
              onClick={() => store.toggleStatus(status)}
            />
          ))}
          <p className="mt-3.5 px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
            Вид
          </p>
          {RunKind.options.map((kind) => (
            <TaskFilterButton
              key={kind}
              active={store.kinds.includes(kind)}
              label={KIND_FILTER_LABELS[kind]}
              count={store.counts.byKind[kind]}
              onClick={() => store.toggleKind(kind)}
            />
          ))}
        </aside>

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {store.error === null ? null : (
            <div
              role="alert"
              className="flex items-center gap-3 border-b border-err-border px-4 py-2.5 text-xs text-err"
            >
              <span className="min-w-0 flex-1">{store.error}</span>
              <Button
                tone="ghost"
                onClick={() => {
                  void store.load();
                }}
              >
                Повторить
              </Button>
            </div>
          )}
          <ScrollArea className="flex min-h-0 flex-1 flex-col">
            {store.runs.map((run) => (
              <RunHistoryRow
                key={run.id}
                run={run}
                selected={store.selectedId === run.id}
                onSelect={() => void store.select(run.id)}
              />
            ))}
            {store.hasMore ? (
              <div className="flex justify-center p-3">
                <Button
                  tone="secondary"
                  disabled={store.loadingMore}
                  onClick={() => {
                    void store.loadMore();
                  }}
                >
                  Показать ещё
                </Button>
              </div>
            ) : null}
            {store.loaded && store.runs.length === 0 ? (
              <div className="flex flex-1 items-center justify-center p-6">
                <EmptyState
                  icon={mdiHistory}
                  title={store.filtered ? "Ничего не найдено" : "Запусков пока нет"}
                  description={
                    store.filtered
                      ? "Измените фильтры или поисковый запрос."
                      : "Здесь появится история завершённых запусков с шагами и журналами."
                  }
                />
              </div>
            ) : null}
          </ScrollArea>
        </div>

        <RunDetailPanel store={store} />
      </div>
    </div>
  );
});
