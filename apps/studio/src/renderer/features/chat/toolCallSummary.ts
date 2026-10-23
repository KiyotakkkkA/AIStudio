import type { ChatToolCallDto, Json } from "@zvs/shared";
import { FILE_TOOL_APPLY, changesSummary, fileChanges } from "./fileChangesCopy";
import {
  LANGUAGE_LABEL,
  PROCESS_TOOL_CODE,
  PROCESS_TOOL_SHELL,
  commandLine,
  processResult,
  processSummary,
} from "./processCopy";
import { ASK_TOOL, questionRecord, recordSummary } from "./questionCopy";

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
  if (call.key === ASK_TOOL) {
    const record = questionRecord(call.meta);
    if (record) return recordSummary(record, call.status === "awaiting-answer");
  }
  if (call.key === FILE_TOOL_APPLY) {
    const changes = fileChanges(call.meta);
    if (changes.length > 0) return changesSummary(changes);
  }
  if (call.key === PROCESS_TOOL_CODE || call.key === PROCESS_TOOL_SHELL) {
    const result = processResult(call.meta);
    if (result) return processSummary(result.process);
    const argv = field(call.arguments, "argv");
    if (Array.isArray(argv) && argv.every((part) => typeof part === "string"))
      return toolArgumentsSummary(commandLine(argv));
    const language = field(call.arguments, "language");
    if (language === "python" || language === "javascript" || language === "powershell")
      return LANGUAGE_LABEL[language];
  }
  return toolArgumentsSummary(call.arguments);
}
