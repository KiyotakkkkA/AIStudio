import type { ChatCitationDto } from "@zvs/shared";
import {
  SOURCES_SHOWN,
  SOURCES_TITLE,
  fragmentsLabel,
  sourceDetail,
  type CitationSource,
} from "./citations";

export default function ChatSources({
  sources,
  fragments,
  onOpen,
  onView,
}: {
  readonly sources: readonly CitationSource[];
  readonly fragments: number;
  readonly onOpen: (storeId?: ChatCitationDto["storeId"]) => void;
  readonly onView: (citation: ChatCitationDto) => void;
}) {
  if (sources.length === 0) return null;
  const shown = sources.slice(0, SOURCES_SHOWN);
  const hidden = sources.length - shown.length;
  return (
    <section aria-label={SOURCES_TITLE} className="flex flex-col gap-1.5">
      <h3 className="m-0 text-[13px] font-semibold text-main-200">{SOURCES_TITLE}:</h3>
      <ul className="m-0 flex list-disc flex-col gap-1 pl-5 marker:text-main-500">
        {shown.map((source) => (
          <li key={source.key} className="text-[13px] leading-snug">
            <button
              type="button"
              title={source.path}
              className="cursor-pointer text-left text-accent-medium underline decoration-accent-line underline-offset-3 hover:decoration-accent-medium"
              onClick={() => onView(source.citation)}
            >
              {source.name}
              <span className="inline-block pl-1 text-main-500">— {sourceDetail(source)}</span>
            </button>
          </li>
        ))}
      </ul>
      <button
        type="button"
        className="self-start text-[12px] text-main-500 hover:text-main-200"
        onClick={() => onOpen()}
      >
        {fragmentsLabel(fragments, hidden)}
      </button>
    </section>
  );
}
