import { mdiKeyPlus, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { SecretId } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRow from "../../ui/molecules/ListRow";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import useStore from "../../stores/useStore";
import SecretListItem from "./SecretListItem";
import { SCOPE_FILTERS, type ScopeFilter } from "./SecretStore";

const FILTER_LABELS: Record<ScopeFilter, string> = {
  all: "Все",
  personal: "Личные",
  shared: "Общие",
  public: "Публичные",
};

export interface SecretListProps {
  readonly onSelect: (id: SecretId) => void;
  readonly onCreate: () => void;
}

function SecretList({ onSelect, onCreate }: SecretListProps) {
  const { secrets } = useStore();
  const now = Date.now();
  const typeNames = secrets.types.map((schema) => schema.label).join(" · ");

  return (
    <div className="flex min-h-0 w-[396px] flex-none flex-col gap-[10px]">
      <div className="flex flex-none gap-[6px]">
        {SCOPE_FILTERS.map((filter) => (
          <Chip
            key={filter}
            tone={secrets.filter === filter ? "selected" : "neutral"}
            onClick={() => {
              secrets.setFilter(filter);
            }}
          >
            {`${FILTER_LABELS[filter]} · ${String(secrets.counts[filter])}`}
          </Chip>
        ))}
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto pr-0.5">
        {secrets.loading && secrets.summaries.length === 0 ? (
          <>
            <ListRowSkeleton />
            <ListRowSkeleton />
            <ListRowSkeleton />
          </>
        ) : null}

        {secrets.isEmpty ? (
          <div className="flex flex-1 items-center justify-center rounded-card border border-dashed border-main-600 bg-main-800/40 px-4">
            <EmptyState
              icon={mdiKeyPlus}
              title="Секретов пока нет"
              description="Добавьте первый ключ — он будет зашифрован и останется на этой машине."
            />
          </div>
        ) : null}

        {!secrets.isEmpty && !secrets.loading && secrets.visible.length === 0 ? (
          <p className="rounded-card border border-dashed border-main-600 px-3 py-4.5 text-center text-[12px] text-main-400">
            Ничего не найдено по текущему фильтру.
          </p>
        ) : null}

        {secrets.visible.map((secret) => (
          <SecretListItem
            key={secret.id}
            secret={secret}
            schema={secrets.schemaOf(secret.type)}
            selected={secrets.selectedId === secret.id}
            now={now}
            onSelect={() => {
              onSelect(secret.id);
            }}
          />
        ))}

        <ListRow
          dashed
          onClick={onCreate}
          leading={
            <span className="flex size-[32px] flex-none items-center justify-center rounded-[8px] border border-dashed border-main-600 text-main-400">
              <Icon path={mdiPlus} size={15} />
            </span>
          }
        >
          <span className="font-medium text-main-300">Добавить секрет по схеме провайдера</span>
          <span className="truncate text-[11px] text-main-400">{typeNames}</span>
        </ListRow>
      </div>
    </div>
  );
}

export default observer(SecretList);
