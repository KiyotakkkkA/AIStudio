import JSZip from "jszip";

export const SHEET_ROWS_MAX = 5000;
export const SHEET_COLUMNS_MAX = 200;

export interface Sheet {
  readonly name: string;
  readonly rows: string[][];
  readonly truncated: boolean;
}

export interface Slide {
  readonly index: number;
  readonly paragraphs: string[];
}

const RELATIONSHIPS_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

function xml(text: string): Document {
  return new DOMParser().parseFromString(text, "application/xml");
}

function elements(root: Document | Element, name: string): Element[] {
  return Array.from(root.getElementsByTagNameNS("*", name));
}

async function entry(zip: JSZip, path: string): Promise<Document | null> {
  const file = zip.file(path);
  return file === null ? null : xml(await file.async("string"));
}

function resolveTarget(base: string, target: string): string {
  if (target.startsWith("/")) return target.slice(1);
  const parts = base.split("/").slice(0, -1);
  for (const part of target.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== ".") parts.push(part);
  }
  return parts.join("/");
}

async function relationships(zip: JSZip, owner: string): Promise<Map<string, string>> {
  const folder = owner.split("/").slice(0, -1).join("/");
  const name = owner.split("/").at(-1) ?? owner;
  const document = await entry(zip, `${folder}/_rels/${name}.rels`);
  const targets = new Map<string, string>();
  if (document === null) return targets;
  for (const relation of elements(document, "Relationship")) {
    const id = relation.getAttribute("Id");
    const target = relation.getAttribute("Target");
    if (id !== null && target !== null) targets.set(id, resolveTarget(owner, target));
  }
  return targets;
}

function relationId(element: Element): string | null {
  return element.getAttributeNS(RELATIONSHIPS_NS, "id") ?? element.getAttribute("r:id");
}

function runText(element: Element): string {
  return elements(element, "t")
    .map((node) => node.textContent ?? "")
    .join("");
}

export function columnIndex(reference: string): number {
  const letters = /^[A-Z]+/u.exec(reference.toUpperCase())?.[0] ?? "";
  let index = 0;
  for (const letter of letters) index = index * 26 + (letter.charCodeAt(0) - 64);
  return index - 1;
}

function cellValue(cell: Element, shared: readonly string[]): string {
  const type = cell.getAttribute("t");
  if (type === "inlineStr") {
    const inline = elements(cell, "is")[0];
    return inline === undefined ? "" : runText(inline);
  }
  const raw = elements(cell, "v")[0]?.textContent ?? "";
  if (type === "s") return shared[Number(raw)] ?? "";
  if (type === "b") return raw === "1" ? "ИСТИНА" : "ЛОЖЬ";
  if (type === "str" || type === "e") return raw;
  const number = Number(raw);
  return raw.length > 0 && Number.isFinite(number) ? number.toLocaleString("ru-RU") : raw;
}

function sheetRows(document: Document, shared: readonly string[]): Sheet["rows"] {
  const rows: string[][] = [];
  for (const row of elements(document, "row")) {
    if (rows.length >= SHEET_ROWS_MAX) break;
    const number = Number(row.getAttribute("r") ?? rows.length + 1);
    while (rows.length < number - 1 && rows.length < SHEET_ROWS_MAX) rows.push([]);
    const cells: string[] = [];
    let next = 0;
    for (const cell of elements(row, "c")) {
      const reference = cell.getAttribute("r");
      const column = reference === null ? next : columnIndex(reference);
      next = column + 1;
      if (column >= SHEET_COLUMNS_MAX) continue;
      while (cells.length < column) cells.push("");
      cells[column] = cellValue(cell, shared);
    }
    rows.push(cells);
  }
  return rows;
}

export async function readWorkbook(bytes: Uint8Array): Promise<Sheet[]> {
  const zip = await JSZip.loadAsync(bytes);
  const workbook = await entry(zip, "xl/workbook.xml");
  if (workbook === null) return [];
  const targets = await relationships(zip, "xl/workbook.xml");
  const sharedDocument = await entry(zip, "xl/sharedStrings.xml");
  const shared =
    sharedDocument === null ? [] : elements(sharedDocument, "si").map((item) => runText(item));
  const sheets: Sheet[] = [];
  for (const sheet of elements(workbook, "sheet")) {
    const id = relationId(sheet);
    const target = id === null ? undefined : targets.get(id);
    const document = target === undefined ? null : await entry(zip, target);
    if (document === null) continue;
    const rowCount = elements(document, "row").length;
    sheets.push({
      name: sheet.getAttribute("name") ?? `${String(sheets.length + 1)}`,
      rows: sheetRows(document, shared),
      truncated: rowCount > SHEET_ROWS_MAX,
    });
  }
  return sheets;
}

export async function readSlides(bytes: Uint8Array): Promise<Slide[]> {
  const zip = await JSZip.loadAsync(bytes);
  const presentation = await entry(zip, "ppt/presentation.xml");
  if (presentation === null) return [];
  const targets = await relationships(zip, "ppt/presentation.xml");
  const slides: Slide[] = [];
  for (const slide of elements(presentation, "sldId")) {
    const id = relationId(slide);
    const target = id === null ? undefined : targets.get(id);
    const document = target === undefined ? null : await entry(zip, target);
    if (document === null) continue;
    const paragraphs = elements(document, "p")
      .filter((paragraph) => paragraph.namespaceURI?.includes("drawingml") === true)
      .map(runText)
      .map((text) => text.trim())
      .filter((text) => text.length > 0);
    slides.push({ index: slides.length + 1, paragraphs });
  }
  return slides;
}
