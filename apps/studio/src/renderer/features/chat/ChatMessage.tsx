import { useState, type ReactNode } from "react";
import type { ChatCitationDto, VectorStoreDto, VectorStoreId } from "@zvs/shared";
import { Accordion, ScrollArea, SlidedPanel, Tabs } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiFileEyeOutline, mdiThoughtBubbleOutline } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import ChatMarkdown from "./ChatMarkdown";
import ChatSources from "./ChatSources";
import { OPEN_CITATION, citationLocation, citationSources } from "./citations";
import { useStore } from "../../stores/useStore";
import MessageActions from "../../ui/molecules/MessageActions";
import AssistantAvatar from "../../ui/atoms/AssistantAvatar";
import Icon from "../../ui/atoms/Icon";
import GenerationStatus from "../../ui/molecules/GenerationStatus";

export default function ChatMessage({
  content,
  reasoning = "",
  citations,
  stores,
  partial = false,
  generating = false,
  status = "Обработка запроса",
  usage,
  tools,
  artifacts,
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
  readonly status?: string;
  readonly usage?: string;
  readonly tools?: ReactNode;
  readonly artifacts?: ReactNode;
  readonly onEdit?: () => void;
  readonly onRefresh?: () => void;
  readonly onDelete?: () => void;
}) {
  const { viewer } = useStore();
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const storeName = (storeId: VectorStoreId) =>
    stores.find((store) => store.id === storeId)?.name ?? "Хранилище";
  const view = (citation: ChatCitationDto) => {
    setSourcesOpen(false);
    void viewer.openCitation(citation, citationLocation(citation, storeName(citation.storeId)));
  };
  const citationStoreIds = [...new Set(citations.map((citation) => citation.storeId))];
  const [activeStoreId, setActiveStoreId] = useState<VectorStoreId | "">(citationStoreIds[0] ?? "");
  const selectedStoreId =
    activeStoreId !== "" && citationStoreIds.includes(activeStoreId)
      ? activeStoreId
      : (citationStoreIds[0] ?? "");
  const selectedCitations = citations.filter((citation) => citation.storeId === selectedStoreId);
  const thinking = generating && content === "";
  const sources = citationSources(citations, storeName);

  return (
    <article aria-label="Ответ ассистента" className="group flex min-w-0 gap-3">
      <AssistantAvatar size={28} live={generating} className="mt-0.5" />
      <div className="min-w-0 flex-1">
        <div className="space-y-3">
          {reasoning && (
            <Accordion className="rounded-card bg-main-800">
              <Accordion.Summary className="min-h-9.5 gap-2.5 rounded-card px-3 py-1.5 text-[12px] font-medium text-main-300 transition-colors hover:text-main-50">
                <span className="flex min-w-0 items-center gap-2.5">
                  <span
                    aria-hidden="true"
                    className="flex size-6 shrink-0 items-center justify-center rounded-pill bg-accent-soft text-accent-medium"
                  >
                    <Icon path={mdiThoughtBubbleOutline} size={14} />
                  </span>
                  <span className={`truncate ${thinking ? "zvs-shimmer-text" : ""}`}>
                    {thinking ? "Размышления…" : "Размышления"}
                  </span>
                  <span className="shrink-0 font-mono text-[11px] font-normal text-main-500 tabular-nums">
                    {reasoningWords(reasoning)} сл.
                  </span>
                </span>
              </Accordion.Summary>
              <Accordion.Content className="border-transparent px-3 pt-1 pb-3">
                <ScrollArea className="max-h-80 border-l-2 border-accent-line pr-1 pl-3 text-[12.5px]/relaxed whitespace-pre-wrap text-main-400">
                  {reasoning}
                </ScrollArea>
              </Accordion.Content>
            </Accordion>
          )}
          {tools}
          <ChatMarkdown
            content={content}
            streaming={generating}
            citations={citations}
            onCite={view}
          />
          {generating && <GenerationStatus label={status} />}
          <ChatSources
            sources={sources}
            fragments={citations.length}
            onOpen={(storeId) => {
              if (storeId !== undefined) setActiveStoreId(storeId);
              setSourcesOpen(true);
            }}
            onView={view}
          />
          {artifacts}
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
        className="w-136 max-w-[92vw] bg-main-800 shadow-pop"
      >
        <SlidedPanel.Header className="border-b border-line px-6 py-4.5">
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
                  className="rounded-card border border-line bg-main-900 p-3.5"
                >
                  <div className="flex items-start justify-between gap-3">
                    <h3 className="flex min-w-0 items-center gap-2 text-[13px] font-semibold text-main-100">
                      <span className="shrink-0 rounded-full bg-main-800 px-1.75 py-0.5 font-mono text-[11px] text-main-400 tabular-nums">
                        [{citations.indexOf(citation) + 1}]
                      </span>
                      <span className="truncate">{citation.sourcePath}</span>
                    </h3>
                    <span className="flex shrink-0 items-center gap-1">
                      <IconButton
                        label={OPEN_CITATION}
                        tooltip="bottom-left"
                        path={mdiFileEyeOutline}
                        iconSize={16}
                        onClick={() => view(citation)}
                      />
                      {citation.related === null ? (
                        <span className="shrink-0 rounded-full bg-accent-soft px-2 py-0.5 text-[11px] font-semibold text-accent-medium tabular-nums">
                          {(citation.score * 100).toFixed(0)}%
                        </span>
                      ) : (
                        <span
                          title="Документ добавлен по связи с найденным источником, он не ранжировался"
                          className="shrink-0 rounded-full border border-dashed border-line-strong px-2 py-0.5 text-[11px] text-main-400"
                        >
                          связь «{citation.related}»
                        </span>
                      )}
                    </span>
                  </div>
                  <p className="mt-1.5 text-[12px] text-main-500">{citationPlace(citation)}</p>
                </article>
              ))}
            </div>
          </ScrollArea>
        </SlidedPanel.Content>
      </SlidedPanel>
    </article>
  );
}

function reasoningWords(reasoning: string): number {
  return reasoning.split(/\s+/u).filter(Boolean).length;
}

function citationPlace(citation: ChatCitationDto): string {
  return [
    `Фрагмент ${String(citation.chunkIndex + 1)}`,
    ...(citation.page === null ? [] : [`стр. ${String(citation.page)}`]),
    ...(citation.section === "" ? [] : [citation.section]),
  ].join(" · ");
}
