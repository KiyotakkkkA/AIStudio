import {
  Children,
  createContext,
  isValidElement,
  useContext,
  useMemo,
  type ComponentProps,
  type ReactNode,
} from "react";
import ReactMarkdown, { type Components, type ExtraProps } from "react-markdown";
import remarkGfm from "remark-gfm";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { CodeView } from "@kiyotakkkka/zvs-uikit-lib/code-view";
import type { ChatCitationDto, WebSourceDto } from "@zvs/shared";
import { citationNumber, linkCitations, linkWebSources, webSourceNumber } from "./citationMarkers";
import { webSourceDetail } from "./webSources";
import { isFenceOpen } from "./codeFence";

interface MarkdownSource {
  readonly text: string;
  readonly streaming: boolean;
  readonly citations: readonly ChatCitationDto[];
  readonly webSources: readonly WebSourceDto[];
  readonly onOpenWeb?: (source: WebSourceDto) => void;
  readonly onCite?: (citation: ChatCitationDto) => void;
}

const SourceContext = createContext<MarkdownSource>({
  text: "",
  streaming: false,
  citations: [],
  webSources: [],
});

const MARKER_CHIP =
  "mx-0.5 inline-flex h-4.5 min-w-4.5 -translate-y-px items-center justify-center rounded-full bg-accent-soft px-1 align-middle font-mono text-[10.5px] font-semibold text-accent-medium no-underline tabular-nums";

function PlainCode({ code }: { readonly code: string }) {
  return (
    <pre className="m-0 font-mono text-xs leading-relaxed whitespace-pre text-main-50">
      <code>{code}</code>
    </pre>
  );
}

function StreamingCode({ code, language }: { readonly code: string; readonly language: string }) {
  return (
    <div className="overflow-hidden rounded-xl border border-main-700 bg-main-900">
      <div className="flex min-h-10 items-center border-b border-main-700 bg-main-900 px-3 py-2">
        <span className="shrink-0 rounded-md bg-main-800 px-1.5 py-0.5 text-[11px] font-medium tracking-wide text-main-400 uppercase">
          {language}
        </span>
      </div>
      <div className="p-3">
        <PlainCode code={code} />
      </div>
    </div>
  );
}

function CodeBlock({ children, node }: ComponentProps<"pre"> & ExtraProps) {
  const source = useContext(SourceContext);
  const child = Children.toArray(children)[0];
  if (!isValidElement<{ children?: ReactNode; className?: string }>(child))
    return <pre>{children}</pre>;
  const code = String(child.props.children ?? "").replace(/\n$/, "");
  const language = /language-([\w+-]+)/.exec(child.props.className ?? "")?.[1] ?? "text";
  const open =
    source.streaming &&
    isFenceOpen(source.text, node?.position?.start.offset, node?.position?.end.offset);
  return (
    <ScrollArea orientation="horizontal" className="min-w-0 max-w-full">
      {open ? (
        <StreamingCode code={code} language={language} />
      ) : (
        <CodeView code={code} language={language} downloadable={false}>
          <CodeView.Header />
          <CodeView.Content loadingFallback={<PlainCode code={code} />} />
        </CodeView>
      )}
    </ScrollArea>
  );
}

function Link({ children, href }: ComponentProps<"a"> & ExtraProps) {
  const { citations, webSources, onCite, onOpenWeb } = useContext(SourceContext);
  const webNumber = webSourceNumber(href);
  const webSource = webNumber === null ? undefined : webSources[webNumber - 1];
  if (webNumber !== null && webSource !== undefined)
    return (
      <button
        type="button"
        title={`${webSource.title}\n${webSourceDetail(webSource)}`}
        aria-label={`Источник ${String(webNumber)}: ${webSource.title}`}
        className={`${MARKER_CHIP} hover:bg-accent-dark hover:text-on-accent`}
        onClick={() => onOpenWeb?.(webSource)}
      >
        {webNumber}
      </button>
    );
  const number = citationNumber(href);
  const citation = number === null ? undefined : citations[number - 1];
  if (number !== null && citation !== undefined)
    return (
      <button
        type="button"
        title={citation.sourcePath}
        aria-label={`Источник ${String(number)}: ${citation.sourcePath}`}
        className={`${MARKER_CHIP} hover:bg-accent-dark hover:text-on-accent`}
        onClick={() => onCite?.(citation)}
      >
        {number}
      </button>
    );
  return <span className="text-accent-medium">{children}</span>;
}

function Picture({ alt }: ComponentProps<"img"> & ExtraProps) {
  return <span>{alt}</span>;
}

const COMPONENTS: Components = { pre: CodeBlock, a: Link, img: Picture };

const REMARK_PLUGINS = [remarkGfm];

export default function ChatMarkdown({
  content,
  streaming = false,
  citations = [],
  webSources = [],
  onCite,
  onOpenWeb,
}: {
  readonly content: string;
  readonly streaming?: boolean;
  readonly citations?: readonly ChatCitationDto[];
  readonly webSources?: readonly WebSourceDto[];
  readonly onCite?: (citation: ChatCitationDto) => void;
  readonly onOpenWeb?: (source: WebSourceDto) => void;
}) {
  const text = linkCitations(linkWebSources(content, webSources.length), citations.length);
  const source = useMemo(
    () => ({ text, streaming, citations, webSources, onCite, onOpenWeb }),
    [text, streaming, citations, webSources, onCite, onOpenWeb],
  );
  return (
    <div
      className={`${streaming ? "*:animate-zvs-rise " : ""}min-w-0 space-y-3 text-[13.5px] leading-relaxed wrap-break-word text-main-100 [&_a]:text-accent-medium [&_a]:underline [&_a]:decoration-accent-line [&_a]:underline-offset-2 [&_h1]:text-[20px] [&_h1]:font-semibold [&_h1]:tracking-[-0.02em] [&_h2]:text-[17px] [&_h2]:font-semibold [&_h2]:tracking-[-0.01em] [&_h3]:font-semibold [&_hr]:border-line [&_li]:my-1 [&_li]:marker:text-main-500 [&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5 [&_blockquote]:border-l-2 [&_blockquote]:border-accent-dark [&_blockquote]:pl-3.5 [&_blockquote]:text-main-300 [&_table]:w-full [&_table]:border-collapse [&_th]:border-b [&_th]:border-line [&_th]:px-2 [&_th]:py-1.5 [&_th]:text-left [&_th]:bg-main-850 [&_th]:text-[12px] [&_th]:font-semibold [&_th]:text-main-300 [&_td]:border-b [&_td]:border-line [&_td]:px-2 [&_td]:py-1.5 [&_td]:align-top [&_td]:text-[13px] [&_tr:last-child_td]:border-b-0`}
    >
      <SourceContext.Provider value={source}>
        <ReactMarkdown skipHtml remarkPlugins={REMARK_PLUGINS} components={COMPONENTS}>
          {text}
        </ReactMarkdown>
      </SourceContext.Provider>
    </div>
  );
}
