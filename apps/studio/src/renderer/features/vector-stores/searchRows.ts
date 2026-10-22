import {
  belowRelevanceFloor,
  type VectorChunkDto,
  type VectorHitProvenance,
  type VectorSearchHitDto,
} from "@zvs/shared";

export const PROVENANCE_LABELS: Record<VectorHitProvenance, string> = {
  vector: "векторы",
  keyword: "ключевые слова",
  both: "векторы + слова",
};

const PROVENANCE_TITLES: Record<VectorHitProvenance, string> = {
  vector: "Найден только векторным поиском",
  keyword: "Найден только поиском по ключевым словам (BM25)",
  both: "Найден обоими поисками",
};

function textOf(hit: Pick<VectorSearchHitDto, "text" | "payload">): string | null {
  if (hit.text !== "") return hit.text;
  const payload = hit.payload;
  if (typeof payload === "string") return payload;
  if (payload !== null && typeof payload === "object" && !Array.isArray(payload))
    return typeof payload.text === "string" ? payload.text : null;
  return null;
}

export function placeOf(hit: Pick<VectorSearchHitDto, "page" | "section">): string | null {
  const parts = [
    ...(hit.page === null ? [] : [`стр. ${String(hit.page)}`]),
    ...(hit.section === "" ? [] : [hit.section]),
  ];
  return parts.length === 0 ? null : parts.join(" · ");
}

export function searchRows(hits: readonly VectorSearchHitDto[], floor: number) {
  return hits.map((hit) => {
    const payload = hit.payload;
    const object =
      payload !== null && typeof payload === "object" && !Array.isArray(payload) ? payload : {};
    const total = object.chunkCount;
    return {
      id: hit.id,
      documentId: hit.documentId,
      chunkIndex: hit.chunkIndex,
      score: hit.score.toFixed(2),
      vectorScore: hit.vectorScore === undefined ? null : hit.vectorScore.toFixed(2),
      rerankScore: hit.rerankScore === undefined ? null : hit.rerankScore.toFixed(2),
      keywordScore: hit.keywordScore === undefined ? null : hit.keywordScore.toFixed(2),
      provenance: hit.provenance === undefined ? null : PROVENANCE_LABELS[hit.provenance],
      provenanceTitle: hit.provenance === undefined ? null : PROVENANCE_TITLES[hit.provenance],
      width: `${Math.max(0, Math.min(1, hit.score)) * 100}%`,
      belowFloor: belowRelevanceFloor(hit, floor),
      path: hit.path || hit.documentId,
      place: placeOf(hit),
      position: `${hit.chunkIndex + 1}${typeof total === "number" && Number.isInteger(total) && total > hit.chunkIndex ? `/${total}` : ""}`,
      text: textOf(hit) ?? "Текст фрагмента отсутствует.",
    };
  });
}

export function neighbourRows(hit: { chunkIndex: number }, chunks: readonly VectorChunkDto[]) {
  return chunks
    .filter((chunk) => Math.abs(chunk.chunkIndex - hit.chunkIndex) === 1)
    .sort((left, right) => left.chunkIndex - right.chunkIndex)
    .map((chunk) => ({
      id: chunk.id,
      before: chunk.chunkIndex < hit.chunkIndex,
      label: `${chunk.chunkIndex < hit.chunkIndex ? "Предыдущий" : "Следующий"} фрагмент ${String(chunk.chunkIndex + 1)}`,
      place: placeOf(chunk),
      text: textOf(chunk) ?? "",
    }));
}
