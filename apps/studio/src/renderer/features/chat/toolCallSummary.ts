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
import { MEMORY_TOOL, memoryCallSummary } from "../memory/memoryCopy";
import { WORKING_FOLDER } from "./toolCallCopy";
import { toolCallView, type ToolCallView } from "./toolCallView";

export function toolArgumentsSummary(value: Json): string {
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 80 ? `${text.slice(0, 80)}…` : text;
}

function field(value: Json, name: string): Json | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value[name]
    : undefined;
}

function isDocumentRead(call: ChatToolCallDto): boolean {
  const document = field(call.arguments, "documentId");
  return call.key === "knowledge.search" && typeof document === "string" && document.length > 0;
}

function foundCount(call: ChatToolCallDto): number | null {
  const citations = field(call.meta, "citations");
  return Array.isArray(citations) ? citations.length : null;
}

export function toolCallSummary(call: ChatToolCallDto): string {
  if (call.key === "knowledge.search" && !isDocumentRead(call)) {
    const query = field(call.arguments, "query");
    if (typeof query === "string") {
      const found = foundCount(call);
      return found === null ? `«${query}»` : `«${query}» · найдено фрагментов: ${String(found)}`;
    }
  }
  if (call.key === "knowledge.read" || isDocumentRead(call)) {
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
  if (call.key === MEMORY_TOOL) {
    const summary = memoryCallSummary(call.arguments);
    if (summary !== null) return summary;
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
  const view = toolCallView(call);
  const described = view === null ? null : viewSummary(view);
  return described ?? toolArgumentsSummary(call.arguments);
}

function viewSummary(view: ToolCallView): string | null {
  switch (view.kind) {
    case "search":
      return `«${view.query}»`;
    case "folder":
      return view.glob === null
        ? (view.path ?? WORKING_FOLDER)
        : `${view.path ?? WORKING_FOLDER} · ${view.glob}`;
    case "files": {
      const names = view.files.map((file) => file.path);
      return names.length > 2
        ? `${names.slice(0, 2).join(", ")} и ещё ${String(names.length - 2)}`
        : names.join(", ");
    }
    case "grep":
      return `«${view.query}»`;
    case "skill":
      return view.path === null ? view.skill : `${view.skill} · ${view.path}`;
    case "fields":
      return view.fields.map((field) => field.value).join(" · ");
    default:
      return null;
  }
}
