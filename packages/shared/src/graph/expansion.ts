import type { GraphNeighborDto } from "./GraphQueryDto.js";

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

export interface GraphExpansionDocument {
  readonly id: string;
  readonly sourcePath: string;
  readonly chunkCount: number;
}

export const GRAPH_EXPANSION_DEFAULT_MAX = 5;

export function mergeGraphNeighbors(
  hits: readonly { documentId: string }[],
  neighborhoods: readonly GraphNeighborhood[],
  documents: readonly GraphExpansionDocument[],
  cap: number,
): GraphExpansion[] {
  if (cap <= 0) return [];
  const ranked = new Set(hits.map((hit) => hit.documentId));
  const byId = new Map(documents.map((document) => [document.id, document] as const));
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
