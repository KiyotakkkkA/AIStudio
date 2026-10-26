import {
  mdiContentCopy,
  mdiDatabaseOutline,
  mdiDeleteOutline,
  mdiDeleteSweepOutline,
  mdiDotsHorizontal,
  mdiPencilOutline,
  mdiFolderPlusOutline,
  mdiRefresh,
  mdiScaleBalance,
  mdiStop,
  mdiSync,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { useState } from "react";
import { Dropdown, Switcher } from "@kiyotakkkka/zvs-uikit-lib";
import { VECTOR_SCHEMA_VERSION, type VectorStoreDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import IconButton from "../../ui/atoms/buttons/IconButton";
import IconDropdownButton from "../../ui/atoms/buttons/IconDropdownButton";
import StatusDot from "../../ui/atoms/StatusDot";
import Alert from "../../ui/molecules/Alert";
import ConfirmDialog from "../../ui/molecules/ConfirmDialog";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "../../ui/molecules/ActionMenu";
import EmptyState from "../../ui/molecules/EmptyState";
import VectorSearchPanel from "./VectorSearchPanel";
import VectorDocumentsPanel from "./VectorDocumentsPanel";
import VectorRelationsPanel from "./VectorRelationsPanel";
import VectorEvalSection from "./VectorEvalSection";
import Disclosure from "./Disclosure";
import {
  PRIMARY_ACTION_LABELS,
  VECTOR_STORE_TABS,
  healthLabels,
  healthTones,
  primaryAction,
  storeFacts,
  type PrimaryAction,
} from "./vectorPresentation";
import { DetailSkeleton } from "../../ui/molecules/LoadingSkeleton";

const PRIMARY_ICON_TONE =
  "zvs-press border border-transparent bg-accent-dark text-on-accent hover:bg-accent-medium";
const PRIMARY_ICONS: Record<PrimaryAction, string> = {
  stop: mdiStop,
  add: mdiFolderPlusOutline,
  reconcile: mdiScaleBalance,
  rebuild: mdiRefresh,
  update: mdiSync,
};

function VectorStoreDetail() {
  const { vectorStores: store } = useStore();
  const [confirm, setConfirm] = useState<"clear" | "remove" | null>(null);
  const detail = store.detail;
  if (!detail && (store.loading || (store.selectedId && !store.error)))
    return (
      <div className="flex min-w-0 flex-1 flex-col p-4.5">
        <DetailSkeleton label="Загрузка хранилища…" sections={3} />
      </div>
    );
  if (!detail)
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center p-4.5">
        <EmptyState
          icon={mdiDatabaseOutline}
          title="Хранилище не выбрано"
          description="Выберите хранилище слева или добавьте новое, чтобы настроить поиск по документам."
        />
      </div>
    );
  const outdated = detail.schemaVersion < VECTOR_SCHEMA_VERSION;
  const disabled = store.busy || store.searching;
  const action = primaryAction({
    status: detail.status,
    indexing: store.indexing,
    sources: store.sources.length,
    outdated,
    vectors: detail.vectors,
  });
  const runPrimary = () => {
    if (action === "stop") void store.cancelIndex();
    else if (action === "reconcile") void store.reconcile();
    else if (action === "add") {
      store.set("tab", "Documents");
      void store.pickSource("folder");
    } else void store.startIndex(action === "rebuild");
  };
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <section className="flex flex-none flex-col gap-3 px-6 pt-5 pb-4">
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
              <h2 className="wrap-break-word text-[17px] font-semibold tracking-[-0.015em] text-main-50">
                {detail.name}
              </h2>
              <span className="flex items-center gap-1.5 text-[12px] text-main-400">
                <StatusDot tone={healthTones[detail.status]} />
                {healthLabels[detail.status]}
              </span>
            </div>
            {detail.description ? (
              <p className="mt-1 max-w-3xl text-[12.5px] text-main-400">{detail.description}</p>
            ) : null}
            <p className="mt-2 flex flex-wrap gap-x-2 text-[12px] text-main-300">
              {(detail.status === "broken"
                ? ["статистика не подтверждена"]
                : storeFacts(detail)
              ).map((fact, i) => (
                <span key={fact} className="flex items-center gap-2">
                  {i === 0 ? null : <span className="text-main-500">·</span>}
                  {fact}
                </span>
              ))}
            </p>
          </div>
          <div className="flex flex-none items-center gap-1.5">
            <IconButton
              size="md"
              tooltip="bottom-center"
              path={PRIMARY_ICONS[action]}
              iconSize={17}
              label={PRIMARY_ACTION_LABELS[action]}
              tone={action === "stop" ? "danger" : "ghost"}
              {...(action === "stop" ? {} : { toneClassName: PRIMARY_ICON_TONE })}
              disabled={action === "stop" ? false : disabled}
              onClick={runPrimary}
            />
            <Dropdown menuWidth={260} menuPlacement="bottom-right" disabled={disabled}>
              <IconDropdownButton
                label="Действия с хранилищем"
                tooltip="bottom-right"
                path={mdiDotsHorizontal}
                disabled={disabled}
              />
              <ActionMenu label="Действия с хранилищем">
                <ActionMenuItem icon={mdiPencilOutline} onClick={store.edit}>
                  Изменить параметры
                </ActionMenuItem>
                <ActionMenuItem icon={mdiContentCopy} onClick={store.duplicate}>
                  Дублировать
                </ActionMenuItem>
                <ActionMenuItem icon={mdiScaleBalance} onClick={() => void store.reconcile()}>
                  Сверить статистику
                </ActionMenuItem>
                <ActionMenuItem
                  icon={mdiRefresh}
                  disabled={store.indexing || store.sources.length === 0}
                  onClick={() => void store.startIndex(true)}
                >
                  Полная переиндексация
                </ActionMenuItem>
                <ActionMenuSeparator />
                <ActionMenuItem
                  icon={mdiDeleteSweepOutline}
                  danger
                  disabled={store.indexing || (detail.documents === 0 && detail.vectors === 0)}
                  onClick={() => setConfirm("clear")}
                >
                  Очистить содержимое
                </ActionMenuItem>
                <ActionMenuItem icon={mdiDeleteOutline} danger onClick={() => setConfirm("remove")}>
                  Удалить хранилище
                </ActionMenuItem>
              </ActionMenu>
            </Dropdown>
          </div>
        </div>
        <StatusNotice detail={detail} outdated={outdated} />
        <Disclosure summary="Технические сведения">
          <TechFacts detail={detail} outdated={outdated} />
        </Disclosure>
      </section>
      <div className="flex h-11 flex-none items-center border-b border-line px-5">
        <Switcher
          value={store.tab}
          label="Раздел хранилища"
          options={[...VECTOR_STORE_TABS]}
          onChange={(tab) => {
            store.set("tab", tab);
          }}
          rounded=""
          className="gap-1 rounded-control bg-transparent border-transparent"
          classNames={{ tab: "rounded-pill px-[14px] py-[5px] text-[12px]" }}
        />
      </div>
      {store.tab === "Test search" ? (
        <VectorSearchPanel />
      ) : store.tab === "Relations" ? (
        <VectorRelationsPanel />
      ) : store.tab === "Quality" ? (
        <VectorEvalSection />
      ) : (
        <VectorDocumentsPanel />
      )}
      <ConfirmDialog
        open={confirm === "clear"}
        title={`Очистить «${detail.name}»?`}
        confirmLabel="Очистить"
        confirmTone="danger"
        onCancel={() => setConfirm(null)}
        onConfirm={() => {
          setConfirm(null);
          void store.clear();
        }}
      >
        Из хранилища будут удалены все фрагменты ({detail.vectors.toLocaleString("ru-RU")}) и
        документы ({detail.documents.toLocaleString("ru-RU")}). Источники и настройки сохранятся.
        Действие нельзя отменить.
      </ConfirmDialog>
      <ConfirmDialog
        open={confirm === "remove"}
        title={`Удалить «${detail.name}»?`}
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirm(null)}
        onConfirm={() => {
          setConfirm(null);
          void store.remove();
        }}
      >
        Будут удалены хранилище и его документы ({detail.documents.toLocaleString("ru-RU")}). Файлы
        на диске останутся. Действие нельзя отменить.
      </ConfirmDialog>
    </div>
  );
}

const StatusNotice = observer(function StatusNotice({
  detail,
  outdated,
}: {
  readonly detail: VectorStoreDto;
  readonly outdated: boolean;
}) {
  const { vectorStores: store } = useStore();
  if (detail.status === "broken")
    return (
      <Alert title="Таблица хранилища недоступна">
        Статистика не подтверждена. Сверка обновит сведения, но не восстановит удалённые данные.
      </Alert>
    );
  if (store.indexing) return null;
  if (detail.status === "pending" || detail.vectors === 0)
    return (
      <Alert tone="info" title="Хранилище ещё не проиндексировано">
        {store.sources.length === 0
          ? "Добавьте папку или файлы с документами, затем запустите индексацию."
          : "Источники добавлены. Запустите индексацию, чтобы документы стали доступны для поиска."}
      </Alert>
    );
  if (outdated)
    return (
      <Alert tone="warning" title="Требуется переиндексация">
        Хранилище создано в старом формате индекса: поиск работает только по смыслу, без ключевых
        слов, страниц и разделов.
      </Alert>
    );
  return null;
});

const TechFacts = observer(function TechFacts({
  detail,
  outdated,
}: {
  readonly detail: VectorStoreDto;
  readonly outdated: boolean;
}) {
  const { vectorStores: store } = useStore();
  const facts: [string, string][] = [
    ["Модель эмбеддингов", detail.embeddingModelId],
    [
      "Провайдер",
      store.providers.find((p) => p.id === detail.embeddingProviderId)?.name ??
        detail.embeddingProviderId,
    ],
    ["Размерность", String(detail.dimension)],
    ["Индекс", detail.indexType],
    ["Метрика", detail.metric],
    ["Режим поиска", detail.searchMode === "hybrid" && !outdated ? "гибридный" : "векторный"],
    ["Движок", detail.backend],
    ["Векторы", detail.vectors.toLocaleString("ru-RU")],
  ];
  return (
    <dl className="m-0 grid grid-cols-[repeat(auto-fill,minmax(170px,1fr))] gap-x-6 gap-y-2.5">
      {facts.map(([label, value]) => (
        <div key={label} className="min-w-0">
          <dt className="text-[11px] text-main-500">{label}</dt>
          <dd className="m-0 truncate font-mono text-[12px] text-main-200" title={value}>
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
});

export default observer(VectorStoreDetail);
