import { ChatCitationDto, type ChatToolCallDto } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

const ToolCitations = ChatCitationDto.array();

export function answerCitations(
  citations: readonly ChatCitationDto[],
  calls: readonly ChatToolCallDto[],
): ChatCitationDto[] {
  const merged = [...citations];
  const known = new Set(merged.map(citationKey));
  for (const call of calls) {
    if (call.status !== "succeeded") continue;
    const meta = call.meta;
    const listed =
      typeof meta === "object" && meta !== null && !Array.isArray(meta) ? meta.citations : null;
    const parsed = ToolCitations.safeParse(listed);
    if (!parsed.success) continue;
    for (const citation of parsed.data) {
      const key = citationKey(citation);
      if (known.has(key)) continue;
      known.add(key);
      merged.push(citation);
    }
  }
  return merged;
}

function citationKey(citation: ChatCitationDto): string {
  return `${citation.storeId}:${citation.documentId}:${String(citation.chunkIndex)}`;
}

export const SOURCES_TITLE = "Источники";
export const SOURCES_SHOWN = 6;
export const ALL_FRAGMENTS = "Все фрагменты";

export interface CitationSource {
  readonly key: string;
  readonly storeId: ChatCitationDto["storeId"];
  readonly name: string;
  readonly path: string;
  readonly store: string;
  readonly pages: readonly number[];
  readonly fragments: number;
}

export function sourceName(path: string): string {
  const parts = path.split(/[\\/]/u).filter((part) => part.length > 0);
  return parts.at(-1) ?? path;
}

export function citationSources(
  citations: readonly ChatCitationDto[],
  storeName: (storeId: ChatCitationDto["storeId"]) => string,
): CitationSource[] {
  const grouped = new Map<
    string,
    { citation: ChatCitationDto; pages: Set<number>; count: number }
  >();
  for (const citation of citations) {
    const key = `${citation.storeId}:${citation.documentId}`;
    const entry = grouped.get(key) ?? { citation, pages: new Set<number>(), count: 0 };
    if (citation.page !== null) entry.pages.add(citation.page);
    entry.count += 1;
    grouped.set(key, entry);
  }
  return [...grouped.entries()].map(([key, entry]) => ({
    key,
    storeId: entry.citation.storeId,
    name: sourceName(entry.citation.sourcePath),
    path: entry.citation.sourcePath,
    store: storeName(entry.citation.storeId),
    pages: [...entry.pages].sort((left, right) => left - right),
    fragments: entry.count,
  }));
}

export function sourceDetail(source: CitationSource): string {
  return [
    source.store,
    ...(source.pages.length === 0 ? [] : [`стр. ${source.pages.join(", ")}`]),
  ].join(" · ");
}

export function fragmentsLabel(fragments: number, hidden: number): string {
  const all = `${ALL_FRAGMENTS} (${fragments.toLocaleString("ru-RU")})`;
  if (hidden <= 0) return all;
  return `${all} · ещё ${plural(hidden, "источник", "источника", "источников")}`;
}
