import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/buttons/Button";
import {
  mdiConsole,
  mdiDatabaseOutline,
  mdiDockLeft,
  mdiFolderOutline,
  mdiPinOutline,
  mdiTuneVariant,
  mdiWeb,
} from "@mdi/js";
import Chip from "../../ui/atoms/Chip";
import AssistantAvatar from "../../ui/atoms/AssistantAvatar";
import Icon from "../../ui/atoms/Icon";
import IconButton from "../../ui/atoms/buttons/IconButton";
import TextArea from "../../ui/atoms/TextArea";
import ChatMessage from "./ChatMessage";
import MessageActions from "../../ui/molecules/MessageActions";
import ChatComposer from "./ChatComposer";
import type { ExecPolicyBinding } from "../execPolicy/ExecPolicyMenu";
import ChatApprovals from "./ChatApprovals";
import ChatWorkingFolder from "./ChatWorkingFolder";
import FolderAccessModal from "./FolderAccessModal";
import ChatToolCalls from "./ChatToolCalls";
import { answerCitations, questionSelections } from "./citations";
import { answerArtifacts } from "./artifacts";
import ChatArtifacts from "./ChatArtifacts";
import { shouldPinToBottom } from "./scroll";
import { commandText } from "./chatCommands";
import type ChatStore from "./ChatStore";
import { CONTEXT_PANEL, FILES_PANEL, PANEL_GROUP_LABEL } from "./explorer/explorerCopy";
import TerminalDock from "../terminal/TerminalDock";
import { TERMINAL_PANEL } from "../terminal/terminalCopy";
import { BROWSER_PANEL } from "../browser/browserCopy";
import MemoryDropdown from "../memory/MemoryDropdown";
import { AnimatePresence, motion } from "motion/react";
import FadeIn from "../../ui/motion/FadeIn";
import { useFx, useTransition } from "../../ui/motion/fx";
import { toolTitle } from "./toolCallCopy";
import { SHOW_CONVERSATIONS } from "./conversationListCopy";
import Alert from "../../ui/molecules/Alert";
import PanelButton from "./PanelButton";
import Skeleton from "../../ui/atoms/Skeleton";
import { LoadingStatus, SkeletonLines } from "../../ui/molecules/LoadingSkeleton";
import PinnedNavigator from "./PinnedNavigator";
import { PINS_PANEL, currentPin, pinPreview } from "./pinnedMessages";
import { BRANCH_IN_NEW_DIALOG, CREATE_BRANCH } from "./branching";

export default observer(function ChatThread({
  store,
  policy,
}: {
  readonly store: ChatStore;
  readonly policy?: ExecPolicyBinding;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [jump, setJump] = useState(false);
  const answerSelections = questionSelections(store.active?.messages ?? []);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState("");
  const [pinAt, setPinAt] = useState<string | null>(null);
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
  const pinIds = store.pinnedMessages.map((message) => message.id).join(",");
  const trackPin = () => {
    const element = viewport.current;
    if (!element || !pinIds) return;
    const top = element.getBoundingClientRect().top;
    const tops = pinIds.split(",").flatMap((id) => {
      const target = element.querySelector(`[data-message-id="${id}"]`);
      return target ? [{ id, top: target.getBoundingClientRect().top - top }] : [];
    });
    setPinAt(currentPin(tops, element.clientHeight / 3));
  };
  useLayoutEffect(trackPin, [pinIds, store.active?.id]);
  const jumpToPin = (id: string) => {
    viewport.current
      ?.querySelector(`[data-message-id="${id}"]`)
      ?.scrollIntoView({ block: "start", behavior: level === "full" ? "smooth" : "auto" });
    setPinAt(id);
  };
  const artifactTarget = store.artifactTarget;
  useEffect(() => {
    const element = viewport.current;
    if (!artifactTarget || !element) return;
    const message = Array.from(element.querySelectorAll<HTMLElement>("[data-message-id]")).find(
      (node) => node.dataset.messageId === artifactTarget.messageId,
    );
    const tile =
      Array.from(message?.querySelectorAll<HTMLElement>("[data-artifact-id]") ?? []).find(
        (node) => node.dataset.artifactId === artifactTarget.artifactId,
      ) ?? message;
    if (!tile) return;
    pinned.current = false;
    tile.scrollIntoView({ block: "center", behavior: level === "full" ? "smooth" : "auto" });
    if (level !== "none" && typeof tile.animate === "function")
      tile.animate([{ backgroundColor: "var(--color-accent-soft)", offset: 0 }], {
        duration: 1600,
        easing: "ease-out",
      });
    if (window.matchMedia?.("(max-width: 1100px)").matches) store.closePanel();
  }, [artifactTarget, level, store]);
  useEffect(() => {
    if (!content.current || typeof ResizeObserver === "undefined") return;
    const resize = new ResizeObserver(() => {
      if (pinned.current) scrollLatest();
    });
    resize.observe(content.current);
    return () => resize.disconnect();
  }, []);
  return (
    <section
      aria-label="Лента чата"
      className={`flex min-h-0 flex-1 flex-col ${store.isPanelShown("browser") ? "min-w-90" : "min-w-0"}`}
    >
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
          <PanelButton
            label={BROWSER_PANEL}
            path={mdiWeb}
            active={store.isPanelShown("browser")}
            onClick={() => store.showPanel("browser")}
          />
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
          {store.pinnedMessages.length > 0 && (
            <PanelButton
              label={PINS_PANEL}
              path={mdiPinOutline}
              active={store.pinsOpen}
              activeClassName="bg-accent-soft text-accent-medium hover:bg-accent-soft hover:text-accent-light"
              onClick={store.togglePins}
            />
          )}
        </div>
      </header>
      <div className="relative flex min-h-0 flex-1 flex-col">
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
            trackPin();
          }}
          className="min-h-0 flex-1 px-6 pt-7 [overflow-anchor:none]"
        >
          <div ref={content} className="mx-auto w-full max-w-4xl space-y-7 pb-6">
            {store.loading && (
              <LoadingStatus label="Загрузка диалога…" className="space-y-7">
                <div className="flex justify-end">
                  <Skeleton rounded="xl" className="h-10 w-[46%]" />
                </div>
                <div className="flex gap-3">
                  <Skeleton rounded="full" className="size-7 flex-none" />
                  <SkeletonLines count={4} className="flex-1 pt-1" />
                </div>
                <div className="flex justify-end">
                  <Skeleton rounded="xl" className="h-10 w-[32%]" />
                </div>
              </LoadingStatus>
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
                          <Button onClick={() => setEditingId(null)}>Отмена</Button>
                          <Button
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
                        {message.command !== undefined && (
                          <span className="mb-1 font-mono text-[11.5px] text-main-400">
                            {commandText(message.command)}
                          </span>
                        )}
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
                <div key={message.id} data-message-id={message.id} className="scroll-mt-6">
                  <ChatMessage
                    messageId={message.id}
                    content={message.content}
                    reasoning={message.reasoning}
                    rounds={message.rounds}
                    citations={answerCitations(message.citations, message.toolCalls)}
                    webSources={message.webSources}
                    selections={answerSelections.get(message.id)}
                    stores={store.stores}
                    partial={message.partial}
                    toolCalls={message.toolCalls}
                    renderTools={(calls) => <ChatToolCalls calls={calls} />}
                    artifacts={
                      <ChatArtifacts
                        artifacts={answerArtifacts(message.toolCalls)}
                        actions={store.artifacts}
                      />
                    }
                    usage={`${message.usageEstimated ? "~" : ""}${message.tokensIn} вход. · ${message.tokensOut} выход. · ${(message.durationMs / 1000).toFixed(1)} с`}
                    pinned={message.pinned}
                    onPin={() => void store.pinMessage(message.id, !message.pinned)}
                    branch={{
                      label: CREATE_BRANCH,
                      newDialogLabel: BRANCH_IN_NEW_DIALOG,
                      disabled: store.generating || store.branching,
                      onNewDialog: () => void store.branchConversation(message.id),
                    }}
                  />
                </div>
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
                  rounds={store.liveRounds}
                  citations={answerCitations(store.liveCitations, store.liveToolCalls)}
                  webSources={store.liveWebSources}
                  selections={store.liveSelections}
                  stores={store.stores}
                  generating={store.generating}
                  status={generationPhase(store)}
                  since={store.runStartedAt}
                  artifacts={
                    <ChatArtifacts
                      artifacts={answerArtifacts(store.liveToolCalls)}
                      actions={store.artifacts}
                    />
                  }
                  partial={store.outcome !== "ok"}
                  toolCalls={store.liveToolCalls}
                  renderTools={(calls) => (
                    <ChatToolCalls
                      calls={calls}
                      approvalFor={store.approvalFor}
                      onDecide={(id, approve) => void store.decideApproval(id, approve)}
                      questionFor={store.questionFor}
                      onAnswer={(id, answers) => void store.answerQuestion(id, answers)}
                      onSkip={(id) => void store.skipQuestion(id)}
                    />
                  )}
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
                <Alert
                  title={store.error}
                  action={
                    store.canRetry ? (
                      <Button
                        size="sm"
                        onClick={store.retry}
                        disabled={!store.available || store.loading}
                      >
                        Повторить сообщение
                      </Button>
                    ) : (
                      <Button
                        size="sm"
                        onClick={() => void store.mount()}
                        disabled={store.generating || store.loading}
                      >
                        Перезагрузить чат
                      </Button>
                    )
                  }
                />
              </FadeIn>
            )}
          </div>
        </ScrollArea>
        {store.pinsShown && (
          <PinnedNavigator
            items={store.pinnedMessages.map((message) => ({
              id: message.id,
              label: pinPreview(message.content),
            }))}
            current={pinAt}
            onSelect={jumpToPin}
          />
        )}
      </div>
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
      <ChatComposer store={store} {...(policy === undefined ? {} : { policy })} />
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
