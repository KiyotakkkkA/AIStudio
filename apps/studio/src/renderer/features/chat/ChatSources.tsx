import type { ChatCitationDto, DocumentSelectionDto } from "@zvs/shared";
import {
  SOURCES_SHOWN,
  SOURCES_TITLE,
  fragmentsLabel,
  selectionDetail,
  sourceDetail,
  type CitationSource,
  type SelectionSource,
} from "./citations";

type SourceRow =
  | { readonly kind: "selection"; readonly source: SelectionSource }
  | { readonly kind: "citation"; readonly source: CitationSource };

export default function ChatSources({
  sources,
  selections,
  fragments,
  onOpen,
  onView,
  onViewSelection,
}: {
  readonly sources: readonly CitationSource[];
  readonly selections: readonly SelectionSource[];
  readonly fragments: number;
  readonly onOpen: () => void;
  readonly onView: (citation: ChatCitationDto) => void;
  readonly onViewSelection: (selection: DocumentSelectionDto) => void;
}) {
  const rows: SourceRow[] = [
    ...selections.map((source) => ({ kind: "selection" as const, source })),
    ...sources.map((source) => ({ kind: "citation" as const, source })),
  ];
  if (rows.length === 0) return null;
  const shown = rows.slice(0, SOURCES_SHOWN);
  const hidden = rows.length - shown.length;
  return (
    <section aria-label={SOURCES_TITLE} className="flex flex-col gap-1.5">
      <h3 className="m-0 text-[13px] font-semibold text-main-200">{SOURCES_TITLE}:</h3>
      <ul className="m-0 flex list-disc flex-col gap-1 pl-5 marker:text-main-500">
        {shown.map((row) => (
          <li key={row.source.key} className="text-[13px] leading-snug">
            <button
              type="button"
              title={row.kind === "citation" ? row.source.path : row.source.name}
              className="cursor-pointer text-left text-accent-medium underline decoration-accent-line underline-offset-3 hover:decoration-accent-medium"
              onClick={() =>
                row.kind === "citation"
                  ? onView(row.source.citation)
                  : onViewSelection(row.source.selection)
              }
            >
              {row.source.name}
              <span className="inline-block pl-1 text-main-500">
                — {row.kind === "citation" ? sourceDetail(row.source) : selectionDetail(row.source)}
              </span>
            </button>
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
