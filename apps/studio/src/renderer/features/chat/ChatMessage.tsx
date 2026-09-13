import type { ChatCitationDto, VectorStoreDto, VectorStoreId } from "@zvs/shared";
import ChatMarkdown from "./ChatMarkdown";
import ChatRetrieval from "./ChatRetrieval";

export default function ChatMessage({
  content,
  citations,
  storeIds,
  stores,
  partial = false,
  generating = false,
  usage,
}: {
  readonly content: string;
  readonly citations: readonly ChatCitationDto[];
  readonly storeIds: readonly VectorStoreId[];
  readonly stores: readonly VectorStoreDto[];
  readonly partial?: boolean;
  readonly generating?: boolean;
  readonly usage?: string;
}) {
  return (
    <article aria-label="Ответ ассистента" className="flex min-w-0 gap-3">
      <span
        aria-hidden="true"
        className="flex size-6.5 shrink-0 items-center justify-center rounded-[7px] bg-accent-dark font-semibold text-main-900"
      >
        Z
      </span>
      <div className="min-w-0 flex-1 space-y-3">
        {storeIds.length > 0 && (
          <ChatRetrieval citations={citations} storeIds={storeIds} stores={stores} />
        )}
        <ChatMarkdown content={content} />
        {generating && (
          <p role="status" className="text-xs text-accent-medium">
            Генерация…
          </p>
        )}
        {citations.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 text-[11px] text-main-400">
            <span>Источники</span>
            {citations.map((citation) => (
              <span
                key={`${citation.storeId}:${citation.documentId}:${citation.chunkIndex}`}
                title={citation.sourcePath}
                className="max-w-full rounded-pill border border-main-600 px-2 py-1 break-all text-accent-medium"
              >
                {citation.sourcePath} · {citation.score.toFixed(2)}
              </span>
            ))}
          </div>
        )}
        {partial && !generating && (
          <p className="text-xs text-warn">Частичный ответ · генерация не завершена</p>
        )}
        {usage && <p className="font-mono text-[10.5px] text-main-500">{usage}</p>}
      </div>
    </article>
  );
}
