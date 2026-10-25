import { observer } from "mobx-react-lite";
import { mdiClose, mdiDeleteOutline, mdiTrayArrowDown } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
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
  removalPrompt,
  shortChecksum,
  STATE_LABELS,
  STATUS_LABELS,
} from "./downloadPresentation";
import { isToolchainRef } from "../toolchain/toolchainPresentation";
import Alert from "../../ui/molecules/Alert";

export default observer(function DownloadDetailRail({ store }: { readonly store: DownloadStore }) {
  const item = store.selectedItem;
  const download = store.selectedDownload;

  if (item === null && download === null) {
    return (
      <aside
        aria-label="Детали загрузки"
        className="flex w-78.5 flex-none flex-col border-l border-line bg-main-800"
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
  const installed = item?.state === "installed" || item?.state === "update";
  const settled =
    download === null || download.status === "succeeded" || download.status === "cancelled";
  const size = download?.sizeBytes ?? item?.sizeBytes ?? 0;
  const done = download?.bytesDone ?? (installed ? size : 0);
  const stateLabel =
    item !== null && settled
      ? STATE_LABELS[item.state]
      : download === null
        ? "не скачивалось"
        : STATUS_LABELS[download.status];
  const percent = percentOf(done, size);
  const checksum = item?.checksum ?? download?.checksum;
  const options = store.optionsFor(ref);
  const running = download?.status === "running";
  const pausable = running || download?.status === "queued";
  const busy = store.busy;
  const removable = item !== null && installed && store.isRemovable(item);

  return (
    <aside
      aria-label="Детали загрузки"
      className="flex w-78.5 flex-none flex-col border-l border-line bg-main-800"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
        {item?.kind === "runtime" && isToolchainRef(item.ref) && store.runtimeError !== null ? (
          <span role="alert" className="text-[11px] text-err">
            {store.runtimeError}
          </span>
        ) : null}
        {item?.kind === "runtime" && !isToolchainRef(item.ref) && store.runtimes !== null ? (
          <div className="flex flex-col gap-1 rounded-card bg-main-900 shadow-card px-3 py-2.5">
            <span className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
              Эта машина
            </span>
            <p className="m-0 text-xs/normal  text-main-300">{store.runtimes.plan.reason}</p>
            {store.runtimes.plan.gpu === null ? null : (
              <span className="font-mono text-[11px] text-main-500">
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
            <p className="m-0 text-xs/normal  text-main-400">{item.description}</p>
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

        <dl className="m-0 flex flex-col gap-2.25 rounded-card bg-main-900 shadow-card p-3 text-[12px]">
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Загружено</dt>
            <dd className="m-0 font-mono">
              {formatBytes(done)} / {formatBytes(size)}
            </dd>
          </div>
          <div className="h-1.25 rounded-full bg-main-700">
            <span
              className="block h-1.25 rounded-full bg-accent-dark"
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
            <dd className="m-0 font-mono">{stateLabel}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt className="text-main-400">Докачка</dt>
            <dd className="m-0 font-mono text-ok">поддерживается</dd>
          </div>
        </dl>

        {download?.error === undefined ? null : <Alert>{download.error}</Alert>}

        <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          Назначение
        </p>
        <div className="flex flex-col gap-1.5 rounded-card bg-main-900 shadow-card p-2.75">
          <span className="font-mono text-[11px] break-all text-main-300">
            {download?.targetPath ?? store.disk?.root ?? "будет выбран при запуске"}
          </span>
          <span className="font-mono text-[11px] text-main-500">
            {checksum === undefined
              ? "контрольная сумма не опубликована"
              : shortChecksum(checksum.algorithm, checksum.value)}
          </span>
        </div>

        <p className="m-0 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
          После загрузки
        </p>
        <div className="flex flex-col gap-2.25 rounded-card bg-main-900 shadow-card p-2.75">
          <Toggle
            checked={checksum !== undefined && options.verifyChecksum}
            disabled={checksum === undefined}
            label="Проверить контрольную сумму"
            onChange={(checked) => void store.setOption(ref, { verifyChecksum: checked })}
          />
        </div>

        <div className="flex-1" />
        <div className="flex gap-2">
          {download !== null && pausable ? null : item?.state === "installed" ? null : (
            <Button
              tone="primary"
              disabled={busy || item === null || !item.downloadable}
              title={item?.blockedReason}
              onClick={() => {
                if (item !== null) void store.install(item);
              }}
            >
              {item?.state === "update" ? "Обновить" : "Скачать"}
            </Button>
          )}
          {item === null || !removable || (download !== null && pausable) ? null : (
            <IconButton
              tone="danger"
              size="md"
              path={mdiDeleteOutline}
              label="Удалить"
              disabled={busy}
              needConfirm
              modalSetup={{
                title: `Удалить «${item.displayName}»?`,
                content: removalPrompt(item),
                confirmLabel: "Удалить",
              }}
              onClick={() => void store.uninstall(item)}
            />
          )}
          {download === null || !pausable ? null : (
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
            <IconButton
              tone="danger"
              size="md"
              path={mdiClose}
              label="Отменить"
              disabled={busy}
              onClick={() => {
                void store.cancel(download.id);
              }}
            />
          )}
        </div>
      </ScrollArea>
    </aside>
  );
});
