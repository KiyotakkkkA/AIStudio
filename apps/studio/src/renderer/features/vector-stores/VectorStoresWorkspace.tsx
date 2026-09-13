import { mdiDatabaseOutline, mdiMagnify, mdiPlus, mdiRefresh } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useEffect } from "react";
import { useStore } from "../../stores/useStore";
import PageShell from "../../ui/templates/PageShell";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import TextInput from "../../ui/atoms/TextInput";
import VectorStoreDetail from "./VectorStoreDetail";
import VectorStoreForm from "./VectorStoreForm";
import { healthColors, healthLabels } from "./vectorPresentation";
import ListRow from "../../ui/molecules/ListRow";
import Icon from "../../ui/atoms/Icon";
import EmptyState from "../../ui/molecules/EmptyState";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";

function VectorStoresWorkspace() {
  const { vectorStores: store } = useStore();
  useEffect(() => {
    void store.load();
  }, [store]);
  return (
    <PageShell
      icon={mdiDatabaseOutline}
      title="Векторные хранилища"
      subtitle={`${store.stores.length} хранилищ`}
      actions={
        <div className="flex flex-none items-center gap-3">
          <div className="w-60">
            <TextInput
              preset="search"
              aria-label="Поиск хранилищ"
              placeholder="Поиск хранилищ…"
              value={store.filter}
              onChange={(e) => store.set("filter", e.target.value)}
            />
          </div>
          <Button
            disabled={store.busy || store.searching || !!store.form || !store.stores.length}
            onClick={() => {
              void store.reconcile(true);
            }}
          >
            <Icon path={mdiRefresh} size={15} />
            Полная переиндексация
          </Button>
          <Button tone="primary" disabled={store.busy || !!store.form} onClick={store.create}>
            <Icon path={mdiPlus} size={15} />
            Новое хранилище
          </Button>
        </div>
      }
    >
      {store.error ? (
        <div
          role="alert"
          className="flex items-center gap-3 rounded border border-err-border p-3 text-xs text-err"
        >
          <span className="flex-1">{store.error}</span>
          <Button
            disabled={store.loading || store.busy}
            onClick={() => {
              void store.load();
            }}
          >
            Обновить
          </Button>
        </div>
      ) : null}
      <div className="flex min-h-0 flex-1 gap-4.5">
        <aside className="flex min-h-0 w-62.5 flex-none flex-col gap-2">
          <div className="flex-none px-0.5 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
            Хранилища
          </div>
          <ScrollArea className="flex min-h-0 flex-1 flex-col gap-2 pr-0.5">
            {store.loading && !store.loaded ? (
              <>
                <ListRowSkeleton />
                <ListRowSkeleton />
              </>
            ) : null}
            {store.visibleStores.map((item) => (
              <ListRow
                key={item.id}
                disabled={store.busy || !!store.form}
                selected={store.selectedId === item.id}
                onClick={() => {
                  void store.select(item.id);
                }}
              >
                <span className="flex w-full items-center gap-2">
                  <span
                    title={healthLabels[item.status]}
                    className={`size-1.75  flex-none rounded-full ${healthColors[item.status]}`}
                  />
                  <span className="min-w-0 flex-1 wrap-break-word text-[13px] font-semibold">
                    {item.name}
                  </span>
                  <Chip>{item.backend}</Chip>
                </span>
                <span className="text-[11.5px] text-main-400">{item.description}</span>
                <span className="font-mono text-[10.5px] text-main-500">
                  {item.vectors.toLocaleString()} vectors · {item.dimension} dim
                </span>
              </ListRow>
            ))}
            {store.loaded && store.visibleStores.length === 0 ? (
              <div className="flex flex-1 items-center justify-center rounded-card border border-dashed border-main-600 bg-main-800/40 px-3">
                <EmptyState
                  icon={store.stores.length ? mdiMagnify : mdiDatabaseOutline}
                  title={store.stores.length ? "Ничего не найдено" : "Хранилищ пока нет."}
                  description={
                    store.stores.length
                      ? "Измените запрос, чтобы найти нужное хранилище."
                      : "Добавьте первое хранилище для документов и поиска по их содержимому."
                  }
                />
              </div>
            ) : null}

            <ListRow
              dashed
              disabled={store.busy || !!store.form}
              onClick={store.create}
              leading={
                <span className="flex size-7.5 flex-none items-center justify-center rounded-lg border border-dashed border-main-600 text-main-400">
                  <Icon path={mdiPlus} size={14} />
                </span>
              }
            >
              <span className="font-medium text-main-300">Подключить хранилище</span>
              <span className="text-[11px] text-main-400">Документы хранятся локально.</span>
            </ListRow>
          </ScrollArea>
        </aside>
        {store.form ? <VectorStoreForm /> : <VectorStoreDetail />}
      </div>
    </PageShell>
  );
}
export default observer(VectorStoresWorkspace);
