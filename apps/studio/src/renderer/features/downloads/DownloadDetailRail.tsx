import { observer } from "mobx-react-lite";
import { mdiTrayArrowDown } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Toggle from "../../ui/atoms/Toggle";
import EmptyState from "../../ui/molecules/EmptyState";
import type DownloadStore from "./DownloadStore";
import {
  formatBytes,
  formatEta,
  formatRate,
  KIND_LABELS,
  percentOf,
  shortChecksum,
  STATUS_LABELS,
} from "./downloadPresentation";

export default observer(function DownloadDetailRail({ store }: { readonly store: DownloadStore }) {
  const item = store.selectedItem;
  const download = store.selectedDownload;

  if (item === null && download === null) {
    return (
      <aside
        aria-label="Детали загрузки"
        className="flex w-78.5 flex-none flex-col border-l border-main-750 bg-main-900"
      >
        <div className="flex flex-1 items-center justify-center p-4">
          <EmptyState
            icon={mdiTrayArrowDown}
            title="Ничего не выбрано"
            description="Выберите строку, чтобы увидеть описание, путь назначения, контрольную сумму и действия после загрузки."
          />
        </div>
      </aside>
    );
  }

  const ref = item?.ref ?? download?.itemRef ?? "";
  const title = item?.displayName ?? download?.displayName ?? "";
  const size = download?.sizeBytes ?? item?.sizeBytes ?? 0;
  const done = download?.bytesDone ?? 0;
  const percent = percentOf(done, size);
  const checksum = item?.checksum ?? download?.checksum;
  const options = store.optionsFor(ref);
  const running = download?.status === "running";
  const pausable = running || download?.status === "queued";
  const busy = store.busyId !== null;

  return (
    <aside
      aria-label="Детали загрузки"
      className="flex w-78.5 flex-none flex-col border-l border-main-750 bg-main-900"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 p-3.5">
        {item?.kind === "runtime" && store.runtimes !== null ? (
          <div className="flex flex-col gap-1 rounded-card border border-main-750 bg-main-800 px-3 py-2.5">
            <span className="text-[11px] font-semibold tracking-[0.06em] text-main-400 uppercase">
              Эта машина
            </span>
            <p className="m-0 text-[11.5px] leading-normal text-main-300">
              {store.runtimes.plan.reason}
            </p>
            {store.runtimes.plan.gpu === null ? null : (
              <span className="font-mono text-[10.5px] text-main-500">
                {store.runtimes.plan.gpu.vendor} {store.runtimes.plan.gpu.model}
                {store.runtimes.plan.gpu.vramBytes === null
                  ? ""
                  : ` · ${formatBytes(store.runtimes.plan.gpu.vramBytes)} видеопамяти`}
              </span>
            )}
            {store.runtimeError === null ? null : (
              <span role="alert" className="text-[11px] text-err">
                {store.runtimeError}
              </span>
            )}
          </div>
        ) : null}
        <div className="flex flex-col gap-1">
          <h2 className="m-0 font-mono text-[13.5px] font-medium text-main-50">{title}</h2>
          {item === null ? null : (
            <p className="m-0 text-[11.5px] leading-normal text-main-400">{item.description}</p>
          )}
        </div>

        <div className="flex flex-wrap gap-1.25">
          {item === null ? null : <Chip>{KIND_LABELS[item.kind]}</Chip>}
          {(item?.tags ?? []).map((tag) => (
            <Chip key={tag}>
              <span className="font-mono">{tag}</span>
            </Chip>
          ))}
          {item?.dimension === undefined ? null : (
            <Chip>
              <span className="font-mono">{item.dimension} dim</span>
            </Chip>
          )}
        </div>

        <dl className="m-0 flex flex-col gap-2.25 rounded-card border border-main-750 bg-main-800 p-3 text-[11.5px]">
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Загружено</dt>
            <dd className="m-0 font-mono">
              {formatBytes(done)} / {formatBytes(size)}
            </dd>
          </div>
          <div className="h-1.25 rounded-sm bg-main-700">
            <span
              className="block h-1.25 rounded-sm bg-accent-dark"
              style={{ width: `${String(percent)}%` }}
            />
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Скорость</dt>
            <dd className="m-0 font-mono">
              {download === null ? "—" : formatRate(store.rateOf(download))}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Осталось</dt>
            <dd className="m-0 font-mono">
              {download === null ? "—" : formatEta(store.etaOf(download))}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Состояние</dt>
            <dd className="m-0 font-mono">
              {download === null ? "не скачивалось" : STATUS_LABELS[download.status]}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Докачка</dt>
            <dd className="m-0 font-mono text-ok">поддерживается</dd>
          </div>
        </dl>

        {download?.error === undefined ? null : (
          <p
            role="alert"
            className="m-0 rounded-card border border-err-border bg-main-800 p-2.75 text-[11.5px] text-err"
          >
            {download.error}
          </p>
        )}

        <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Назначение
        </p>
        <div className="flex flex-col gap-1.5 rounded-card border border-main-750 bg-main-800 p-2.75">
          <span className="font-mono text-[11px] break-all text-main-300">
            {download?.targetPath ?? store.disk?.root ?? "будет выбран при запуске"}
          </span>
          <span className="font-mono text-[10.5px] text-main-500">
            {checksum === undefined
              ? "контрольная сумма не опубликована"
              : shortChecksum(checksum.algorithm, checksum.value)}
          </span>
        </div>

        <p className="m-0 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          После загрузки
        </p>
        <div className="flex flex-col gap-2.25 rounded-card border border-main-750 bg-main-800 p-2.75">
          <Toggle
            checked={checksum !== undefined && options.verifyChecksum}
            disabled={checksum === undefined}
            label="Проверить контрольную сумму"
            onChange={(checked) => void store.setOption(ref, { verifyChecksum: checked })}
          />
        </div>

        <div className="flex-1" />
        <div className="flex gap-2">
          {download === null || !pausable ? (
            <Button
              tone="primary"
              disabled={busy || item === null || !item.downloadable}
              title={item?.blockedReason}
              onClick={() => {
                if (item !== null) void store.start(item.ref);
              }}
            >
              {item?.state === "update" ? "Обновить" : "Скачать"}
            </Button>
          ) : (
            <Button
              tone="secondary"
              disabled={busy}
              onClick={() => {
                if (download !== null) void store.pause(download.id);
              }}
            >
              Приостановить
            </Button>
          )}
          {download?.status === "paused" || download?.status === "failed" ? (
            <Button
              tone="secondary"
              disabled={busy}
              onClick={() => {
                void store.resume(download.id);
              }}
            >
              Продолжить
            </Button>
          ) : null}
          {download === null || download.status === "succeeded" ? null : (
            <Button
              tone="danger"
              disabled={busy}
              onClick={() => {
                void store.cancel(download.id);
              }}
            >
              Отменить
            </Button>
          )}
        </div>
      </ScrollArea>
    </aside>
  );
});
