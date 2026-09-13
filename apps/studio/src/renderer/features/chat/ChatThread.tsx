import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import ChatMessage from "./ChatMessage";
import ChatComposer from "./ChatComposer";
import ChatApprovals from "./ChatApprovals";
import { shouldPinToBottom } from "./scroll";
import type ChatStore from "./ChatStore";

export default observer(function ChatThread({ store }: { readonly store: ChatStore }) {
  const viewport = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const [jump, setJump] = useState(false);
  const scrollLatest = () => {
    const element = viewport.current;
    if (element) element.scrollTop = element.scrollHeight;
  };
  useLayoutEffect(() => {
    pinned.current = true;
    setJump(false);
    scrollLatest();
  }, [store.active?.id]);
  useLayoutEffect(() => {
    if (pinned.current) scrollLatest();
  }, [store.liveText, store.active?.messages.length, store.generating, store.liveCitations.length]);
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
      <header className="shrink-0 border-b border-main-700 px-5">
        <div className="mx-auto flex h-header w-full max-w-4xl items-center gap-3">
          <h1 className="min-w-0 flex-1 truncate text-sm font-semibold">
            {store.active?.title ?? "Новый чат"}
          </h1>
          <Chip>{store.attachedStoreIds.length} хр.</Chip>
          <span className="min-[1101px]:hidden">
            <Button onClick={store.toggleContext}>Контекст</Button>
          </span>
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
        className="min-h-0 flex-1 overflow-y-auto px-5 pt-6 [overflow-anchor:none]"
      >
        <div ref={content} className="mx-auto w-full max-w-4xl space-y-6 pb-5">
          {store.loading && (
            <p role="status" className="text-xs text-main-400">
              Загрузка диалога…
            </p>
          )}
          {!store.loading && !store.active?.messages.length && !store.generating && (
            <div className="py-16 text-center">
              <h2 className="mb-2 text-lg font-medium">Что хотите исследовать?</h2>
              <p className="text-xs text-main-400">
                Выберите модель и подключите хранилища, чтобы отвечать на основе ваших документов.
              </p>
            </div>
          )}
          {store.active?.messages.map((message) =>
            message.role === "user" ? (
              <div key={message.id} className="flex justify-end">
                <div className="max-w-[78%] rounded-xl rounded-br-sm border border-main-600 bg-main-700 px-3.5 py-3 text-[13.5px] leading-relaxed wrap-break-word whitespace-pre-wrap">
                  {message.content}
                </div>
              </div>
            ) : (
              <ChatMessage
                key={message.id}
                content={message.content}
                citations={message.citations}
                storeIds={store.active?.attachedStoreIds ?? []}
                stores={store.stores}
                partial={message.partial}
                usage={`${message.usageEstimated ? "~" : ""}${message.tokensIn} вход. · ${message.tokensOut} выход. · ${(message.durationMs / 1000).toFixed(1)} с`}
              />
            ),
          )}
          {store.pendingUserText && (
            <div className="flex justify-end">
              <div className="max-w-[78%] rounded-xl rounded-br-sm border border-main-600 bg-main-700 px-3.5 py-3 wrap-break-word whitespace-pre-wrap">
                {store.pendingUserText}
              </div>
            </div>
          )}
          {(store.generating || store.liveText) && (
            <ChatMessage
              content={store.liveText}
              citations={store.liveCitations}
              storeIds={store.searchedStoreIds}
              stores={store.stores}
              generating={store.generating}
              partial={store.outcome !== "ok"}
            />
          )}
          {store.outcome === "cancelled" && (
            <p role="status" className="text-xs text-warn">
              Генерация остановлена. Частичный ответ сохранён.
            </p>
          )}
          {store.error && (
            <div
              role="alert"
              className="space-y-2 rounded-card border border-err-border p-3 text-xs text-err"
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
          )}
        </div>
      </ScrollArea>
      {jump && (
        <div className="flex justify-center py-2">
          <Button
            onClick={() => {
              pinned.current = true;
              setJump(false);
              scrollLatest();
            }}
          >
            К последнему сообщению ↓
          </Button>
        </div>
      )}
      <div className="px-5">
        <div className="mx-auto w-full max-w-4xl">
          <ChatApprovals store={store} />
        </div>
      </div>
      <ChatComposer store={store} />
    </section>
  );
});
