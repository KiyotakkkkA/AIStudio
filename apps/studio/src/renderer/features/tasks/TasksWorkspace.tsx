import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiClockOutline } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import PageHeader from "../../ui/organisms/PageHeader";
import EmptyState from "../../ui/molecules/EmptyState";
import TaskDetailRail from "./TaskDetailRail";
import TaskFilterRail from "./TaskFilterRail";
import TaskRow from "./TaskRow";

const GROUP_TITLES = {
  running: "Выполняется",
  queued: "В очереди и по расписанию",
  attention: "Требуют внимания",
  finished: "Завершены",
} as const;

export default observer(function TasksWorkspace() {
  const { tasks: store } = useStore();
  useEffect(() => {
    void store.mount();
    return store.dispose;
  }, [store]);

  const { byStatus } = store.counts;
  const groups = store.groups;
  const sections = (["running", "queued", "attention", "finished"] as const).filter(
    (group) => groups[group].length > 0,
  );

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiClockOutline}
        title="Задачи"
        subtitle={
          <>
              {byStatus.running} выполняется · {byStatus.queued} в очереди · {byStatus.blocked} ждут
              решения
          </>
        }
        actions={
          <>
            <Button
              tone="secondary"
              disabled={store.loading}
              onClick={() => {
                void store.load();
              }}
            >
              Обновить
            </Button>
            <Button
              tone="ghost"
              disabled={store.clearing}
              needConfirm
              modalSetup={{
                title: "Очистить историю завершённых запусков?",
                content:
                  "Завершённые запуски и их шаги будут удалены. Запуски, на которые ссылаются сообщения чата, сохранятся.",
                confirmLabel: "Очистить",
              }}
              onClick={() => {
                void store.clearFinished();
              }}
            >
              Очистить завершённые
            </Button>
          </>
        }
      />

      <div className="flex min-h-0 flex-1">
        <TaskFilterRail store={store} />
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {store.error === null ? null : (
            <div
              role="alert"
              className="flex items-center gap-3 bg-err-soft px-5 py-2.5 text-[12.5px] text-err"
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
            {sections.map((group) => (
              <section key={group}>
                <div className="flex items-center gap-2.5 border-b border-line bg-main-850 px-5 py-2">
                  <span className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                    {GROUP_TITLES[group]}
                  </span>
                  <Chip>{groups[group].length}</Chip>
                </div>
                {groups[group].map((run) => (
                  <TaskRow
                    key={run.id}
                    run={run}
                    now={store.now}
                    selected={store.selectedId === run.id}
                    busy={store.busyId !== null}
                    onSelect={() => void store.select(run.id)}
                    onStop={() => void store.stop(run.id)}
                    onRetry={() => void store.retry(run.id)}
                    onReview={() => void store.select(run.id)}
                  />
                ))}
              </section>
            ))}
            {store.loaded && sections.length === 0 ? (
              <div className="flex flex-1 items-center justify-center p-6">
                <EmptyState
                  icon={mdiClockOutline}
                  title="Активных задач нет"
                  description="Активные ходы чата и сценарии отображаются здесь."
                />
              </div>
            ) : null}
          </ScrollArea>
        </div>
        <TaskDetailRail store={store} />
      </div>
    </div>
  );
});
