import { mdiDatabaseOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea, Switcher } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import VectorSearchPanel from "./VectorSearchPanel";
import VectorDocumentsPanel from "./VectorDocumentsPanel";
import { formatBytes } from "../downloads/downloadPresentation";
import { healthLabels } from "./vectorPresentation";
import EmptyState from "../../ui/molecules/EmptyState";

const VECTOR_STORE_TABS = [
  { value: "Overview", label: "Обзор" },
  { value: "Documents", label: "Документы" },
  { value: "Test search", label: "Тестовый поиск" },
  { value: "Settings", label: "Настройки" },
] as const;

function VectorStoreDetail() {
  const { vectorStores: store } = useStore();
  const detail = store.detail;
  if (!detail)
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center rounded-card border border-main-750 bg-main-900">
        <EmptyState
          icon={mdiDatabaseOutline}
          title={
            store.loading || (store.selectedId && !store.error)
              ? "Загрузка хранилища…"
              : "Хранилище не выбрано"
          }
          description={
            store.loading || (store.selectedId && !store.error)
              ? "Получаем настройки и статистику хранилища."
              : "Выберите хранилище слева или добавьте новое, чтобы настроить поиск по документам."
          }
        />
      </div>
    );
  const stats = [
    [detail.documents.toLocaleString(), "Документы"],
    [detail.vectors.toLocaleString(), "Векторы"],
    [String(detail.dimension), "Размерность"],
    [detail.indexType, `Индекс · ${detail.metric}`],
    [formatBytes(detail.bytes), "На диске"],
    [
      detail.embeddingModelId,
      `Embedding · ${store.providers.find((p) => p.id === detail.embeddingProviderId)?.name ?? detail.embeddingProviderId}`,
    ],
  ];
  const disabled = store.busy || store.searching;
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3.5">
      <div className="flex flex-none flex-col gap-3.5 rounded-card border border-main-750 bg-main-900 px-4.5 py-4">
        <div className="flex flex-wrap items-start gap-3">
          <div className="flex size-9.5  flex-none items-center justify-center rounded-card bg-main-700 text-accent-medium">
            <Icon path={mdiDatabaseOutline} size={19} />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="wrap-break-word text-[15px] font-semibold">{detail.name}</h2>
              <Chip>{healthLabels[detail.status]}</Chip>
            </div>
            <p className="mt-0.75 text-xs text-main-400">{detail.description}</p>
          </div>
          <div className="flex gap-1.75">
            <Button disabled={disabled} onClick={store.edit}>
              Изменить
            </Button>
            <Button
              disabled={disabled || store.indexing || store.sources.length === 0}
              title="Переиндексировать все файлы источников"
              onClick={() => {
                void store.startIndex(true);
              }}
            >
              Полная переиндексация
            </Button>
            <Button
              disabled={disabled}
              title="Сверить статистику с таблицей"
              onClick={() => {
                void store.reconcile();
              }}
            >
              Сверить
            </Button>
            <Button
              tone="danger"
              disabled={disabled || (detail.documents === 0 && detail.vectors === 0)}
              title="Очистить содержимое, не удаляя хранилище"
              needConfirm
              modalSetup={{
                title: `Очистить «${detail.name}»?`,
                content: `Из хранилища будут удалены все векторы (${detail.vectors.toLocaleString("ru-RU")}) и документы (${detail.documents.toLocaleString("ru-RU")}). Само хранилище, его источники и настройки останутся — индексацию можно запустить заново. Это действие нельзя отменить.`,
                tone: "danger",
                confirmLabel: "Очистить",
              }}
              onClick={() => {
                void store.clear();
              }}
            >
              Сброс
            </Button>
            <Button
              tone="danger"
              disabled={disabled}
              needConfirm
              modalSetup={{
                title: `Удалить «${detail.name}»?`,
                content: `Будут удалены хранилище и его документы (${detail.documents}). Зависимые чаты и сценарии ещё не подключены к хранилищам. Это действие нельзя отменить.`,
                tone: "danger",
                confirmLabel: "Удалить",
              }}
              onClick={() => {
                void store.remove();
              }}
            >
              Удалить
            </Button>
          </div>
        </div>
        <div className="flex overflow-hidden rounded-[9px] border border-main-750 bg-main-800">
          {stats.map(([value, label], i) => (
            <div
              key={label}
              className={`min-w-0 flex-1 border-r border-main-750 px-3.25 py-2.75 last:border-r-0 ${i === 5 ? "grow-[1.6]" : ""}`}
            >
              <div
                title={value}
                className={`truncate text-[17px] font-semibold ${i === 5 ? "text-accent-medium" : ""}`}
              >
                {detail.status === "broken" && i !== 2 && i !== 5 ? "—" : value}
              </div>
              <div title={label} className="mt-px truncate text-[10.5px] text-main-500">
                {label}
              </div>
            </div>
          ))}
        </div>
        {detail.status === "broken" ? (
          <div role="alert" className="flex items-center gap-3 text-xs text-err">
            <span>
              Таблица недоступна. Статистика не подтверждена. Сверка не восстанавливает удалённые
              данные.
            </span>
            <Button
              disabled={disabled}
              onClick={() => {
                void store.reconcile();
              }}
            >
              Сверить
            </Button>
          </div>
        ) : detail.status === "pending" || detail.vectors === 0 ? (
          <p className="text-xs text-warn">
            Ожидает индексации. Добавьте источники на вкладке «Документы» и запустите индексацию.
          </p>
        ) : null}
      </div>
      <div className="flex min-h-0 flex-1 flex-col rounded-card border border-main-750 bg-main-900">
        <div className="flex h-11 flex-none items-center border-b border-main-750 px-3.5">
          <Switcher
            value={store.tab}
            label="Раздел хранилища"
            options={[...VECTOR_STORE_TABS]}
            onChange={(tab) => {
              store.set("tab", tab);
            }}
            rounded=""
            className="gap-1 rounded-lg bg-transparent border-transparent"
            classNames={{ tab: "rounded-[5px] px-[14px] py-[5px] text-[12px]" }}
          />
        </div>
        {store.tab === "Test search" ? (
          <VectorSearchPanel />
        ) : store.tab === "Documents" ? (
          <VectorDocumentsPanel />
        ) : (
          <ScrollArea role="tabpanel" className="space-y-3 p-4 text-xs text-main-300">
            {store.tab === "Settings" ? (
              <>
                <p>
                  Чанк: {detail.chunkSize} · перекрытие: {detail.chunkOverlap}
                </p>
                <p>
                  Движок: {detail.backend} · метрика: {detail.metric}
                </p>
                <p>
                  Переранжирование:{" "}
                  {detail.rerank.enabled
                    ? `${describeRef(detail.rerank.modelRef)} · ${detail.rerank.candidates} кандидатов`
                    : "выключено"}
                </p>
                <p>
                  OCR:{" "}
                  {detail.ocr.enabled
                    ? `${describeRef(detail.ocr.modelRef)} · язык: ${detail.ocr.language} · порог: ${detail.ocr.minCharsPerPage}`
                    : "выключен"}
                </p>
              </>
            ) : (
              <>
                <p>{detail.description || "Описание не задано."}</p>
                <p>
                  Хранилище: {detail.backend} · {healthLabels[detail.status]}
                </p>
                <p>
                  Последняя индексация:{" "}
                  {detail.lastIndexedAt === null
                    ? "ещё не выполнялась"
                    : new Date(detail.lastIndexedAt).toLocaleString()}
                </p>
                <p>
                  Источников: {store.sources.length} · документов: {store.documents.length}
                </p>
              </>
            )}
          </ScrollArea>
        )}
      </div>
    </div>
  );
}
/** A catalogue ref is `<source>:<kind>:<name>`; the name is the part worth showing. */
function describeRef(ref: string): string {
  return ref.split(":").slice(2).join(":") || ref || "модель не выбрана";
}

export default observer(VectorStoreDetail);
