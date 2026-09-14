import { extname } from "node:path";
import { AppError, AppErrorCode } from "@zvs/shared";

export interface ExtractionInput {
  readonly path: string;
  readonly bytes: Uint8Array;
}

export type Extractor = (input: ExtractionInput) => Promise<string>;

const decoder = new TextDecoder("utf-8", { fatal: false });

function decode(bytes: Uint8Array): string {
  const text = decoder.decode(bytes);
  return text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
}

function splitCsvLine(line: string, separator: string): string[] {
  const cells: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    const char = line[i]!;
    if (quoted) {
      if (char !== '"') cell += char;
      else if (line[i + 1] === '"') {
        cell += '"';
        i += 1;
      } else quoted = false;
    } else if (char === '"') quoted = true;
    else if (char === separator) {
      cells.push(cell);
      cell = "";
    } else cell += char;
  }
  cells.push(cell);
  return cells.map((value) => value.trim());
}

export const plainTextExtractor: Extractor = async ({ bytes }) => decode(bytes);

export const csvExtractor: Extractor = async ({ path, bytes }) => {
  const separator = extname(path).toLowerCase() === ".tsv" ? "\t" : ",";
  const lines = decode(bytes)
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0);
  const header = lines.shift();
  if (header === undefined) return "";
  const columns = splitCsvLine(header, separator);
  if (lines.length === 0) return columns.join(", ");
  return lines
    .map((line) =>
      splitCsvLine(line, separator)
        .map((value, index) => `${columns[index] ?? `column ${String(index + 1)}`}: ${value}`)
        .join("; "),
    )
    .join("\n");
};

const TEXT_EXTENSIONS = [
  ".txt",
  ".text",
  ".log",
  ".md",
  ".markdown",
  ".mdx",
  ".rst",
  ".adoc",
  ".json",
  ".jsonl",
  ".yaml",
  ".yml",
  ".toml",
  ".ini",
  ".cfg",
  ".conf",
  ".env",
  ".xml",
  ".html",
  ".htm",
  ".css",
  ".scss",
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".py",
  ".rs",
  ".go",
  ".java",
  ".kt",
  ".swift",
  ".c",
  ".h",
  ".cc",
  ".cpp",
  ".hpp",
  ".cs",
  ".rb",
  ".php",
  ".lua",
  ".sh",
  ".bash",
  ".ps1",
  ".sql",
  ".graphql",
  ".proto",
];

const DEFERRED_EXTENSIONS = [
  ".pdf",
  ".doc",
  ".docx",
  ".odt",
  ".rtf",
  ".ppt",
  ".pptx",
  ".xls",
  ".xlsx",
  ".ods",
  ".epub",
];

export class ExtractorRegistry {
  private readonly extractors = new Map<string, Extractor>();
  private readonly deferred = new Set<string>();

  register(extensions: readonly string[], extractor: Extractor): this {
    for (const extension of extensions) {
      const key = extension.toLowerCase();
      this.extractors.set(key, extractor);
      this.deferred.delete(key);
    }
    return this;
  }
  defer(extensions: readonly string[]): this {
    for (const extension of extensions)
      if (!this.extractors.has(extension.toLowerCase())) this.deferred.add(extension.toLowerCase());
    return this;
  }
  supports(path: string): boolean {
    return this.extractors.has(extname(path).toLowerCase());
  }
  reason(path: string): string {
    const extension = extname(path).toLowerCase();
    if (this.deferred.has(extension)) return `формат ${extension} пока не поддерживается`;
    return extension === "" ? "файл без расширения" : `неизвестный формат ${extension}`;
  }
  async extract(input: ExtractionInput): Promise<string> {
    const extractor = this.extractors.get(extname(input.path).toLowerCase());
    if (!extractor) throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, this.reason(input.path));
    return extractor(input);
  }
}

export function createExtractorRegistry(): ExtractorRegistry {
  return new ExtractorRegistry()
    .register(TEXT_EXTENSIONS, plainTextExtractor)
    .register([".csv", ".tsv"], csvExtractor)
    .defer(DEFERRED_EXTENSIONS);
}
