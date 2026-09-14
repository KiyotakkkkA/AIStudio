import { InputCheckBox } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../../ui/atoms/Chip";
import type { ModelRow } from "./modelRows";
import { contextWindowLabel, MODEL_CAPABILITY_LABELS, sizeLabel } from "./providerPresentation";

export interface ModelCardProps {
  readonly row: ModelRow;
  readonly providerName: string;
  readonly selected: boolean;
  readonly selectable: boolean;
  readonly onToggle: () => void;
}

export default function ModelCard({
  row,
  providerName,
  selected,
  selectable,
  onToggle,
}: ModelCardProps) {
  const meta = [providerName, row.sizeBytes === null ? null : sizeLabel(row.sizeBytes)]
    .filter((part) => part !== null)
    .join(" · ");

  return (
    <article
      className={`flex flex-col gap-2 rounded-card border p-3 ${
        selected ? "border-accent-dark bg-main-750" : "border-main-750 bg-main-800"
      } ${row.available ? "" : "opacity-55"}`}
      onClick={selectable ? onToggle : undefined}
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <div
            className="truncate font-mono text-[12.5px] font-medium text-main-50"
            title={row.externalId}
          >
            {row.externalId}
          </div>
          <div className={`truncate text-[11px] ${row.available ? "text-main-400" : "text-err"}`}>
            {row.available ? meta : (row.unavailableReason ?? "Недоступна")}
          </div>
        </div>
        {row.available || selected ? (
          <span onClick={(event) => event.stopPropagation()}>
            <InputCheckBox
              aria-label={`Использовать модель: ${row.externalId}`}
              checked={selected}
              disabled={!selectable || (!row.available && !selected)}
              onChange={onToggle}
            />
          </span>
        ) : null}
      </div>

      <div className="flex flex-wrap gap-1.25">
        {row.contextWindow === null ? null : <Chip>{contextWindowLabel(row.contextWindow)}</Chip>}
        {row.capabilities.map((capability) => (
          <Chip key={capability}>{MODEL_CAPABILITY_LABELS[capability]}</Chip>
        ))}
      </div>
    </article>
  );
}
