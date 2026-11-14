import type {
  ChatCitationDto,
  ChatSourceDto,
  ChatSourceKind,
  NoteSourceDto,
  VectorStoreId,
  WebSourceDto,
} from "@zvs/shared";

type SourceOf<K extends ChatSourceKind> = Extract<ChatSourceDto, { kind: K }>;

export function sourcesOfKind<K extends ChatSourceKind>(
  sources: readonly ChatSourceDto[],
  kind: K,
): SourceOf<K>[] {
  return sources.filter((source): source is SourceOf<K> => source.kind === kind);
}

function withoutKind<T extends { readonly kind: ChatSourceKind }>(source: T): Omit<T, "kind"> {
  const { kind, ...rest } = source;
  void kind;
  return rest;
}

export function vectorCitations(sources: readonly ChatSourceDto[]): ChatCitationDto[] {
  return sourcesOfKind(sources, "vector").map(withoutKind);
}

export function webSourceList(sources: readonly ChatSourceDto[]): WebSourceDto[] {
  return sourcesOfKind(sources, "web").map(withoutKind);
}

export function noteSourceList(sources: readonly ChatSourceDto[]): NoteSourceDto[] {
  return sourcesOfKind(sources, "note").map(withoutKind);
}

export function withStoreCitations(
  sources: readonly ChatSourceDto[],
  storeId: VectorStoreId,
  citations: readonly ChatCitationDto[],
): ChatSourceDto[] {
  return [
    ...sources.filter((source) => source.kind !== "vector" || source.storeId !== storeId),
    ...citations.map((citation) => ({ kind: "vector" as const, ...citation })),
  ];
}
