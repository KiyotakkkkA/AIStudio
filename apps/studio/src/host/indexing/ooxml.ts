import { AppError, AppErrorCode } from "@zvs/shared";
import { readZipEntries } from "../platform/archive.ts";
import type { Extractor } from "./extraction.ts";

/**
 * `.docx`, `.xlsx` and `.pptx` are zips with XML inside, so extracting their text needs a zip
 * reader and a tag scanner — not a document library. The parts are machine-generated and
 * well-formed, which is what makes scanning them with expressions honest rather than a shortcut:
 * there is no arbitrary nesting to get wrong, only runs of text inside known elements.
 *
 * The legacy binary formats (`.doc`, `.xls`, `.ppt`) share none of this and stay deferred.
 */

const ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

export function decodeXmlText(raw: string): string {
  return raw.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z]+);/g, (match, entity: string) => {
    if (entity.startsWith("#")) {
      const code = entity.startsWith("#x")
        ? Number.parseInt(entity.slice(2), 16)
        : Number.parseInt(entity.slice(1), 10);
      return Number.isFinite(code) && code > 0 ? String.fromCodePoint(code) : match;
    }
    return ENTITIES[entity.toLowerCase()] ?? match;
  });
}

function textRuns(xml: string, tag: string): string[] {
  const pattern = new RegExp(`<${tag}(?:\\s[^>]*)?>([\\s\\S]*?)</${tag}>`, "g");
  return [...xml.matchAll(pattern)].map((match) => decodeXmlText(match[1] ?? ""));
}

function blocks(xml: string, closing: string): string[] {
  return xml.split(closing);
}

/**
 * Collapses the padding a layout leaves behind without touching tabs: a tab is the column
 * separator that a spreadsheet row and a Word tab stop both depend on, so squashing it into a
 * space would lose the structure this text is being extracted for.
 */
function tidy(lines: readonly string[]): string {
  const out: string[] = [];
  for (const line of lines) {
    const normalised = line.replace(/ {2,}/g, " ").replace(/[ \t]+$/, "");
    const blank = normalised.trim() === "";
    if (blank && (out.at(-1) ?? "") === "") continue;
    out.push(blank ? "" : normalised);
  }
  return out.join("\n").replace(/^\n+|\n+$/g, "");
}

async function parts(
  bytes: Uint8Array,
  wanted: (name: string) => boolean,
  what: string,
): Promise<Map<string, string>> {
  const found = await readZipEntries(bytes, wanted);
  if (found.size === 0)
    throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, `В файле нет части ${what}`);
  return new Map([...found].map(([name, content]) => [name, content.toString("utf8")]));
}

/**
 * Word keeps the body in `word/document.xml`; a paragraph is `w:p`, and the visible text is the
 * `w:t` runs inside it. Tables come through as one line per cell paragraph, which is what a
 * chunker wants anyway.
 */
export const docxExtractor: Extractor = async ({ bytes }) => {
  const found = await parts(bytes, (name) => name === "word/document.xml", "word/document.xml");
  const xml = found.get("word/document.xml") ?? "";
  // A tab stop is its own element, outside any `w:t`. Rewriting it as a text run is what keeps
  // it in reading order instead of dropping it.
  const lines = blocks(xml, "</w:p>").map((paragraph) =>
    textRuns(paragraph.replace(/<w:tab\b[^>]*\/?>/g, "<w:t>\t</w:t>"), "w:t").join(""),
  );
  return tidy(lines);
};

/**
 * Excel stores repeated text once in `sharedStrings.xml` and refers to it by index, so both
 * parts are needed. A row becomes one tab-separated line; an empty sheet contributes nothing.
 */
export const xlsxExtractor: Extractor = async ({ bytes }) => {
  const found = await parts(
    bytes,
    (name) => name === "xl/sharedStrings.xml" || /^xl\/worksheets\/sheet\d+\.xml$/.test(name),
    "xl/worksheets",
  );
  const shared = textRuns(found.get("xl/sharedStrings.xml") ?? "", "t");
  const sheets = [...found.keys()]
    .filter((name) => name.startsWith("xl/worksheets/"))
    .sort((left, right) => left.localeCompare(right, "en"));

  const lines: string[] = [];
  for (const sheet of sheets) {
    for (const row of blocks(found.get(sheet) ?? "", "</row>")) {
      const cells: string[] = [];
      for (const cell of [...row.matchAll(/<c\b([^>]*)>([\s\S]*?)<\/c>/g)]) {
        const type = /\bt="([^"]+)"/.exec(cell[1] ?? "")?.[1];
        const body = cell[2] ?? "";
        if (type === "s") {
          const index = Number.parseInt(textRuns(body, "v")[0] ?? "", 10);
          cells.push(shared[index] ?? "");
        } else if (type === "inlineStr") {
          cells.push(textRuns(body, "t").join(""));
        } else {
          cells.push(textRuns(body, "v")[0] ?? "");
        }
      }
      const line = cells.join("\t").replace(/\t+$/, "");
      if (line.trim() !== "") lines.push(line);
    }
  }
  return tidy(lines);
};

/** PowerPoint puts the visible text of every shape in `a:t` runs, one slide per part. */
export const pptxExtractor: Extractor = async ({ bytes }) => {
  const found = await parts(
    bytes,
    (name) => /^ppt\/slides\/slide\d+\.xml$/.test(name),
    "ppt/slides",
  );
  const slides = [...found.keys()].sort((left, right) => slideNumber(left) - slideNumber(right));
  const lines: string[] = [];
  for (const [index, slide] of slides.entries()) {
    lines.push(`Слайд ${String(index + 1)}`);
    for (const paragraph of blocks(found.get(slide) ?? "", "</a:p>"))
      lines.push(textRuns(paragraph, "a:t").join(""));
    lines.push("");
  }
  return tidy(lines);
};

function slideNumber(name: string): number {
  return Number.parseInt(/slide(\d+)\.xml$/.exec(name)?.[1] ?? "0", 10);
}
