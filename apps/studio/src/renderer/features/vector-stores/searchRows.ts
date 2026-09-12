import type { VectorSearchHitDto } from "@zvs/shared";

export function searchRows(hits: readonly VectorSearchHitDto[], floor: number) {
  return hits.map((hit) => {
    const payload = hit.payload;
    const object =
      payload !== null && typeof payload === "object" && !Array.isArray(payload) ? payload : {};
    const total = object.chunkCount;
    return {
      id: hit.id,
      score: hit.score.toFixed(2),
      width: `${Math.max(0, Math.min(1, hit.score)) * 100}%`,
      belowFloor: hit.score < floor,
      path: hit.path || hit.documentId,
      position: `${hit.chunkIndex + 1}${typeof total === "number" && Number.isInteger(total) && total > hit.chunkIndex ? `/${total}` : ""}`,
      text:
        typeof payload === "string"
          ? payload
          : typeof object.text === "string"
            ? object.text
            : "Текст чанка отсутствует в payload.",
    };
  });
}
