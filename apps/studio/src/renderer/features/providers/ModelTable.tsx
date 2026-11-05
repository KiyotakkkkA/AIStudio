import { mdiArrowDown, mdiArrowUp, mdiUnfoldMoreHorizontal } from "@mdi/js";
import { InputCheckBox, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import type { ModelRow } from "./modelRows";
import type { ModelSortDirection, ModelTableSort } from "./modelTableModel";
import { MODEL_CAPABILITY_LABELS, sizeLabel } from "./providerPresentation";

export default function ModelTable({
  rows,
  selectedIds,
  selectable,
  sort,
  direction,
  onSort,
  onToggle,
}: {
  readonly rows: readonly ModelRow[];
  readonly selectedIds: readonly string[];
  readonly selectable: boolean;
  readonly sort: ModelTableSort;
  readonly direction: ModelSortDirection;
  readonly onSort: (column: ModelTableSort) => void;
  readonly onToggle: (externalId: string) => void;
}) {
  const selected = new Set(selectedIds);
  const heading = (column: ModelTableSort, label: string, alignRight = false) => (
    <th
      scope="col"
      aria-sort={sort === column ? direction : "none"}
      className="px-3 py-2.5 font-medium"
    >
      <button
        type="button"
        onClick={() => onSort(column)}
        className={`flex w-full items-center gap-1.5 rounded-control text-main-400 hover:text-main-50 ${alignRight ? "justify-end" : ""}`}
      >
        {label}
        <Icon
          path={
            sort === column
              ? direction === "ascending"
                ? mdiArrowUp
                : mdiArrowDown
              : mdiUnfoldMoreHorizontal
          }
          size={13}
          className={sort === column ? "text-accent-medium" : "text-main-500"}
        />
      </button>
    </th>
  );
  return (
    <ScrollArea className="max-h-120 min-w-0 rounded-control bg-main-850">
      <table
        aria-label="Модели провайдера"
        className="w-full min-w-180 border-collapse text-left text-[12px]"
      >
        <thead className="sticky top-0 z-10 bg-main-850 text-[11px] shadow-[0_1px_0_var(--color-line)]">
          <tr>
            <th scope="col" className="w-12 px-3 py-2.5">
              <span className="sr-only">Выбор модели</span>
            </th>
            {heading("name", "Модель")}
            {heading("size", "Размер", true)}
            {heading("context", "Контекст, токены", true)}
            <th scope="col" className="px-3 py-2.5 font-medium text-main-400">
              Возможности
            </th>
            {heading("availability", "Доступность")}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const checked = selected.has(row.externalId);
            return (
              <tr
                key={row.key}
                className={`transition-colors ${checked ? "bg-accent-soft/50" : "even:bg-main-800/25 hover:bg-main-800/60"}`}
              >
                <td
                  className={`px-3 py-3 ${checked ? "shadow-[inset_3px_0_0_var(--color-accent-dark)]" : ""}`}
                >
                  <InputCheckBox
                    aria-label={`Использовать модель: ${row.externalId}`}
                    checked={checked}
                    disabled={!selectable || (!row.available && !checked)}
                    onChange={() => onToggle(row.externalId)}
                  />
                </td>
                <th scope="row" className="max-w-100 px-3 py-3 font-normal">
                  <div
                    className={`truncate font-mono font-medium ${checked ? "text-accent-medium" : "text-main-100"}`}
                    title={row.externalId}
                  >
                    {row.externalId}
                  </div>
                  {row.displayName !== row.externalId ? (
                    <div
                      className="mt-0.5 truncate text-[11px] text-main-500"
                      title={row.displayName}
                    >
                      {row.displayName}
                    </div>
                  ) : null}
                </th>
                <td className="px-3 py-3 text-right text-main-300 tabular-nums whitespace-nowrap">
                  {row.sizeBytes === null ? "—" : sizeLabel(row.sizeBytes)}
                </td>
                <td className="px-3 py-3 text-right text-main-300 tabular-nums whitespace-nowrap">
                  {row.contextWindow === null ? "—" : row.contextWindow.toLocaleString("ru-RU")}
                </td>
                <td className="px-3 py-3">
                  <div className="flex flex-wrap gap-1">
                    {row.capabilities.length === 0 ? (
                      <span className="text-main-500">—</span>
                    ) : (
                      row.capabilities.map((capability) => (
                        <Chip key={capability}>{MODEL_CAPABILITY_LABELS[capability]}</Chip>
                      ))
                    )}
                  </div>
                </td>
                <td className="px-3 py-3" title={row.unavailableReason ?? undefined}>
                  <span
                    className={`inline-flex items-center gap-1.5 whitespace-nowrap ${row.available ? "text-ok" : "text-err"}`}
                  >
                    <StatusDot tone={row.available ? "ok" : "err"} />
                    {row.available ? "Доступна" : "Недоступна"}
                  </span>
                  {!row.available && row.unavailableReason ? (
                    <p className="mt-1 max-w-52 text-[11px] text-main-400">
                      {row.unavailableReason}
                    </p>
                  ) : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </ScrollArea>
  );
}
