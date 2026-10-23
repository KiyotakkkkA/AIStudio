import type { DocumentSelectionDto } from "@zvs/shared";
import { mdiClose, mdiTextBoxOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import {
  SELECTIONS_LABEL,
  excerptPreview,
  removeSelectionLabel,
  selectionTitle,
} from "./selectionCopy";

export default function SelectionChips({
  selections,
  onRemove,
  align = "start",
}: {
  readonly selections: readonly DocumentSelectionDto[];
  readonly onRemove?: (index: number) => void;
  readonly align?: "start" | "end";
}) {
  if (selections.length === 0) return null;
  return (
    <ul
      aria-label={SELECTIONS_LABEL}
      className={`m-0 flex list-none flex-wrap gap-1.5 p-0 ${align === "end" ? "justify-end" : ""}`}
    >
      {selections.map((selection, index) => (
        <li
          key={`${selection.documentId}:${String(selection.page)}:${String(selection.start)}`}
          title={selection.text}
          className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-full bg-accent-soft pr-1.5 pl-2.5 text-[12px] font-medium text-accent-medium"
        >
          <Icon path={mdiTextBoxOutline} size={14} />
          <span className="max-w-48 truncate">{selectionTitle(selection)}</span>
          <span className="max-w-56 truncate font-normal text-main-400">
            «{excerptPreview(selection.text)}»
          </span>
          {onRemove === undefined ? (
            <span className="w-1" />
          ) : (
            <button
              type="button"
              aria-label={removeSelectionLabel(selection)}
              className="flex size-4.5 flex-none items-center justify-center rounded-full hover:bg-accent-dark hover:text-on-accent"
              onClick={() => onRemove(index)}
            >
              <Icon path={mdiClose} size={13} />
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}
