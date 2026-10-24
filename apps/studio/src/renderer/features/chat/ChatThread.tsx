import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import {
  mdiConsole,
  mdiDatabaseOutline,
  mdiDockLeft,
  mdiFolderOutline,
  mdiTuneVariant,
} from "@mdi/js";
import Chip from "../../ui/atoms/Chip";
import AssistantAvatar from "../../ui/atoms/AssistantAvatar";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import TextArea from "../../ui/atoms/TextArea";
import ChatMessage from "./ChatMessage";
import MessageActions from "../../ui/molecules/MessageActions";
import ChatComposer from "./ChatComposer";
import ChatApprovals from "./ChatApprovals";
import ChatWorkingFolder from "./ChatWorkingFolder";
import FolderAccessModal from "./FolderAccessModal";
import ChatToolCalls from "./ChatToolCalls";
import { answerCitations, questionSelections } from "./citations";
import { answerArtifacts } from "./artifacts";
import ChatArtifacts from "./ChatArtifacts";
import { shouldPinToBottom } from "./scroll";
import type ChatStore from "./ChatStore";
import { CONTEXT_PANEL, FILES_PANEL, PANEL_GROUP_LABEL } from "./explorer/explorerCopy";
import TerminalDock from "../terminal/TerminalDock";
import { TERMINAL_PANEL } from "../terminal/terminalCopy";
import MemoryDropdown from "../memory/MemoryDropdown";
import { AnimatePresence, motion } from "motion/react";
import FadeIn from "../../ui/motion/FadeIn";
import { useFx, useTransition } from "../../ui/motion/fx";
import { toolTitle } from "./toolCallCopy";
import { SHOW_CONVERSATIONS } from "./conversationListCopy";

function PanelButton({
  label,
  path,
  active,
  activeClassName = "bg-main-700 text-main-50",
  onClick,
}: {
  readonly label: string;
  readonly path: string;
  readonly active: boolean;
  readonly activeClassName?: string;
  readonly onClick: () => void;
}) {
  return (
    <IconButton
      label={label}
      tooltip="bottom-center"
      aria-pressed={active}
      path={path}
      iconSize={16}
      onClick={onClick}
      {...(active ? { toneClassName: activeClassName } : {})}
    />
  );
}

export default observer(function ChatThread({ store }: { readonly store: ChatStore }) {
  const viewport = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [jump, setJump] = useState(false);
  const answerSelections = questionSelections(store.active?.messages ?? []);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState("");
  const { motion: level } = useFx();
  const pop = useTransition("reveal");
  const scrollLatest = () => {
    const element = viewport.current;
    if (element) element.scrollTop = element.scrollHeight;
  };
  useLayoutEffect(() => {
    pinned.current = true;
    setJump(false);
    scrollLatest();
  }, [store.active?.id, store.revealed]);
  useLayoutEffect(() => {
    if (pinned.current) scrollLatest();
  }, [
    store.liveText,
    store.active?.messages.length,
    store.generating,
    store.liveCitations.length,
    store.liveToolCalls,
  ]);
  useEffect(() => {
    if (!content.current || typeof ResizeObserver === "undefined") return;
    const resize = new ResizeObserver(() => {
      if (pinned.current) scrollLatest();
    });
    resize.observe(content.current);
    return () => resize.disconnect();
  }, []);
  return (
    <section aria-label="Лента чата" className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex h-14 shrink-0 items-center gap-2.5 px-6">
        {store.listOpen ? null : (
          <IconButton
            label={SHOW_CONVERSATIONS}
            tooltip="bottom-left"
            path={mdiDockLeft}
            iconSize={17}
            className="-ml-2.5 rotate-180"
            onClick={store.toggleList}
          />
        )}
        <h2 className="min-w-0 flex-1 truncate text-[14px] font-medium text-main-100">
          {store.active?.title ?? "Новый диалог"}
        </h2>
        {store.attachedStoreIds.length > 0 ? (
          <span title={`Подключено хранилищ: ${String(store.attachedStoreIds.length)}`}>
            <Chip tone="accent">
              <Icon path={mdiDatabaseOutline} size={12} />
              {store.attachedStoreIds.length}
            </Chip>
          </span>
        ) : null}
        <div
          role="group"
          aria-label={PANEL_GROUP_LABEL}
          className="flex flex-none items-center gap-0.5 rounded-control bg-main-800 p-0.5"
        >
          <PanelButton
            label={CONTEXT_PANEL}
            path={mdiTuneVariant}
            active={store.isPanelShown("context")}
            onClick={() => store.showPanel("context")}
          />
          {store.workingFolder === null ? null : (
            <PanelButton
              label={FILES_PANEL}
              path={mdiFolderOutline}
              active={store.isPanelShown("files")}
              onClick={() => store.showPanel("files")}
            />
          )}
        </div>
        <div
          role="group"
          aria-label={PANEL_GROUP_LABEL}
          className="flex flex-none items-center gap-0.5 rounded-control bg-main-800 p-0.5"
        >
          <PanelButton
            label={TERMINAL_PANEL}
            path={mdiConsole}
            active={store.terminal.open}
            activeClassName="bg-accent-soft text-accent-medium hover:bg-accent-soft hover:text-accent-light"
            onClick={() => void store.terminal.toggle()}
          />
          <MemoryDropdown conversationId={store.active?.id ?? null} />
        </div>
      </header>
      <ScrollArea
        ref={viewport}
        showScrollbar={false}
        onScroll={(event) => {
          const element = event.currentTarget;
          pinned.current = shouldPinToBottom(
            element.scrollTop,
            element.clientHeight,
            element.scrollHeight,
          );
          setJump(!pinned.current);
        }}
        className="min-h-0 flex-1 px-6 pt-7 [overflow-anchor:none]"
      >
        <div ref={content} className="mx-auto w-full max-w-4xl space-y-7 pb-6">
          {store.loading && (
            <p role="status" className="text-[12px] text-main-500">
              Загрузка диалога…
            </p>
          )}
          {!store.loading && !store.active?.messages.length && !store.generating && (
            <div className="flex flex-col items-center py-20 text-center">
              <AssistantAvatar
                size={56}
                className="mb-4 fx-full:drop-shadow-[0_8px_20px_rgb(180_80_20/0.3)]"
              />
              <h2 className="mb-1.5 text-[20px] font-semibold tracking-[-0.02em] text-main-50">
                Начните диалог
              </h2>
              <p className="max-w-100 text-[13px] leading-relaxed text-main-400">
                Выберите модель и подключите векторные хранилища для поиска по документам.
              </p>
            </div>
          )}
          {store.active?.messages.map((message) =>
            message.role === "user" ? (
              <div key={message.id} className="group flex justify-end">
                <div className="flex max-w-[78%] flex-col items-end">
                  {editingId === message.id ? (
                    <div className="w-[min(78vw,620px)] space-y-2 rounded-card bg-main-900 p-2.5 shadow-pop">
                      <TextArea
                        aria-label="Редактировать сообщение"
                        value={editingText}
                        onChange={(event) => setEditingText(event.target.value)}
                        autoFocus
                      />
                      <div className="flex justify-end gap-2">
                        <Button type="button" onClick={() => setEditingId(null)}>
                          Отмена
                        </Button>
                        <Button
                          type="button"
                          tone="primary"
                          disabled={!editingText.trim() || store.generating}
                          onClick={() => {
                            void store
                              .editMessage(message.id, editingText)
                              .then((edited) => edited && setEditingId(null));
                          }}
                        >
                          Отправить
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <>
                      <div className="rounded-[18px] rounded-br-md bg-main-700 px-4 py-2.5 text-[13.5px] leading-relaxed text-main-50 wrap-break-word whitespace-pre-wrap">
                        {message.content}
                      </div>
                    </>
                  )}
                  <div className="mt-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
                    <MessageActions
                      content={message.content}
                      isUser={message.role === "user"}
                      onEdit={() => {
                        setEditingId(message.id);
                        setEditingText(message.content);
                      }}
                      onRefresh={() => void store.refreshMessage(message.id, message.content)}
                      onDelete={() => void store.deleteMessage(message.id)}
                    />
                  </div>
                </div>
              </div>
            ) : (
              <ChatMessage
                key={message.id}
                content={message.content}
                reasoning={message.reasoning}
                citations={answerCitations(message.citations, message.toolCalls)}
                selections={answerSelections.get(message.id)}
                stores={store.stores}
                partial={message.partial}
                tools={<ChatToolCalls calls={message.toolCalls} />}
                artifacts={
                  <ChatArtifacts
                    artifacts={answerArtifacts(message.toolCalls)}
                    actions={store.artifacts}
                  />
                }
                usage={`${message.usageEstimated ? "~" : ""}${message.tokensIn} вход. · ${message.tokensOut} выход. · ${(message.durationMs / 1000).toFixed(1)} с`}
              />
            ),
          )}
          {store.pendingUserText && (
            <div className="flex justify-end">
              <div className="max-w-[78%] animate-zvs-rise rounded-[18px] rounded-br-md bg-main-700 px-4 py-2.5 text-[13.5px] leading-relaxed text-main-50 wrap-break-word whitespace-pre-wrap">
                {store.pendingUserText}
              </div>
            </div>
          )}
          {(store.generating || store.liveText) && (
            <FadeIn>
              <ChatMessage
                content={store.liveText}
                reasoning={store.liveReasoning}
                citations={answerCitations(store.liveCitations, store.liveToolCalls)}
                selections={store.liveSelections}
                stores={store.stores}
                generating={store.generating}
                status={generationPhase(store)}
                artifacts={
                  <ChatArtifacts
                    artifacts={answerArtifacts(store.liveToolCalls)}
                    actions={store.artifacts}
                  />
                }
                partial={store.outcome !== "ok"}
                tools={
                  <ChatToolCalls
                    calls={store.liveToolCalls}
                    approvalFor={store.approvalFor}
                    onDecide={(id, approve) => void store.decideApproval(id, approve)}
                    questionFor={store.questionFor}
                    onAnswer={(id, answers) => void store.answerQuestion(id, answers)}
                    onSkip={(id) => void store.skipQuestion(id)}
                  />
                }
              />
            </FadeIn>
          )}
          {store.outcome === "cancelled" && (
            <p
              role="status"
              className="inline-flex rounded-full bg-warn-soft px-3 py-1 text-[12px] text-warn"
            >
              Генерация остановлена. Частичный ответ сохранён.
            </p>
          )}
          {store.error && (
            <FadeIn>
              <div
                role="alert"
                className="space-y-2.5 rounded-card bg-err-soft p-3.5 text-[13px] text-err"
              >
                <p>{store.error}</p>
                {store.canRetry ? (
                  <Button onClick={store.retry} disabled={!store.available || store.loading}>
                    Повторить сообщение
                  </Button>
                ) : (
                  <Button
                    onClick={() => void store.mount()}
                    disabled={store.generating || store.loading}
                  >
                    Перезагрузить чат
                  </Button>
                )}
              </div>
            </FadeIn>
          )}
        </div>
      </ScrollArea>
      <AnimatePresence>
        {jump && (
          <motion.div
            initial={{ opacity: 0, y: level === "full" ? 10 : 0 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: level === "full" ? 10 : 0 }}
            transition={pop}
            className="pointer-events-none relative z-10 -mt-12 flex justify-center pb-2"
          >
            <span className="pointer-events-auto rounded-control shadow-pop">
              <Button
                onClick={() => {
                  pinned.current = true;
                  setJump(false);
                  scrollLatest();
                }}
              >
                К последнему сообщению ↓
              </Button>
            </span>
          </motion.div>
        )}
      </AnimatePresence>
      <div className="px-6">
        <div className="mx-auto w-full max-w-4xl">
          <ChatApprovals store={store} />
        </div>
      </div>
      <div className="mx-auto w-full max-w-4xl shrink-0 px-6">
        <ChatWorkingFolder store={store} />
      </div>
      <FolderAccessModal store={store} />
      <ChatComposer store={store} />
      <TerminalDock store={store.terminal} available={store.active !== null} />
    </section>
  );
});

function generationPhase(store: ChatStore): string {
  if (store.standaloneApprovals.length > 0 || store.folderRequest !== null)
    return "Ожидает подтверждения";
  if (store.liveToolCalls.some((call) => call.status === "awaiting-approval"))
    return "Ожидает подтверждения";
  if (store.questions.length > 0) return "Ожидает ответа";
  const running = store.liveToolCalls.find((call) => call.status === "running");
  if (running) return toolTitle(running.key);
  if (store.liveText) return "Генерация ответа";
  if (store.liveReasoning) return "Размышления";
  return "Обработка запроса";
}
