import type { ModelRow } from "./modelRows";

export type ModelTableSort = "name" | "size" | "context" | "availability";
export type ModelTableFilter = "all" | "selected" | "available" | "unavailable";
export type ModelSortDirection = "ascending" | "descending";

const names = new Intl.Collator("ru-RU", { numeric: true, sensitivity: "base" });

export function modelTableRows(
  rows: readonly ModelRow[],
  selectedIds: readonly string[],
  filter: ModelTableFilter,
  sort: ModelTableSort,
  direction: ModelSortDirection,
): ModelRow[] {
  const selected = new Set(selectedIds);
  return rows
    .filter((row) => {
      if (filter === "selected") return selected.has(row.externalId);
      if (filter === "available") return row.available;
      if (filter === "unavailable") return !row.available;
      return true;
    })
    .sort((a, b) => {
      let comparison: number;
      if (sort === "name") comparison = names.compare(a.externalId, b.externalId);
      else {
        const value = (row: ModelRow) =>
          sort === "size"
            ? row.sizeBytes
            : sort === "context"
              ? row.contextWindow
              : Number(row.available);
        const left = value(a);
        const right = value(b);
        // Unknown values stay last in either direction.
        if (left === null || right === null) {
          if (left !== right) return left === null ? 1 : -1;
          return names.compare(a.externalId, b.externalId);
        }
        comparison = left - right;
      }
      return (
        (direction === "ascending" ? comparison : -comparison) ||
        names.compare(a.externalId, b.externalId)
      );
    });
}
