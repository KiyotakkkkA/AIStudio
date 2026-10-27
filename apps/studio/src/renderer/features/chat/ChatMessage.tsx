import { useState, type ReactNode } from "react";
import type {
  ChatCitationDto,
  DocumentSelectionDto,
  MessageId,
  VectorStoreDto,
  VectorStoreId,
  WebSourceDto,
} from "@zvs/shared";
import { ScrollArea, SlidedPanel, Tabs } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiFileEyeOutline } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import ChatMarkdown from "./ChatMarkdown";
import AnswerQuote from "./AnswerQuote";
import ReasoningBlock from "./ReasoningBlock";
import ChatSources from "./ChatSources";
import WebSourceCard from "./WebSourceCard";
import { WEB_SOURCES_TAB } from "./webSources";
import {
  OPEN_CITATION,
  OPEN_SELECTION,
  SELECTIONS_TAB,
  citationLocation,
  citationSources,
  documentSelections,
  selectionLocation,
  type DocumentQuote,
  selectionSources,
} from "./citations";
import { excerptPreview } from "./selectionCopy";
import { useStore } from "../../stores/useStore";
import MessageActions, { type MessageBranchAction } from "../../ui/molecules/MessageActions";
import AssistantAvatar from "../../ui/atoms/AssistantAvatar";
import GenerationStatus from "../../ui/molecules/GenerationStatus";

export default function ChatMessage({
  messageId,
  content,
  reasoning = "",
  citations,
  webSources = [],
  selections: attached = [],
  stores,
  partial = false,
  generating = false,
  status = "Обработка запроса",
  since = null,
  usage,
  tools,
  artifacts,
  onEdit,
  onRefresh,
  onDelete,
  pinned = false,
  onPin,
  branch,
}: {
  readonly messageId?: MessageId;
  readonly content: string;
  readonly reasoning?: string;
  readonly citations: readonly ChatCitationDto[];
  readonly webSources?: readonly WebSourceDto[];
  readonly selections?: readonly DocumentSelectionDto[];
  readonly stores: readonly VectorStoreDto[];
  readonly partial?: boolean;
  readonly generating?: boolean;
  readonly status?: string;
  readonly since?: number | null;
  readonly usage?: string;
  readonly tools?: ReactNode;
  readonly artifacts?: ReactNode;
  readonly onEdit?: () => void;
  readonly onRefresh?: () => void;
  readonly onDelete?: () => void;
  readonly pinned?: boolean;
  readonly onPin?: () => void;
  readonly branch?: MessageBranchAction;
}) {
  const { viewer, chat } = useStore();
  const selections = documentSelections(attached);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const storeName = (storeId: VectorStoreId) =>
    stores.find((store) => store.id === storeId)?.name ?? "Хранилище";
  const view = (citation: ChatCitationDto) => {
    setSourcesOpen(false);
    void viewer.openCitation(citation, citationLocation(citation, storeName(citation.storeId)));
  };
  const viewSelection = (selection: DocumentQuote) => {
    setSourcesOpen(false);
    void viewer.openSelection(selection, selectionLocation(selection));
  };
  const openWeb = (source: WebSourceDto) => {
    setSourcesOpen(false);
    void chat.openSite(source.url);
  };
  const citationStoreIds = [...new Set(citations.map((citation) => citation.storeId))];
  const tabs: string[] = [
    ...(selections.length > 0 ? [SELECTIONS_KEY] : []),
    ...citationStoreIds,
    ...(webSources.length > 0 ? [WEB_KEY] : []),
  ];
  const [activeTab, setActiveTab] = useState<string>(tabs[0] ?? "");
  const selectedTab = tabs.find((tab) => tab === activeTab) ?? tabs[0] ?? "";
  const selectedCitations = citations.filter((citation) => citation.storeId === selectedTab);
  const thinking = generating && content === "";
  const markdown = (
    <ChatMarkdown
      content={content}
      streaming={generating}
      citations={citations}
      webSources={webSources}
      onCite={view}
      onOpenWeb={openWeb}
    />
  );
  const sources = citationSources(citations, storeName);

  return (
    <article aria-label="Ответ ассистента" className="group flex min-w-0 gap-3">
      <AssistantAvatar size={28} live={generating} className="mt-0.5" />
      <div className="min-w-0 flex-1">
        <div className="space-y-3">
          {reasoning && <ReasoningBlock reasoning={reasoning} thinking={thinking} />}
          {tools}
          {messageId === undefined || generating ? (
            markdown
          ) : (
            <AnswerQuote messageId={messageId}>{markdown}</AnswerQuote>
          )}
          {generating && <GenerationStatus label={status} since={since} />}
          <ChatSources
            sources={sources}
            selections={selectionSources(selections)}
            web={webSources}
            fragments={citations.length + selections.length + webSources.length}
            onOpen={() => setSourcesOpen(true)}
            onView={view}
            onViewSelection={viewSelection}
            onOpenWeb={openWeb}
          />
          {artifacts}
          {partial && !generating && (
            <p className="text-[12px] text-warn">Частичный ответ · генерация не завершена</p>
          )}
          {usage && <p className="pt-2 font-mono text-[11px] text-main-500">{usage}</p>}
        </div>
        <div className="mt-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
          <MessageActions
            content={content}
            onEdit={onEdit}
            onRefresh={onRefresh}
            onDelete={onDelete}
            pinned={pinned}
            onPin={onPin}
            branch={branch}
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
            value={selectedTab}
            onChange={setActiveTab}
            options={tabs.map((id) => ({
              value: id,
              label:
                id === SELECTIONS_KEY
                  ? SELECTIONS_TAB
                  : id === WEB_KEY
                    ? WEB_SOURCES_TAB
                    : (stores.find((store) => store.id === id)?.name ?? "Хранилище"),
            }))}
            label="Группы источников"
            classNames={{
              list: "flex-wrap",
              tab: "text-[12px]",
              activeTab: "text-[12px] text-accent-medium",
            }}
          />
          <ScrollArea className="min-h-0 flex-1" showScrollbar>
            <div className="space-y-2 pr-1">
              {selectedTab === SELECTIONS_KEY
                ? selections.map((selection) => (
                    <article
                      key={`${selection.documentId}:${String(selection.page)}:${String(selection.start)}`}
                      className="rounded-card border border-line bg-main-900 p-3.5"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <h3 className="min-w-0 truncate text-[13px] font-semibold text-main-100">
                          {selection.name}
                        </h3>
                        <IconButton
                          label={OPEN_SELECTION}
                          tooltip="bottom-left"
                          path={mdiFileEyeOutline}
                          iconSize={16}
                          onClick={() => viewSelection(selection)}
                        />
                      </div>
                      <p className="mt-1.5 text-[12px] text-main-500">
                        {selectionLocation(selection)}
                      </p>
                      <p className="mt-2 line-clamp-4 border-l-2 border-accent-line pl-2.5 text-[12.5px]/relaxed text-main-300">
                        {excerptPreview(selection.text, SELECTION_EXCERPT)}
                      </p>
                    </article>
                  ))
                : null}
              {selectedTab === WEB_KEY
                ? webSources.map((source, index) => (
                    <WebSourceCard
                      key={`${String(index)}:${source.url}`}
                      source={source}
                      number={index + 1}
                      onOpen={openWeb}
                    />
                  ))
                : null}
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

const SELECTIONS_KEY = "selections";
const WEB_KEY = "web";
const SELECTION_EXCERPT = 600;

function citationPlace(citation: ChatCitationDto): string {
  return [
    `Фрагмент ${String(citation.chunkIndex + 1)}`,
    ...(citation.page === null ? [] : [`стр. ${String(citation.page)}`]),
    ...(citation.section === "" ? [] : [citation.section]),
  ].join(" · ");
}
