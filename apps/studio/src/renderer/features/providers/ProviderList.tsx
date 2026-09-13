import { mdiLayersOutline, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { ProviderId } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRow from "../../ui/molecules/ListRow";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { useStore } from "../../stores/useStore";
import {
  CAPABILITY_LIST_HEADINGS,
  providerInitials,
  statusTone,
  summaryLine,
} from "./providerPresentation";

export interface ProviderListProps {
  readonly onSelect: (id: ProviderId) => void;
  readonly onCreate: () => void;
}

function ProviderList({ onSelect, onCreate }: ProviderListProps) {
  const { providers } = useStore();

  return (
    <div className="flex min-h-0 w-62.5 flex-none flex-col gap-2">
      <div className="flex-none px-0.5 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
        {CAPABILITY_LIST_HEADINGS[providers.capability]}
      </div>

      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2 pr-0.5">
        {providers.loading && providers.summaries.length === 0 ? (
          <>
            <ListRowSkeleton />
            <ListRowSkeleton />
          </>
        ) : null}

        {providers.isEmpty ? (
          <div className="flex flex-1 items-center justify-center rounded-card border border-dashed border-main-600 bg-main-800/40 px-3">
            <EmptyState
              icon={mdiLayersOutline}
              title="Подключений нет"
              description="Добавьте первое подключение: ключ берётся из секретов, модели — из проверки связи."
            />
          </div>
        ) : null}

        {providers.summaries.map((summary) => (
          <ListRow
            key={summary.id}
            selected={providers.selectedId === summary.id}
            onClick={() => {
              onSelect(summary.id);
            }}
            leading={
              <span className="flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 text-[11px] font-semibold text-accent-medium">
                {providerInitials(summary.name, summary.kind)}
              </span>
            }
            trailing={
              <span className="flex flex-none items-center pt-1.5">
                <StatusDot tone={statusTone(summary.status)} title={summaryLine(summary)} />
              </span>
            }
          >
            <span className="truncate font-semibold text-main-50">{summary.name}</span>
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
            <span className="flex size-7.5 flex-none items-center justify-center rounded-lg border border-dashed border-main-600 text-main-400">
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
