import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { CodeView } from "@kiyotakkkka/zvs-uikit-lib/code-view";
import type ViewerStore from "../stores/ViewerStore";
import ChatMarkdown from "../features/chat/ChatMarkdown";
import GridView from "./GridView";
import { csvDelimiter, parseCsv } from "./csv";
import { EMPTY_DOCUMENT } from "./viewerCopy";
import { useDocumentText } from "./useDocumentText";

export const HIGHLIGHT_LIMIT = 400_000;

function useContentVersion(root: RefObject<HTMLElement | null>): number {
  const [version, setVersion] = useState(0);
  useEffect(() => {
    const element = root.current;
    if (element === null) return;
    let frame = 0;
    const observer = new MutationObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setVersion((value) => value + 1));
    });
    observer.observe(element, { childList: true, subtree: true, characterData: true });
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [root]);
  return version;
}

const DomText = observer(function DomText({
  viewer,
  text,
  kind,
  language,
}: {
  readonly viewer: ViewerStore;
  readonly text: string;
  readonly kind: "markdown" | "code" | "text";
  readonly language: string | null;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const version = useContentVersion(rootRef);
  useDocumentText(viewer, rootRef, scrollerRef, version);
  const highlighted = kind === "code" && text.length <= HIGHLIGHT_LIMIT;

  return (
    <ScrollArea ref={scrollerRef} orientation="both" className="min-h-0 flex-1 bg-main-900">
      <div
        ref={rootRef}
        className="mx-auto w-full px-6 pt-5 pb-10"
        style={{
          fontSize: `${String(Math.round(viewer.zoom * 100))}%`,
          maxWidth: kind === "markdown" ? "880px" : undefined,
        }}
      >
        {kind === "markdown" ? (
          <ChatMarkdown content={text} />
        ) : highlighted ? (
          <CodeView code={text} language={language ?? "text"} downloadable={false} />
        ) : (
          <pre className="m-0 font-mono text-[12.5px] leading-[1.6] whitespace-pre-wrap text-main-100">
            {text}
          </pre>
        )}
      </div>
    </ScrollArea>
  );
});

function TextView({
  viewer,
  text,
  kind,
  language,
  mimeType,
}: {
  readonly viewer: ViewerStore;
  readonly text: string;
  readonly kind: "markdown" | "code" | "csv" | "text";
  readonly language: string | null;
  readonly mimeType: string;
}) {
  const rows = useMemo(() => {
    if (kind !== "csv") return null;
    const firstLine = text.slice(0, text.indexOf("\n") === -1 ? undefined : text.indexOf("\n"));
    return parseCsv(text, csvDelimiter(firstLine, mimeType));
  }, [kind, text, mimeType]);

  if (text.length === 0)
    return <p className="m-auto px-6 text-center text-[13px] text-main-400">{EMPTY_DOCUMENT}</p>;
  if (rows !== null) return <GridView viewer={viewer} rows={rows} source={text} />;
  return (
    <DomText
      viewer={viewer}
      text={text}
      kind={kind === "csv" ? "text" : kind}
      language={language}
    />
  );
}

export default observer(TextView);
