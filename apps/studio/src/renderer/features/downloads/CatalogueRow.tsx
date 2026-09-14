import type { CatalogueItemDto } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  formatBytes,
  KIND_ICONS,
  KIND_LABELS,
  STATE_LABELS,
  STATE_TONES,
} from "./downloadPresentation";

export interface CatalogueRowProps {
  readonly item: CatalogueItemDto;
  readonly selected: boolean;
  readonly busy: boolean;
  readonly removable: boolean;
  readonly onSelect: () => void;
  readonly onDownload: () => void;
  readonly onRemove: () => void;
}

export default function CatalogueRow({
  item,
  selected,
  busy,
  removable,
  onSelect,
  onDownload,
  onRemove,
}: CatalogueRowProps) {
  const tone = STATE_TONES[item.state];
  const blocked = item.blockedReason;
  const installed = item.state === "installed";
  const updatable = item.state === "update";
  const inFlight = item.state === "downloading" || item.state === "queued";
  const subline = [
    formatBytes(item.sizeBytes),
    item.dimension === undefined ? undefined : `${String(item.dimension)} dim`,
    updatable && item.installedVersion !== undefined
      ? `установлено ${item.installedVersion} · доступно ${item.version ?? "новее"}`
      : undefined,
  ]
    .filter((part) => part !== undefined)
    .join(" · ");

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
        <span className="flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 text-main-300">
          <Icon path={KIND_ICONS[item.kind]} size={16} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.75">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate font-mono text-[12.5px] font-medium text-main-100">
              {item.displayName}
            </span>
            <Chip>{KIND_LABELS[item.kind]}</Chip>
            {item.version === undefined ? null : (
              <Chip>
                <span className="font-mono">{item.version}</span>
              </Chip>
            )}
          </span>
          <span
            className={`truncate font-mono text-[10.5px] ${updatable ? "text-warn" : "text-main-500"}`}
          >
            {subline}
          </span>
          {blocked === undefined ? null : (
            <span className="truncate text-[10.5px] text-err" title={blocked}>
              {blocked}
            </span>
          )}
        </span>
      </button>
      <Chip title={STATE_LABELS[item.state]}>
        <StatusDot tone={tone} />
        <span className={STATUS_TONE_TEXT[tone]}>{STATE_LABELS[item.state]}</span>
      </Chip>
      {inFlight ? null : installed ? (
        <Button
          tone="danger"
          disabled={busy || !removable}
          title={removable ? undefined : "Файл установлен не через этот каталог"}
          needConfirm
          modalSetup={{
            title: `Удалить «${item.displayName}»?`,
            content: "Файл будет удалён с диска. Скачать его снова можно в любой момент.",
            confirmLabel: "Удалить",
          }}
          onClick={onRemove}
        >
          Удалить
        </Button>
      ) : (
        <Button
          tone="secondary"
          disabled={busy || !item.downloadable}
          title={blocked}
          onClick={onDownload}
        >
          {updatable ? "Обновить" : "Скачать"}
        </Button>
      )}
    </div>
  );
}
