import { mdiDeleteOutline } from "@mdi/js";
import type { CatalogueItemDto } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT } from "../../ui/atoms/statusTone";
import {
  formatBytes,
  KIND_ICONS,
  KIND_LABELS,
  removalPrompt,
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
      onClick={onSelect}
      className={`flex items-center gap-3 border-b border-line px-5 py-3 ${
        selected
          ? "bg-main-750 shadow-[inset_2px_0_0_var(--color-accent-dark)]"
          : "hover:bg-main-800"
      }`}
    >
      <button
        type="button"
        aria-current={selected ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <span className="flex size-9 flex-none items-center justify-center rounded-control bg-main-700 text-main-300">
          <Icon path={KIND_ICONS[item.kind]} size={16} />
        </span>
        <span className="flex min-w-0 flex-1 flex-col gap-0.75">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate font-mono text-[13px] font-medium text-main-100">
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
            className={`truncate font-mono text-[11px] ${updatable ? "text-warn" : "text-main-500"}`}
          >
            {subline}
          </span>
          {blocked === undefined ? null : (
            <span className="truncate text-[11px] text-err" title={blocked}>
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
        <IconButton
          tone="danger"
          size="md"
          path={mdiDeleteOutline}
          label={removable ? "Удалить" : "Файл установлен не через этот каталог"}
          disabled={busy || !removable}
          needConfirm
          modalSetup={{
            title: `Удалить «${item.displayName}»?`,
            content: removalPrompt(item),
            confirmLabel: "Удалить",
          }}
          onClick={onRemove}
        />
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
