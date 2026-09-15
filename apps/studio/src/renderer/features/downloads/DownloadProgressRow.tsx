import { mdiClose, mdiPause, mdiPlay } from "@mdi/js";
import type { DownloadDto } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  formatBytes,
  formatEta,
  formatRate,
  KIND_ICONS,
  KIND_LABELS,
  percentOf,
  STATUS_LABELS,
  STATUS_TONES,
} from "./downloadPresentation";

export interface DownloadProgressRowProps {
  readonly download: DownloadDto;
  readonly rate: number;
  readonly etaMs: number | undefined;
  readonly selected: boolean;
  readonly busy: boolean;
  readonly onSelect: () => void;
  readonly onPause: () => void;
  readonly onResume: () => void;
  readonly onCancel: () => void;
}

export default function DownloadProgressRow({
  download,
  rate,
  etaMs,
  selected,
  busy,
  onSelect,
  onPause,
  onResume,
  onCancel,
}: DownloadProgressRowProps) {
  const percent = percentOf(download.bytesDone, download.sizeBytes);
  const running = download.status === "running";
  const tone = STATUS_TONES[download.status];
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
        <span
          className={`flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 ${STATUS_TONE_TEXT[tone]}`}
        >
          <Icon path={KIND_ICONS[download.itemKind]} size={16} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.75">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate font-mono text-[12.5px] font-medium text-main-100">
              {download.displayName}
            </span>
            <Chip>{KIND_LABELS[download.itemKind]}</Chip>
            {download.version === undefined ? null : (
              <Chip>
                <span className="font-mono">{download.version}</span>
              </Chip>
            )}
            {running ? null : <Chip tone="accent">{STATUS_LABELS[download.status]}</Chip>}
          </span>
          <span className="flex min-w-0 items-center gap-2.25">
            <span className="flex-none font-mono text-[10.5px] text-main-300">
              {formatBytes(download.bytesDone)} из {formatBytes(download.sizeBytes)}
            </span>
            <span className="flex-none font-mono text-[10.5px] text-main-500">
              {running
                ? `${formatRate(rate)}${etaMs === undefined ? "" : ` · осталось ${formatEta(etaMs)}`}`
                : "докачается с того же места"}
            </span>
          </span>
          <span className="mt-0.75 block h-0.75 w-full rounded-sm bg-main-700">
            <span
              className={`block h-0.75 rounded-sm ${running ? "bg-accent-dark" : "bg-warn"}`}
              style={{ width: `${String(percent)}%` }}
            />
          </span>
        </span>
      </button>
      <span className="w-10 flex-none text-right font-mono text-[12px] text-accent-medium">
        {percent}%
      </span>
      <IconButton
        path={running ? mdiPause : mdiPlay}
        aria-label={`${running ? "Приостановить" : "Продолжить"} загрузку ${download.displayName}`}
        title={running ? "Приостановить" : "Продолжить"}
        disabled={busy}
        onClick={running ? onPause : onResume}
      />
      <IconButton
        path={mdiClose}
        aria-label={`Отменить загрузку ${download.displayName}`}
        title="Отменить"
        disabled={busy}
        onClick={onCancel}
      />
    </div>
  );
}
