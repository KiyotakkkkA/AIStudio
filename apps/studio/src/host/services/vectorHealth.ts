import type { VectorHealth } from "@zvs/shared";
import type { VectorStoreEntity } from "../data/schema/index.ts";
import type { VectorStats } from "../drivers/rust/vectorTypes.ts";

export function deriveVectorHealth(
  row: Pick<
    VectorStoreEntity,
    "tableCreatedAt" | "documents" | "vectors" | "bytes" | "dimension" | "metric"
  > &
    Partial<Pick<VectorStoreEntity, "status">>,
  stats: VectorStats | null,
  failed = false,
): VectorHealth {
  if (failed) return "broken";
  if (stats === null)
    return row.tableCreatedAt === null && row.status !== "broken" ? "pending" : "broken";
  if (stats.dimension !== row.dimension || stats.metric !== row.metric) return "broken";
  if (
    stats.rowCount !== row.vectors ||
    stats.documentCount !== row.documents ||
    stats.onDiskBytes !== row.bytes
  )
    return "stale";
  return "healthy";
}
