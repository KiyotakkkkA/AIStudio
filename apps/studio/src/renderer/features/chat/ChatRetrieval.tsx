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
            className="rounded-card border border-main-750 bg-main-850 px-3.5 py-2.5 text-[12px] text-main-300"
          >
            <summary className="cursor-pointer marker:text-main-500">
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
                  className="max-w-full rounded-pill border border-main-750 bg-main-900 px-2 py-0.5 font-mono text-[11px] break-all text-main-400"
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
