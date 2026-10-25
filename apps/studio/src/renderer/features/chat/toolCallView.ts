import { ChatCitationDto, type ChatToolCallDto, type CodeLanguage, type Json } from "@zvs/shared";
import { LANGUAGE_LABEL, PROCESS_TOOL_CODE, PROCESS_TOOL_SHELL } from "./processCopy";

const Citations = ChatCitationDto.array();

export interface FileTarget {
  readonly path: string;
  readonly start: number | null;
  readonly end: number | null;
}

export interface ViewField {
  readonly label: string;
  readonly value: string;
}

export type ToolCallView =
  | {
      readonly kind: "search";
      readonly query: string;
      readonly store: string | null;
      readonly limit: number | null;
      readonly citations: readonly ChatCitationDto[] | null;
    }
  | {
      readonly kind: "document";
      readonly document: string;
      readonly section: string | null;
      readonly from: number | null;
      readonly to: number | null;
    }
  | {
      readonly kind: "folder";
      readonly path: string | null;
      readonly recursive: boolean;
      readonly glob: string | null;
      readonly limit: number | null;
    }
  | { readonly kind: "files"; readonly files: readonly FileTarget[] }
  | {
      readonly kind: "grep";
      readonly query: string;
      readonly path: string | null;
      readonly regex: boolean;
      readonly ignoreCase: boolean;
      readonly glob: string | null;
      readonly limit: number | null;
    }
  | {
      readonly kind: "skill";
      readonly skill: string;
      readonly path: string | null;
      readonly args: readonly string[];
      readonly stdin: string | null;
    }
  | {
      readonly kind: "command";
      readonly argv: readonly string[];
      readonly cwd: string | null;
    }
  | {
      readonly kind: "program";
      readonly language: string;
      readonly source: string;
      readonly args: readonly string[];
      readonly packages: readonly string[];
    }
  | { readonly kind: "fields"; readonly fields: readonly ViewField[] };

type JsonObject = { readonly [key: string]: Json };

function object(value: Json): JsonObject | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

function text(args: JsonObject, name: string): string | null {
  const value = args[name];
  return typeof value === "string" && value.length > 0 ? value : null;
}

function count(args: JsonObject, name: string): number | null {
  const value = args[name];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function flag(args: JsonObject, name: string): boolean {
  return args[name] === true;
}

function strings(args: JsonObject, name: string): string[] {
  const value = args[name];
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

export function callCitations(call: Pick<ChatToolCallDto, "meta">): ChatCitationDto[] | null {
  const meta = object(call.meta);
  if (meta === null) return null;
  const parsed = Citations.safeParse(meta.citations);
  return parsed.success ? parsed.data : null;
}

function fileTargets(args: JsonObject): FileTarget[] {
  const paths = args.paths;
  if (!Array.isArray(paths)) return [];
  return paths.flatMap((entry) => {
    const target = object(entry);
    const path = target === null ? null : text(target, "path");
    if (target === null || path === null) return [];
    return [{ path, start: count(target, "startLine"), end: count(target, "endLine") }];
  });
}

function scalar(value: Json): string | null {
  if (typeof value === "string") return value.length > 0 ? value : null;
  if (typeof value === "number") return value.toLocaleString("ru-RU");
  if (typeof value === "boolean") return value ? "да" : "нет";
  return null;
}

function plainFields(args: JsonObject): ViewField[] {
  return Object.entries(args).flatMap(([label, value]) => {
    if (label === "conversationId") return [];
    const shown = scalar(value);
    return shown === null ? [] : [{ label, value: shown }];
  });
}

function isLanguage(value: string | null): value is CodeLanguage {
  return value !== null && value in LANGUAGE_LABEL;
}

export function toolCallView(
  call: Pick<ChatToolCallDto, "key" | "arguments" | "meta">,
): ToolCallView | null {
  const args = object(call.arguments);
  if (args === null) return null;
  switch (call.key) {
    case "knowledge.search":
    case "vector.search": {
      const query = text(args, "query");
      if (query === null) return null;
      return {
        kind: "search",
        query,
        store: text(args, "store"),
        limit: count(args, "limit") ?? count(args, "k"),
        citations: callCitations(call),
      };
    }
    case "knowledge.read": {
      const cited = callCitations(call)?.[0];
      const document = cited?.sourcePath ?? text(args, "documentId");
      if (document === null) return null;
      return {
        kind: "document",
        document,
        section: text(args, "section"),
        from: count(args, "fromChunk"),
        to: count(args, "toChunk"),
      };
    }
    case "fs.list":
      return {
        kind: "folder",
        path: text(args, "path"),
        recursive: flag(args, "recursive"),
        glob: text(args, "glob"),
        limit: count(args, "limit"),
      };
    case "fs.read": {
      const files = fileTargets(args);
      return files.length === 0 ? null : { kind: "files", files };
    }
    case "fs.search": {
      const query = text(args, "query");
      if (query === null) return null;
      return {
        kind: "grep",
        query,
        path: text(args, "path"),
        regex: flag(args, "regex"),
        ignoreCase: flag(args, "ignoreCase"),
        glob: text(args, "glob"),
        limit: count(args, "limit"),
      };
    }
    case "skill.load":
    case "skill.read":
    case "skill.save":
    case "skill.script": {
      const skill = text(args, "key");
      if (skill === null) return null;
      return {
        kind: "skill",
        skill,
        path: text(args, "path"),
        args: strings(args, "args"),
        stdin: text(args, "stdin"),
      };
    }
    case PROCESS_TOOL_SHELL: {
      const argv = strings(args, "argv");
      return argv.length === 0 ? null : { kind: "command", argv, cwd: text(args, "cwd") };
    }
    case PROCESS_TOOL_CODE: {
      const language = text(args, "language");
      const source = text(args, "source");
      if (source === null) return null;
      return {
        kind: "program",
        language: isLanguage(language) ? LANGUAGE_LABEL[language] : (language ?? ""),
        source,
        args: strings(args, "args"),
        packages: strings(args, "packages"),
      };
    }
    case "knowledge.list":
    case "vector.list":
      return null;
    case "job.run":
    case "download.fetch":
    case "vector.index": {
      const fields = plainFields(args);
      return fields.length === 0 ? null : { kind: "fields", fields };
    }
    default:
      return null;
  }
}

export function resultLines(result: string): number {
  return result.length === 0 ? 0 : result.split("\n").length;
}
