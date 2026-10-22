import { mdiLayersOutline, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { ProviderId } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import StatusDot from "../../ui/atoms/StatusDot";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRow from "../../ui/molecules/ListRow";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { useStore } from "../../stores/useStore";
import {
  CAPABILITY_LIST_HEADINGS,
  KIND_LABELS,
  statusTone,
  summaryLine,
} from "./providerPresentation";

interface ProviderListProps {
  readonly onSelect: (id: ProviderId) => void;
  readonly onCreate: () => void;
}

function ProviderList({ onSelect, onCreate }: ProviderListProps) {
  const { providers } = useStore();

  return (
    <div className="flex min-h-0 w-65.5 flex-none flex-col gap-2 border-r border-line pr-2.5">
      <div className="flex-none px-0.5 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        {CAPABILITY_LIST_HEADINGS[providers.capability]}
      </div>
      <TextInput
        preset="search"
        aria-label="Поиск подключений"
        placeholder="Поиск подключений…"
        value={providers.listQuery}
        onChange={(event) => {
          providers.setListQuery(event.target.value);
        }}
      />

      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2 pr-0.5">
        {providers.loading && providers.summaries.length === 0 ? (
          <>
            <ListRowSkeleton />
            <ListRowSkeleton />
          </>
        ) : null}

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
              <Chip>{KIND_LABELS[summary.kind]}</Chip>
            </span>
            <span
              className={`truncate text-[11px] ${
                summary.status === "failed"
                  ? "text-err"
                  : summary.status === "degraded" || summary.status === "needs-relink"
                    ? "text-warn"
                    : "text-main-400"
              }`}
            >
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
      </ScrollArea>
    </div>
  );
}

export default observer(ProviderList);
