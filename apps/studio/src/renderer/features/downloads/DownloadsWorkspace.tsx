import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import { mdiPause, mdiRefresh, mdiTrayArrowDown } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { useStore } from "../../stores/useStore";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import EmptyState from "../../ui/molecules/EmptyState";
import CatalogueRow from "./CatalogueRow";
import DownloadDetailRail from "./DownloadDetailRail";
import DownloadFailureRow from "./DownloadFailureRow";
import DownloadFilterRail from "./DownloadFilterRail";
import DownloadProgressRow from "./DownloadProgressRow";
import DownloadQueueRow from "./DownloadQueueRow";
import { formatBytes, formatRate } from "./downloadPresentation";

export default observer(function DownloadsWorkspace() {
  const { downloads: store } = useStore();
  useEffect(() => {
    void store.mount();
    return store.dispose;
  }, [store]);

  const busy = store.busyId !== null;
  const disk = store.disk;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex h-header flex-none items-center gap-3 border-b border-main-750 bg-main-900 px-5">
        <Icon path={mdiTrayArrowDown} size={20} className="flex-none text-accent-dark" />
        <h1 className="m-0 flex-none text-xl/7 font-semibold tracking-[-0.01em] text-main-50">
          Загрузки
        </h1>
        <span className="ml-1.5 truncate text-xs/4 text-main-400">
          {store.runningCount} загружается · {store.queued.length} в очереди
          {disk === null
            ? ""
            : ` · занято ${formatBytes(disk.usedBytes)} из ${formatBytes(disk.totalBytes)}`}
        </span>
        <div className="flex-1" />
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
      </header>

      <div className="flex min-h-0 flex-1">
        <DownloadFilterRail store={store} />

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {store.error === null ? null : (
            <div
              role="alert"
              className="flex items-center gap-3 border-b border-err-border px-4 py-2.5 text-xs text-err"
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
            <div className="flex items-center gap-2.5 border-b border-main-750 bg-main-900/40 px-4 py-2.5">
              <span className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
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
              <p className="border-b border-main-750 p-4  text-[11.5px] text-main-500">
                Сейчас ничего не загружается. Выберите артефакт в каталоге ниже.
              </p>
            ) : (
              store.active.map((download) => (
                <DownloadProgressRow
                  key={download.id}
                  download={download}
                  rate={store.rateOf(download)}
                  etaMs={store.etaOf(download)}
                  selected={store.selectedRef === download.itemRef}
                  busy={busy}
                  onSelect={() => store.select(download.itemRef)}
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
                selected={store.selectedRef === download.itemRef}
                busy={busy}
                onSelect={() => store.select(download.itemRef)}
                onRaise={() => void store.nudge(download.id, 1)}
                onLower={() => void store.nudge(download.id, -1)}
                onCancel={() => void store.cancel(download.id)}
              />
            ))}

            {store.failed.length === 0 ? null : (
              <>
                <div className="flex items-center gap-2.5 border-y border-main-750 bg-main-900/40 px-4 py-2.5">
                  <span className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-err uppercase">
                    Не удалось
                  </span>
                  <Chip>{store.failed.length}</Chip>
                </div>
                {store.failed.map((download) => (
                  <DownloadFailureRow
                    key={download.id}
                    download={download}
                    selected={store.selectedRef === download.itemRef}
                    busy={busy}
                    onSelect={() => store.select(download.itemRef)}
                    onRetry={() => void store.resume(download.id)}
                    onDismiss={() => void store.remove(download.id)}
                  />
                ))}
              </>
            )}

            <div className="flex items-center gap-2.5 border-y border-main-750 bg-main-900/40 px-4 py-2.5">
              <span className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
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
            ) : (
              store.visible.map((item) => (
                <CatalogueRow
                  key={item.ref}
                  item={item}
                  selected={store.selectedRef === item.ref}
                  busy={busy}
                  removable={store.installedId(item.ref) !== undefined}
                  onSelect={() => store.select(item.ref)}
                  onDownload={() => void store.start(item.ref)}
                  onRemove={() => {
                    const id = store.installedId(item.ref);
                    if (id !== undefined) void store.remove(id);
                  }}
                />
              ))
            )}
          </ScrollArea>
        </div>

        <DownloadDetailRail store={store} />
      </div>
    </div>
  );
});
