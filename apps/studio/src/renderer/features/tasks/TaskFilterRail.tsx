import { observer } from "mobx-react-lite";
import { RunKind } from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";
import { KIND_FILTER_LABELS } from "./runPresentation";
import type { TaskStatusFilter } from "./runGroups";
import TaskFilterButton from "./TaskFilterButton";
import type TaskStore from "./TaskStore";

const STATUS_FILTERS: readonly { key: TaskStatusFilter; label: string; tone?: StatusTone }[] = [
  { key: "active", label: "Всё активное" },
  { key: "running", label: "Выполняется", tone: "accent" },
  { key: "queued", label: "В очереди", tone: "idle" },
  { key: "blocked", label: "Ждут решения", tone: "warn" },
  { key: "failed", label: "С ошибкой", tone: "err" },
];

export default observer(function TaskFilterRail({ store }: { readonly store: TaskStore }) {
  return (
    <aside
      aria-label="Фильтры задач"
      className="flex w-54 flex-none flex-col gap-0.5 border-r border-main-750 p-2.5"
    >
      <p className="px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
        Статус
      </p>
      {STATUS_FILTERS.map((filter) => (
        <TaskFilterButton
          key={filter.key}
          active={store.status === filter.key}
          label={filter.label}
          count={store.statusCounts[filter.key]}
          tone={filter.tone}
          onClick={() => store.setStatus(filter.key)}
        />
      ))}
      <p className="mt-3.5 px-2 pb-1.5 text-[10px] font-medium tracking-[0.09em] text-main-500 uppercase">
        Вид
      </p>
      <TaskFilterButton
        active={store.kind === "all"}
        label="Все виды"
        count={store.visible.length}
        onClick={() => store.setKind("all")}
      />
      {RunKind.options.map((kind) => (
        <TaskFilterButton
          key={kind}
          active={store.kind === kind}
          label={KIND_FILTER_LABELS[kind]}
          count={store.kindCounts[kind]}
          onClick={() => store.setKind(kind)}
        />
      ))}
      <div className="flex-1" />
      <p className="rounded-card border border-main-750 bg-main-900 p-2.75 text-[11.5px] leading-[1.55] text-main-400">
        Задачи — это то, что происходит сейчас. Завершённая работа и её журналы живут в разделе
        «Запуски и логи».
      </p>
    </aside>
  );
});
