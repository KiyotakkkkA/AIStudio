import type { ChatCitationDto, VectorStoreDto, VectorStoreId } from "@zvs/shared";

export default function ChatRetrieval({
  citations,
  storeIds,
  stores,
}: {
  readonly citations: readonly ChatCitationDto[];
  readonly storeIds: readonly VectorStoreId[];
  readonly stores: readonly VectorStoreDto[];
}) {
  return (
    <div className="space-y-2">
      {storeIds.map((id) => {
        const hits = citations.filter((citation) => citation.storeId === id);
        return (
          <details
            key={id}
            className="rounded-card border border-main-750 bg-main-900 p-3 text-xs text-main-300"
          >
            <summary className="cursor-pointer">
              Searched{" "}
              <strong className="text-main-100">
                {stores.find((store) => store.id === id)?.name ?? "Unavailable store"}
              </strong>{" "}
              — {hits.length} chunks at or above 0
            </summary>
            <div className="mt-2 flex flex-wrap gap-2">
              {hits.map((hit) => (
                <span
                  key={`${hit.documentId}:${hit.chunkIndex}`}
                  title={hit.sourcePath}
                  className="max-w-full rounded-pill bg-main-700 px-2 py-1 font-mono text-[11px] break-all"
                >
                  {hit.sourcePath} · {hit.score.toFixed(2)}
                </span>
              ))}
              {hits.length === 0 && <span>Подходящих источников нет.</span>}
            </div>
          </details>
        );
      })}
    </div>
  );
}
