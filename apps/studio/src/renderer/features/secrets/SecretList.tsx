import { mdiKeyPlus, mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import type { SecretId } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import EmptyState from "../../ui/molecules/EmptyState";
import EntityListPanel from "../../ui/molecules/EntityListPanel";
import ListRow from "../../ui/molecules/ListRow";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { useStore } from "../../stores/useStore";
import SecretListItem from "./SecretListItem";
import { SCOPE_FILTERS, type ScopeFilter } from "./SecretStore";

const FILTER_LABELS: Record<ScopeFilter, string> = {
  all: "Все",
  personal: "Личные",
  shared: "Общие",
  public: "Публичные",
};

interface SecretListProps {
  readonly onSelect: (id: SecretId) => void;
  readonly onCreate: () => void;
}

function SecretList({ onSelect, onCreate }: SecretListProps) {
  const { secrets } = useStore();
  const now = Date.now();

  return (
    <EntityListPanel
      heading="Секреты"
      search={{
        value: secrets.query,
        label: "Поиск секретов",
        placeholder: "Поиск секретов…",
        onChange: secrets.setQuery,
      }}
      filters={SCOPE_FILTERS.map((filter) => (
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
    >
      {secrets.loading && secrets.summaries.length === 0 ? (
        <>
          <ListRowSkeleton />
          <ListRowSkeleton />
          <ListRowSkeleton />
        </>
      ) : null}

      {secrets.isEmpty ? (
        <div className="flex flex-1 items-center justify-center rounded-card bg-main-800 px-3">
          <EmptyState
            icon={mdiKeyPlus}
            title="Секретов пока нет"
            description="Добавьте ключ: значение шифруется и остаётся на этом устройстве."
          />
        </div>
      ) : null}

      {!secrets.isEmpty && !secrets.loading && secrets.visible.length === 0 ? (
        <p className="rounded-card bg-main-800 px-3 py-4.5 text-center text-[12px] text-main-400">
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
          <span className="flex size-7.5 flex-none items-center justify-center rounded-control bg-main-700 text-main-400">
            <Icon path={mdiPlus} size={14} />
          </span>
        }
      >
        <span className="font-medium text-main-300">Добавить секрет</span>
        <span className="truncate text-[11px] text-main-400">Значения хранятся локально.</span>
      </ListRow>
    </EntityListPanel>
  );
}

export default observer(SecretList);
