import {
  ChatCitationDto,
  type ChatToolCallDto,
  type DocumentId,
  type DocumentSelectionDto,
} from "@zvs/shared";
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
  readonly citation: ChatCitationDto;
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
    if (
      citation.page !== null &&
      (entry.citation.page === null || citation.page < entry.citation.page)
    )
      entry.citation = citation;
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
    citation: entry.citation,
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

export const OPEN_CITATION = "Открыть документ";

export function citationLocation(citation: ChatCitationDto, store: string): string {
  return [
    store,
    ...(citation.page === null ? [] : [`стр. ${citation.page.toLocaleString("ru-RU")}`]),
    ...(citation.section === "" ? [] : [citation.section]),
  ].join(" · ");
}

export const SELECTIONS_TAB = "Выделенные фрагменты";
export const SELECTION_ORIGIN = "выделено в документе";
export const OPEN_SELECTION = "Открыть фрагмент в документе";

export type DocumentQuote = DocumentSelectionDto & { readonly documentId: DocumentId };

export function documentSelections(selections: readonly DocumentSelectionDto[]): DocumentQuote[] {
  return selections.filter(
    (selection): selection is DocumentQuote => selection.documentId !== null,
  );
}

export interface SelectionSource {
  readonly key: string;
  readonly name: string;
  readonly pages: readonly number[];
  readonly fragments: number;
  readonly selection: DocumentQuote;
}

export function selectionSources(selections: readonly DocumentSelectionDto[]): SelectionSource[] {
  const grouped = new Map<
    string,
    { selection: DocumentQuote; pages: Set<number>; count: number }
  >();
  for (const selection of documentSelections(selections)) {
    const entry = grouped.get(selection.documentId) ?? {
      selection,
      pages: new Set<number>(),
      count: 0,
    };
    if (selection.page !== null) entry.pages.add(selection.page);
    if (
      selection.page !== null &&
      (entry.selection.page === null || selection.page < entry.selection.page)
    )
      entry.selection = selection;
    entry.count += 1;
    grouped.set(selection.documentId, entry);
  }
  return [...grouped.entries()].map(([key, entry]) => ({
    key: `selection:${key}`,
    name: entry.selection.name,
    pages: [...entry.pages].sort((left, right) => left - right),
    fragments: entry.count,
    selection: entry.selection,
  }));
}

export function selectionDetail(source: SelectionSource): string {
  return [
    SELECTION_ORIGIN,
    ...(source.pages.length === 0 ? [] : [`стр. ${source.pages.join(", ")}`]),
  ].join(" · ");
}

export function selectionLocation(selection: DocumentSelectionDto): string {
  return selection.page === null
    ? SELECTION_ORIGIN
    : `${SELECTION_ORIGIN} · стр. ${selection.page.toLocaleString("ru-RU")}`;
}

export function questionSelections(
  messages: readonly {
    readonly id: string;
    readonly role: string;
    readonly selections: readonly DocumentSelectionDto[];
  }[],
): Map<string, readonly DocumentSelectionDto[]> {
  const byAnswer = new Map<string, readonly DocumentSelectionDto[]>();
  let question: readonly DocumentSelectionDto[] = [];
  for (const message of messages) {
    if (message.role === "user") question = message.selections;
    else byAnswer.set(message.id, question);
  }
  return byAnswer;
}
