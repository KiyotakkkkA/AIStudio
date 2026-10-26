import type { ChatCitationDto, DocumentSelectionDto, WebSourceDto } from "@zvs/shared";
import {
  SOURCES_SHOWN,
  SOURCES_TITLE,
  fragmentsLabel,
  selectionDetail,
  sourceDetail,
  type CitationSource,
  type SelectionSource,
} from "./citations";
import SiteIcon from "./SiteIcon";
import { webSourceDetail } from "./webSources";

type SourceRow =
  | { readonly kind: "selection"; readonly key: string; readonly source: SelectionSource }
  | { readonly kind: "citation"; readonly key: string; readonly source: CitationSource }
  | { readonly kind: "web"; readonly key: string; readonly source: WebSourceDto };

const LINK =
  "cursor-pointer text-left text-accent-medium underline decoration-accent-line underline-offset-3 hover:decoration-accent-medium";

export default function ChatSources({
  sources,
  selections,
  web,
  fragments,
  onOpen,
  onView,
  onViewSelection,
  onOpenWeb,
}: {
  readonly sources: readonly CitationSource[];
  readonly selections: readonly SelectionSource[];
  readonly web: readonly WebSourceDto[];
  readonly fragments: number;
  readonly onOpen: () => void;
  readonly onView: (citation: ChatCitationDto) => void;
  readonly onViewSelection: (selection: DocumentSelectionDto) => void;
  readonly onOpenWeb: (source: WebSourceDto) => void;
}) {
  const rows: SourceRow[] = [
    ...selections.map((source) => ({ kind: "selection" as const, key: source.key, source })),
    ...sources.map((source) => ({ kind: "citation" as const, key: source.key, source })),
    ...web.map((source, index) => ({
      kind: "web" as const,
      key: `web:${String(index)}:${source.url}`,
      source,
    })),
  ];
  if (rows.length === 0) return null;
  const shown = rows.slice(0, SOURCES_SHOWN);
  const hidden = rows.length - shown.length;
  return (
    <section aria-label={SOURCES_TITLE} className="flex flex-col gap-1.5">
      <h3 className="m-0 text-[13px] font-semibold text-main-200">{SOURCES_TITLE}:</h3>
      <ul className="m-0 flex list-disc flex-col gap-1 pl-5 marker:text-main-500">
        {shown.map((row) => (
          <li key={row.key} className="text-[13px] leading-snug">
            {row.kind === "web" ? (
              <button
                type="button"
                title={row.source.url}
                className={LINK}
                onClick={() => onOpenWeb(row.source)}
              >
                <span className="mr-1.5 inline-block">
                  <SiteIcon source={row.source} />
                </span>
                {row.source.title}
                <span className="inline-block pl-1 text-main-500">
                  — {webSourceDetail(row.source)}
                </span>
              </button>
            ) : (
              <button
                type="button"
                title={row.kind === "citation" ? row.source.path : row.source.name}
                className={LINK}
                onClick={() =>
                  row.kind === "citation"
                    ? onView(row.source.citation)
                    : onViewSelection(row.source.selection)
                }
              >
                {row.source.name}
                <span className="inline-block pl-1 text-main-500">
                  —{" "}
                  {row.kind === "citation" ? sourceDetail(row.source) : selectionDetail(row.source)}
                </span>
              </button>
            )}
          </li>
        ))}
      </ul>
      <button
        type="button"
        className="self-start text-[12px] text-main-500 hover:text-main-200"
        onClick={onOpen}
      >
        {fragmentsLabel(fragments, hidden)}
      </button>
    </section>
  );
}
