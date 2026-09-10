import Chip from "../../ui/atoms/Chip";
import type { ModelRow } from "./modelRows";
import { contextWindowLabel, MODEL_CAPABILITY_LABELS, sizeLabel } from "./providerPresentation";

export interface ModelCardProps {
  readonly row: ModelRow;
  readonly providerName: string;
  readonly isDefault: boolean;
  readonly selectable: boolean;
  readonly onMakeDefault: () => void;
}

export default function ModelCard({
  row,
  providerName,
  isDefault,
  selectable,
  onMakeDefault,
}: ModelCardProps) {
  const meta = [providerName, row.sizeBytes === null ? null : sizeLabel(row.sizeBytes)]
    .filter((part) => part !== null)
    .join(" · ");

  return (
    <article
      className={`flex flex-col gap-2 rounded-card border p-3 ${
        isDefault ? "border-accent-dark bg-main-750" : "border-main-700 bg-main-800"
      } ${row.available ? "" : "opacity-55"}`}
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
        {row.available && isDefault ? <Chip tone="selected">По умолчанию</Chip> : null}
        {row.available && !isDefault ? (
          <input
            type="radio"
            name="provider-default-model"
            aria-label={`Модель по умолчанию: ${row.externalId}`}
            checked={isDefault}
            disabled={!selectable}
            title={
              selectable ? undefined : "Сохраните подключение, чтобы выбрать модель по умолчанию."
            }
            className="mt-0.5 size-3.75 flex-none accent-accent-dark disabled:cursor-not-allowed disabled:opacity-40"
            onChange={onMakeDefault}
          />
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
