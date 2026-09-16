import type { GraphNeighborDto, VectorDocumentDto } from "@zvs/shared";

export interface GraphNeighborhood {
  readonly documentId: string;
  readonly neighbors: readonly GraphNeighborDto[];
}

export interface GraphExpansion {
  readonly documentId: string;
  readonly relation: string;
  readonly direction: GraphNeighborDto["direction"];
  readonly viaDocumentId: string;
  readonly path: string;
  readonly chunkCount: number | null;
}

export const GRAPH_EXPANSION_DEFAULT_MAX = 5;

/**
 * Widens vector results by one hop. A document already ranked by the search keeps its score and is
 * never repeated here, the first edge that reaches a document wins, and the cap is a hard limit.
 */
export function mergeGraphNeighbors(
  hits: readonly { documentId: string }[],
  neighborhoods: readonly GraphNeighborhood[],
  documents: readonly VectorDocumentDto[],
  cap: number,
): GraphExpansion[] {
  if (cap <= 0) return [];
  const ranked = new Set(hits.map((hit) => hit.documentId));
  const byId = new Map(documents.map((document) => [document.id as string, document] as const));
  const taken = new Set<string>();
  const expansions: GraphExpansion[] = [];
  for (const neighborhood of neighborhoods) {
    for (const neighbor of neighborhood.neighbors) {
      if (expansions.length >= cap) return expansions;
      const documentId = neighbor.node.id;
      if (neighbor.node.kind !== "document") continue;
      if (ranked.has(documentId) || taken.has(documentId)) continue;
      taken.add(documentId);
      const document = byId.get(documentId);
      expansions.push({
        documentId,
        relation: neighbor.edge.relation,
        direction: neighbor.direction,
        viaDocumentId: neighborhood.documentId,
        path: document?.sourcePath ?? documentId,
        chunkCount: document?.chunkCount ?? null,
      });
    }
  }
  return expansions;
}

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
