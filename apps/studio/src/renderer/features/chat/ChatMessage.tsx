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
        className="flex size-6.5 shrink-0 items-center justify-center rounded-[7px] bg-accent-dark font-semibold text-main-900"
      >
        Z
      </span>
      <div className="min-w-0 flex-1">
        <div className="space-y-3">
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
            <button
              type="button"
              className="inline-flex h-8 items-center gap-2 rounded-lg bg-main-800 px-3 text-xs text-main-300 hover:bg-main-750 hover:text-main-100"
              onClick={() => setSourcesOpen(true)}
            >
              <Icon path={mdiDatabaseOutline} size={16} />
              Источники
              <span className="rounded-pill bg-main-700 px-1.5 py-0.5 text-[10px] text-main-100">
                {citations.length}
              </span>
            </button>
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
      <SlidedPanel
        open={sourcesOpen}
        onClose={() => setSourcesOpen(false)}
        label="Источники ответа"
        panelPlacement="right"
        className="w-136 max-w-[92vw] border-l border-main-750 bg-main-900"
      >
        <SlidedPanel.Header className="border-b border-main-750 px-5 py-4">
          <div className="flex items-start gap-3">
            <div className="min-w-0 flex-1">
              <SlidedPanel.Title className="text-base font-semibold text-main-50">
                Источники ответа
              </SlidedPanel.Title>
              <SlidedPanel.Subtitle className="mt-1 text-xs text-main-500">
                Внешние данные, использованные при подготовке ответа
              </SlidedPanel.Subtitle>
            </div>
          </div>
        </SlidedPanel.Header>
        <SlidedPanel.Content className="flex min-h-0 flex-1 flex-col gap-4 p-4">
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
            classNames={{ list: "flex-wrap", tab: "text-xs", activeTab: "text-xs" }}
          />
          <ScrollArea className="min-h-0 flex-1" showScrollbar>
            <div className="space-y-2 pr-1">
              {selectedCitations.map((citation) => (
                <article
                  key={`${citation.documentId}:${citation.chunkIndex}`}
                  className="rounded-card border border-main-750 bg-main-800 p-3"
                >
                  <div className="flex items-start justify-between gap-3">
                    <h3 className="min-w-0 truncate text-sm font-semibold text-main-100">
                      {citation.sourcePath}
                    </h3>
                    <span className="shrink-0 text-xs font-semibold text-main-300">
                      {(citation.score * 100).toFixed(0)}%
                    </span>
                  </div>
                  <p className="mt-2 text-xs text-main-500">Фрагмент {citation.chunkIndex + 1}</p>
                </article>
              ))}
            </div>
          </ScrollArea>
        </SlidedPanel.Content>
      </SlidedPanel>
    </article>
  );
}
