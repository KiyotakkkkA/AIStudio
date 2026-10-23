import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiPause, mdiRefresh, mdiTrayArrowDown } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type { CatalogueItemDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import PageHeader from "../../ui/organisms/PageHeader";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import CatalogueRow from "./CatalogueRow";
import DownloadDetailRail from "./DownloadDetailRail";
import DownloadFailureRow from "./DownloadFailureRow";
import DownloadFilterRail from "./DownloadFilterRail";
import DownloadProgressRow from "./DownloadProgressRow";
import DownloadQueueRow from "./DownloadQueueRow";
import { formatBytes, formatRate, SECTION_LABELS } from "./downloadPresentation";

const CatalogueEntry = observer(function CatalogueEntry({
  item,
  busy,
}: {
  readonly item: CatalogueItemDto;
  readonly busy: boolean;
}) {
  const { downloads: store } = useStore();
  return (
    <CatalogueRow
      item={item}
      selected={store.selectedRef === item.ref}
      busy={busy}
      removable={store.isRemovable(item)}
      onSelect={() => store.select(item.ref)}
      onDownload={() => void store.install(item)}
      onRemove={() => void store.uninstall(item)}
    />
  );
});

export default observer(function DownloadsWorkspace() {
  const { downloads: store } = useStore();
  useEffect(() => {
    void store.mount();
    return store.dispose;
  }, [store]);

  const busy = store.busy;
  const disk = store.disk;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <PageHeader
        icon={mdiTrayArrowDown}
        title="Загрузки"
        subtitle={
          <>
            {store.runningCount} загружается · {store.queued.length} в очереди
            {disk === null
              ? ""
              : ` · занято ${formatBytes(disk.usedBytes)} из ${formatBytes(disk.totalBytes)}`}
          </>
        }
        actions={
          <>
            <Button
              tone="ghost"
              disabled={busy || store.runningCount === 0}
              onClick={() => {
                void store.pauseAll();
              }}
            >
              <Icon path={mdiPause} size={14} />
              Приостановить всё
            </Button>
            <Button
              tone="secondary"
              disabled={store.refreshing}
              onClick={() => {
                void store.refresh();
              }}
            >
              <Icon path={mdiRefresh} size={14} />
              Обновить каталог
            </Button>
          </>
        }
      />

      <div className="flex min-h-0 flex-1">
        <DownloadFilterRail store={store} />

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
            <div className="flex items-center gap-2.5 border-b border-line bg-main-850 px-5 py-2">
              <span className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                Выполняется
              </span>
              <Chip>
                <span className="font-mono">↓ {formatRate(store.totalRate)}</span>
              </Chip>
              <Chip>
                {store.runningCount} из {store.concurrency} слотов
              </Chip>
            </div>

            {store.active.length === 0 ? (
              <p className="border-b border-line p-4  text-[12px] text-main-500">
                Сейчас ничего не загружается. Выберите артефакт в каталоге ниже.
              </p>
            ) : (
              store.active.map((download) => (
                <DownloadProgressRow
                  key={download.id}
                  download={download}
                  rate={store.rateOf(download)}
                  etaMs={store.etaOf(download)}
                  selected={store.isSelected(download)}
                  busy={busy}
                  onSelect={() => store.selectDownload(download)}
                  onPause={() => void store.pause(download.id)}
                  onResume={() => void store.resume(download.id)}
                  onCancel={() => void store.cancel(download.id)}
                />
              ))
            )}

            {store.queued.map((download, index) => (
              <DownloadQueueRow
                key={download.id}
                download={download}
                position={index + 1}
                selected={store.isSelected(download)}
                busy={busy}
                onSelect={() => store.selectDownload(download)}
                onRaise={() => void store.nudge(download.id, 1)}
                onLower={() => void store.nudge(download.id, -1)}
                onCancel={() => void store.cancel(download.id)}
              />
            ))}

            {store.failed.length === 0 ? null : (
              <>
                <div className="flex items-center gap-2.5 border-y border-line bg-main-850 px-5 py-2">
                  <span className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-err uppercase">
                    Не удалось
                  </span>
                  <Chip>{store.failed.length}</Chip>
                </div>
                {store.failed.map((download) => (
                  <DownloadFailureRow
                    key={download.id}
                    download={download}
                    selected={store.isSelected(download)}
                    busy={busy}
                    onSelect={() => store.selectDownload(download)}
                    onRetry={() => void store.resume(download.id)}
                    onDismiss={() => void store.remove(download.id)}
                  />
                ))}
              </>
            )}

            <div className="flex items-center gap-2.5 border-y border-line bg-main-850 px-5 py-2">
              <span className="flex-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                Каталог
              </span>
              <div className="w-45">
                <TextInput
                  preset="search"
                  aria-label="Фильтр каталога"
                  placeholder="Фильтр…"
                  value={store.query}
                  onChange={(event) => store.setQuery(event.currentTarget.value)}
                />
              </div>
            </div>

            {store.catalogueError !== null ? (
              <div className="flex flex-1 items-center justify-center p-6">
                <EmptyState
                  icon={mdiTrayArrowDown}
                  title="Каталог недоступен"
                  description={store.catalogueError}
                  action={
                    <Button
                      tone="secondary"
                      disabled={store.refreshing}
                      onClick={() => {
                        void store.refresh();
                      }}
                    >
                      Повторить
                    </Button>
                  }
                />
              </div>
            ) : store.loaded && store.visible.length === 0 ? (
              <div className="flex flex-1 items-center justify-center p-6">
                <EmptyState
                  icon={mdiTrayArrowDown}
                  title="Ничего не найдено"
                  description="Под выбранные фильтры не подходит ни один артефакт. Сбросьте фильтры или обновите каталог."
                />
              </div>
            ) : store.kind === "all" ? (
              store.sections.map(({ section, items }) => (
                <section key={section} aria-label={SECTION_LABELS[section]}>
                  <div className="flex items-center gap-2.5 border-b border-line px-5 pt-3.5 pb-2">
                    <span className="flex-1 text-[11.5px] font-semibold text-main-300">
                      {SECTION_LABELS[section]}
                    </span>
                    <Chip>{items.length}</Chip>
                  </div>
                  {items.map((item) => (
                    <CatalogueEntry key={item.ref} item={item} busy={busy} />
                  ))}
                </section>
              ))
            ) : (
              store.visible.map((item) => <CatalogueEntry key={item.ref} item={item} busy={busy} />)
            )}
          </ScrollArea>
        </div>

        <DownloadDetailRail store={store} />
      </div>
    </div>
  );
});
