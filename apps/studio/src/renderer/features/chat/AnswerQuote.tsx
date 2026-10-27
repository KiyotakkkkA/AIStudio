import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { DocumentSelectionDto, MessageId } from "@zvs/shared";
import { mdiCommentQuoteOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";
import { answerQuote } from "./answerQuoteModel";
import { ASK_IN_CHAT } from "./selectionCopy";

interface Anchor {
  readonly selection: DocumentSelectionDto;
  readonly x: number;
  readonly y: number;
}

export default function AnswerQuote({
  messageId,
  children,
}: {
  readonly messageId: MessageId;
  readonly children: ReactNode;
}) {
  const { chat } = useStore();
  const root = useRef<HTMLDivElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  const [anchor, setAnchor] = useState<Anchor | null>(null);

  useEffect(() => {
    let frame = 0;
    let pressed = false;
    const measure = () => {
      const element = root.current;
      const current = document.getSelection();
      if (element === null || current === null || current.rangeCount === 0) return null;
      const range = current.getRangeAt(0);
      const selection = answerQuote(element, range, messageId);
      if (selection === null) return null;
      const rect = range.getBoundingClientRect();
      return { selection, x: rect.left + rect.width / 2, y: rect.top };
    };
    const capture = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setAnchor(pressed ? null : measure()));
    };
    const press = (event: PointerEvent) => {
      if (popover.current?.contains(event.target as Node)) return;
      pressed = true;
      setAnchor(null);
    };
    const release = () => {
      pressed = false;
      capture();
    };
    document.addEventListener("selectionchange", capture);
    document.addEventListener("pointerdown", press);
    document.addEventListener("pointerup", release);
    window.addEventListener("scroll", capture, true);
    window.addEventListener("resize", capture);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("selectionchange", capture);
      document.removeEventListener("pointerdown", press);
      document.removeEventListener("pointerup", release);
      window.removeEventListener("scroll", capture, true);
      window.removeEventListener("resize", capture);
    };
  }, [messageId]);

  const ask = () => {
    if (anchor === null) return;
    chat.attachSelection(anchor.selection);
    document.getSelection()?.removeAllRanges();
    setAnchor(null);
  };

  return (
    <div ref={root}>
      {children}
      {anchor === null
        ? null
        : createPortal(
            <div
              ref={popover}
              style={{ left: anchor.x, top: anchor.y }}
              className="fixed z-50 -translate-x-1/2 -translate-y-[calc(100%+8px)] animate-zvs-rise"
            >
              <button
                type="button"
                onPointerDown={(event) => event.preventDefault()}
                onClick={ask}
                className="inline-flex h-8 items-center gap-1.5 rounded-full border border-line bg-main-900 px-3 text-[12.5px] font-medium whitespace-nowrap text-main-100 shadow-pop transition-colors hover:bg-main-800 hover:text-accent-medium"
              >
                <Icon path={mdiCommentQuoteOutline} size={15} />
                {ASK_IN_CHAT}
              </button>
            </div>,
            document.body,
          )}
    </div>
  );
}
