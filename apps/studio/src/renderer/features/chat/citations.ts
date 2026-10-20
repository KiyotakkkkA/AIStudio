import { ChatCitationDto, type ChatToolCallDto } from "@zvs/shared";

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
