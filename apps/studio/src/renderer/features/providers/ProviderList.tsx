import { mdiLayersOutline, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import EntityListPanel from "../../ui/molecules/EntityListPanel";
import type { ProviderId } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRow from "../../ui/molecules/ListRow";
import { useStore } from "../../stores/useStore";
import {
  CAPABILITY_LIST_HEADINGS,
  KIND_LABELS,
  statusTone,
  summaryLine,
} from "./providerPresentation";
import { ListSkeleton } from "../../ui/organisms/PageSkeletons";
import { awaitingData } from "../../ui/molecules/LoadingSkeleton";

interface ProviderListProps {
  readonly onSelect: (id: ProviderId) => void;
  readonly onCreate: () => void;
}

function ProviderList({ onSelect, onCreate }: ProviderListProps) {
  const { providers } = useStore();

  return (
    <EntityListPanel
      heading={CAPABILITY_LIST_HEADINGS[providers.capability]}
      search={{
        value: providers.listQuery,
        label: "Поиск подключений",
        placeholder: "Поиск подключений…",
        onChange: providers.setListQuery,
      }}
    >
      {awaitingData(providers) ? <ListSkeleton /> : null}

      {providers.isEmpty ? (
        <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-3">
          <EmptyState
            icon={mdiLayersOutline}
            title="Подключений нет"
            description="Добавьте подключение: ключ выбирается из секретов, список моделей приходит после проверки связи."
          />
        </div>
      ) : null}

      {providers.visibleSummaries.map((summary) => (
        <ListRow
          key={summary.id}
          selected={providers.selectedId === summary.id}
          onClick={() => {
            onSelect(summary.id);
          }}
        >
          <span className="flex min-w-0 items-center gap-2">
            <StatusDot tone={statusTone(summary.status)} title={summaryLine(summary)} />
            <span className="flex-1 truncate text-[12.5px] font-medium text-main-50">
              {summary.name}
            </span>
          </span>
          <span
            className={`truncate text-[11px] ${
              summary.status === "failed"
                ? "text-err"
                : summary.status === "degraded" || summary.status === "needs-relink"
                  ? "text-warn"
                  : "text-main-400"
            }`}
            title={`${KIND_LABELS[summary.kind]} · ${summaryLine(summary)}`}
          >
            <span className="text-main-500">{KIND_LABELS[summary.kind]}</span> ·{" "}
            {summaryLine(summary)}
          </span>
        </ListRow>
      ))}

      <ListRow
        dashed
        onClick={onCreate}
        leading={
          <span className="flex size-7.5 flex-none items-center justify-center rounded-control bg-main-700 text-main-400">
            <Icon path={mdiPlus} size={14} />
          </span>
        }
      >
        <span className="font-medium text-main-300">Добавить подключение</span>
        <span className="truncate text-[11px] text-main-400">Подключения хранятся локально.</span>
      </ListRow>
    </EntityListPanel>
  );
}

export default observer(ProviderList);
