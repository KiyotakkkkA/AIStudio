import type { ChatToolCallDto, Json } from "@zvs/shared";
import { FILE_TOOL_APPLY, changesSummary, fileChanges } from "./fileChangesCopy";

export function toolArgumentsSummary(value: Json): string {
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 80 ? `${text.slice(0, 80)}…` : text;
}

function field(value: Json, name: string): Json | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value[name]
    : undefined;
}

function foundCount(call: ChatToolCallDto): number | null {
  const citations = field(call.meta, "citations");
  return Array.isArray(citations) ? citations.length : null;
}

export function toolCallSummary(call: ChatToolCallDto): string {
  if (call.key === "knowledge.search") {
    const query = field(call.arguments, "query");
    if (typeof query === "string") {
      const found = foundCount(call);
      return found === null ? `«${query}»` : `«${query}» · найдено фрагментов: ${String(found)}`;
    }
  }
  if (call.key === "knowledge.read") {
    const section = field(call.arguments, "section");
    if (typeof section === "string" && section.length > 0) return `раздел «${section}»`;
    const from = field(call.arguments, "fromChunk");
    const to = field(call.arguments, "toChunk");
    if (typeof from === "number")
      return typeof to === "number"
        ? `фрагменты ${String(from + 1)}–${String(to + 1)}`
        : `с фрагмента ${String(from + 1)}`;
    return "начало документа";
  }
  if (call.key === FILE_TOOL_APPLY) {
    const changes = fileChanges(call.meta);
    if (changes.length > 0) return changesSummary(changes);
  }
  return toolArgumentsSummary(call.arguments);
}
