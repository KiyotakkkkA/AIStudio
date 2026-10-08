import { useState } from "react";
import type { ChatCitationDto, VectorStoreDto, VectorStoreId } from "@zvs/shared";
import { Accordion, ScrollArea, SlidedPanel, Tabs } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiDatabaseOutline } from "@mdi/js";
import ChatMarkdown from "./ChatMarkdown";
import MessageActions from "../../ui/molecules/MessageActions";
import Icon from "../../ui/atoms/Icon";

export default function ChatMessage({
  content,
  reasoning = "",
  citations,
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
  readonly stores: readonly VectorStoreDto[];
  readonly partial?: boolean;
  readonly generating?: boolean;
  readonly usage?: string;
  readonly onEdit?: () => void;
  readonly onRefresh?: () => void;
  readonly onDelete?: () => void;
}) {
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const citationStoreIds = [...new Set(citations.map((citation) => citation.storeId))];
  const [activeStoreId, setActiveStoreId] = useState<VectorStoreId | "">(citationStoreIds[0] ?? "");
  const selectedStoreId =
    activeStoreId !== "" && citationStoreIds.includes(activeStoreId)
      ? activeStoreId
      : (citationStoreIds[0] ?? "");
  const selectedCitations = citations.filter((citation) => citation.storeId === selectedStoreId);

  return (
    <article aria-label="Ответ ассистента" className="group flex min-w-0 gap-3">
      <span
        aria-hidden="true"
        className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-control bg-linear-to-br from-accent-medium to-accent-dark text-[12px] font-semibold text-on-accent shadow-[inset_0_1px_0_rgb(255_255_255/0.2)]"
      >
        Z
      </span>
      <div className="min-w-0 flex-1">
        <div className="space-y-3">
          {reasoning && (
            <Accordion className="rounded-card border border-main-750 bg-main-850 px-3.5 py-2.5">
              <Accordion.Summary className="w-full text-left text-[12px] font-medium text-main-400">
                Размышления
              </Accordion.Summary>
              <Accordion.Content className="mt-2 border-l-2 border-main-750 pl-3 text-[12.5px]/relaxed whitespace-pre-wrap text-main-400">
                {reasoning}
              </Accordion.Content>
            </Accordion>
          )}
          <ChatMarkdown content={content} />
          {generating && (
            <p
              role="status"
              className="inline-flex items-center gap-2 text-[12px] font-medium text-accent-medium"
            >
              <span className="size-1.5 animate-pulse rounded-full bg-accent-dark" />
              Генерация…
            </p>
          )}
          {citations.length > 0 && (
            <button
              type="button"
              className="inline-flex h-7.5 items-center gap-2 rounded-full border border-main-750 bg-main-900 pr-1.5 pl-3 text-[12px] font-medium text-main-300 shadow-[0_1px_2px_rgb(40_25_10/0.05)] hover:border-main-600 hover:text-main-50"
              onClick={() => setSourcesOpen(true)}
            >
              <Icon path={mdiDatabaseOutline} size={15} className="text-accent-medium" />
              Источники
              <span className="rounded-full bg-accent-soft px-1.75 py-0.5 text-[11px] text-accent-medium tabular-nums">
                {citations.length}
              </span>
            </button>
          )}
          {partial && !generating && (
            <p className="text-[12px] text-warn">Частичный ответ · генерация не завершена</p>
          )}
          {usage && <p className="font-mono text-[11px] text-main-500">{usage}</p>}
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
      <SlidedPanel
        open={sourcesOpen}
        onClose={() => setSourcesOpen(false)}
        label="Источники ответа"
        panelPlacement="right"
        className="w-136 max-w-[92vw] border-l border-main-750 bg-main-850 shadow-pop"
      >
        <SlidedPanel.Header className="border-b border-main-750 px-6 py-4.5">
          <div className="flex items-start gap-3">
            <div className="min-w-0 flex-1">
              <SlidedPanel.Title className="text-[16px] font-semibold tracking-[-0.01em] text-main-50">
                Источники ответа
              </SlidedPanel.Title>
              <SlidedPanel.Subtitle className="mt-0.5 text-[12px] text-main-500">
                Внешние данные, использованные при подготовке ответа
              </SlidedPanel.Subtitle>
            </div>
          </div>
        </SlidedPanel.Header>
        <SlidedPanel.Content className="flex min-h-0 flex-1 flex-col gap-4 px-6 py-5">
          <Tabs
            value={selectedStoreId}
            onChange={(value) => {
              const storeId = citationStoreIds.find((id) => id === value);
              if (storeId) setActiveStoreId(storeId);
            }}
            options={citationStoreIds.map((id) => ({
              value: id,
              label: stores.find((store) => store.id === id)?.name ?? "Хранилище",
            }))}
            label="Хранилища источников"
            classNames={{
              list: "flex-wrap",
              tab: "text-[12px]",
              activeTab: "text-[12px] text-accent-medium",
            }}
          />
          <ScrollArea className="min-h-0 flex-1" showScrollbar>
            <div className="space-y-2 pr-1">
              {selectedCitations.map((citation) => (
                <article
                  key={`${citation.documentId}:${citation.chunkIndex}`}
                  className="rounded-card border border-main-750 bg-main-900 p-3.5"
                >
                  <div className="flex items-start justify-between gap-3">
                    <h3 className="min-w-0 truncate text-[13px] font-semibold text-main-100">
                      {citation.sourcePath}
                    </h3>
                    <span className="shrink-0 rounded-full bg-accent-soft px-2 py-0.5 text-[11px] font-semibold text-accent-medium tabular-nums">
                      {(citation.score * 100).toFixed(0)}%
                    </span>
                  </div>
                  <p className="mt-1.5 text-[12px] text-main-500">
                    Фрагмент {citation.chunkIndex + 1}
                  </p>
                </article>
              ))}
            </div>
          </ScrollArea>
        </SlidedPanel.Content>
      </SlidedPanel>
    </article>
  );
}
