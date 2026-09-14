import type { ChatCitationDto, VectorStoreDto, VectorStoreId } from "@zvs/shared";
import { Accordion } from "@kiyotakkkka/zvs-uikit-lib";
import ChatMarkdown from "./ChatMarkdown";
import ChatRetrieval from "./ChatRetrieval";
import MessageActions from "../../ui/molecules/MessageActions";

export default function ChatMessage({
  content,
  reasoning = "",
  citations,
  storeIds,
  stores,
  partial = false,
  generating = false,
  usage,
  onEdit,
  onRefresh,
  onDelete,
}: {
  readonly content: string;
  readonly reasoning?: string;
  readonly citations: readonly ChatCitationDto[];
  readonly storeIds: readonly VectorStoreId[];
  readonly stores: readonly VectorStoreDto[];
  readonly partial?: boolean;
  readonly generating?: boolean;
  readonly usage?: string;
  readonly onEdit?: () => void;
  readonly onRefresh?: () => void;
  readonly onDelete?: () => void;
}) {
  return (
    <article aria-label="Ответ ассистента" className="group flex min-w-0 gap-3">
      <span
        aria-hidden="true"
        className="flex size-6.5 shrink-0 items-center justify-center rounded-[7px] bg-accent-dark font-semibold text-main-900"
      >
        Z
      </span>
      <div className="min-w-0 flex-1">
        <div className="space-y-3">
          {storeIds.length > 0 && (
            <ChatRetrieval citations={citations} storeIds={storeIds} stores={stores} />
          )}
          {reasoning && (
            <Accordion className="rounded-card border border-main-750 bg-main-800/40">
              <Accordion.Summary className="w-full text-left text-xs text-main-400">
                Размышления
              </Accordion.Summary>
              <Accordion.Content className="mt-2 text-xs/relaxed  whitespace-pre-wrap text-main-400">
                {reasoning}
              </Accordion.Content>
            </Accordion>
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
                  className="max-w-full rounded-pill border border-main-750 px-2 py-1 break-all text-accent-medium"
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
        <div className="mt-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
          <MessageActions
            content={content}
            onEdit={onEdit}
            onRefresh={onRefresh}
            onDelete={onDelete}
          />
        </div>
      </div>
    </article>
  );
}
