import type { GraphExpansion, VectorDocumentDto } from "@zvs/shared";

export {
  GRAPH_EXPANSION_DEFAULT_MAX,
  mergeGraphNeighbors,
  type GraphExpansion,
  type GraphNeighborhood,
} from "@zvs/shared";

export function graphRows(
  expansions: readonly GraphExpansion[],
  documents: readonly VectorDocumentDto[],
) {
  const byId = new Map(documents.map((document) => [document.id as string, document] as const));
  return expansions.map((expansion) => ({
    id: `graph:${expansion.viaDocumentId}:${expansion.documentId}:${expansion.relation}`,
    relation: expansion.relation,
    path: expansion.path,
    position:
      expansion.chunkCount === null
        ? "документ"
        : `${expansion.chunkCount.toLocaleString("ru-RU")} чанков`,
    via: byId.get(expansion.viaDocumentId)?.sourcePath ?? expansion.viaDocumentId,
    text: `Фрагмент не ранжировался: документ добавлен по связи «${expansion.relation}» с результатом поиска.`,
  }));
}
