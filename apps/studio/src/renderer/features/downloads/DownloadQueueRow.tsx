import { mdiArrowDown, mdiArrowUp, mdiClose } from "@mdi/js";
import type { DownloadDto } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/IconButton";
import { formatBytes, KIND_ICONS, KIND_LABELS } from "./downloadPresentation";

export interface DownloadQueueRowProps {
  readonly download: DownloadDto;
  readonly position: number;
  readonly selected: boolean;
  readonly busy: boolean;
  readonly onSelect: () => void;
  readonly onRaise: () => void;
  readonly onLower: () => void;
  readonly onCancel: () => void;
}

export default function DownloadQueueRow({
  download,
  position,
  selected,
  busy,
  onSelect,
  onRaise,
  onLower,
  onCancel,
}: DownloadQueueRowProps) {
  return (
    <div
      className={`flex items-center gap-3 border-b border-main-750 px-4 py-2.75 ${
        selected
          ? "bg-main-750 shadow-[inset_2px_0_0_var(--color-accent-dark)]"
          : "hover:bg-main-800"
      }`}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-current={selected ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span className="flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 text-main-400">
          <Icon path={KIND_ICONS[download.itemKind]} size={16} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.75">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate font-mono text-[12.5px] font-medium text-main-200">
              {download.displayName}
            </span>
            <Chip>{KIND_LABELS[download.itemKind]}</Chip>
          </span>
          <span className="font-mono text-[10.5px] text-main-500">
            {formatBytes(download.sizeBytes)} · ждёт свободного слота · приоритет{" "}
            {download.priority}
          </span>
        </span>
      </button>
      <span className="w-10 flex-none text-right font-mono text-[11px] text-main-500">
        №{position}
      </span>
      <Chip>в очереди</Chip>
      <IconButton
        path={mdiArrowUp}
        aria-label={`Поднять ${download.displayName} в очереди`}
        title="Выше в очереди"
        disabled={busy || download.priority >= 9}
        onClick={onRaise}
      />
      <IconButton
        path={mdiArrowDown}
        aria-label={`Опустить ${download.displayName} в очереди`}
        title="Ниже в очереди"
        disabled={busy || download.priority <= 0}
        onClick={onLower}
      />
      <IconButton
        path={mdiClose}
        aria-label={`Убрать ${download.displayName} из очереди`}
        title="Убрать из очереди"
        disabled={busy}
        onClick={onCancel}
      />
    </div>
  );
}
