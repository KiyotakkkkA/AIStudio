import { belowRelevanceFloor, type ChatCitationDto, type VectorSearchHitDto } from "@zvs/shared";

export const CHAT_FETCH_K = 20;
export const CHAT_SOURCES_MAX = 6;
export const CHAT_CHUNKS_PER_DOCUMENT = 3;

export interface StoreHits {
  readonly storeId: ChatCitationDto["storeId"];
  readonly hits: readonly VectorSearchHitDto[];
  readonly floor: number;
}

export interface MergeOptions {
  readonly limit?: number;
  readonly perDocument?: number;
}

export interface MergedHit {
  readonly storeId: ChatCitationDto["storeId"];
  readonly hit: VectorSearchHitDto;
}

export interface MergeStats {
  readonly fetched: number;
  readonly belowFloor: number;
  readonly duplicates: number;
  readonly overCap: number;
  readonly overLimit: number;
}

export function mergeRetrieval(
  hitsByStore: readonly StoreHits[],
  options: MergeOptions = {},
): { hits: MergedHit[]; stats: MergeStats } {
  const limit = options.limit ?? CHAT_SOURCES_MAX;
  const perDocument = options.perDocument ?? CHAT_CHUNKS_PER_DOCUMENT;
  let fetched = 0;
  let belowFloor = 0;
  const candidates: {
    storeId: MergedHit["storeId"];
    hit: VectorSearchHitDto;
    key: number;
    store: number;
    rank: number;
  }[] = [];
  hitsByStore.forEach(({ storeId, hits, floor }, store) => {
    fetched += hits.length;
    const kept = hits.filter((hit) => !belowRelevanceFloor(hit, floor));
    belowFloor += hits.length - kept.length;
    let key = 0;
    const keys = kept.map((hit) => hit.score);
    for (let index = keys.length - 1; index >= 0; index--) {
      key = Math.max(key, keys[index]!);
      keys[index] = key;
    }
    kept.forEach((hit, rank) => candidates.push({ storeId, hit, key: keys[rank]!, store, rank }));
  });
  candidates.sort(
    (left, right) => right.key - left.key || left.rank - right.rank || left.store - right.store,
  );

  const taken = new Map<string, number[]>();
  const merged: MergedHit[] = [];
  let duplicates = 0;
  let overCap = 0;
  let overLimit = 0;
  for (const { storeId, hit } of candidates) {
    const document = `${storeId}\u0000${hit.documentId}`;
    const indices = taken.get(document) ?? [];
    if (indices.some((index) => Math.abs(index - hit.chunkIndex) <= 1)) {
      duplicates += 1;
      continue;
    }
    if (indices.length >= perDocument) {
      overCap += 1;
      continue;
    }
    if (merged.length >= limit) {
      overLimit += 1;
      continue;
    }
    indices.push(hit.chunkIndex);
    taken.set(document, indices);
    merged.push({ storeId, hit });
  }
  return { hits: merged, stats: { fetched, belowFloor, duplicates, overCap, overLimit } };
}
