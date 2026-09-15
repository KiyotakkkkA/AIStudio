import { mdiAlertCircleOutline, mdiClose, mdiRefresh } from "@mdi/js";
import type { DownloadDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import { formatBytes } from "./downloadPresentation";

export interface DownloadFailureRowProps {
  readonly download: DownloadDto;
  readonly selected: boolean;
  readonly busy: boolean;
  readonly onSelect: () => void;
  readonly onRetry: () => void;
  readonly onDismiss: () => void;
}

export default function DownloadFailureRow({
  download,
  selected,
  busy,
  onSelect,
  onRetry,
  onDismiss,
}: DownloadFailureRowProps) {
  return (
    <div
      className={`flex items-center gap-3 border-b border-main-750 px-4 py-2.75 ${
        selected ? "bg-main-750 shadow-[inset_2px_0_0_var(--color-err)]" : "hover:bg-main-800"
      }`}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-current={selected ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span className="flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 text-err">
          <Icon path={mdiAlertCircleOutline} size={16} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.75">
          <span className="truncate font-mono text-[12.5px] font-medium text-main-100">
            {download.displayName}
          </span>
          <span className="truncate text-[10.5px] text-err" title={download.error}>
            {download.error ?? "Загрузка прервалась"}
          </span>
          <span className="font-mono text-[10.5px] text-main-500">
            получено {formatBytes(download.bytesDone)} из {formatBytes(download.sizeBytes)}
          </span>
        </span>
      </button>
      <Button tone="secondary" disabled={busy} onClick={onRetry}>
        <Icon path={mdiRefresh} size={14} />
        Повторить
      </Button>
      <IconButton
        path={mdiClose}
        aria-label={`Убрать запись о ${download.displayName}`}
        title="Убрать запись"
        disabled={busy}
        onClick={onDismiss}
      />
    </div>
  );
}
