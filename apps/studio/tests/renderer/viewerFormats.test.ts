import { expect, test } from "vitest";
import JSZip from "jszip";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, DocumentSelectionDto, contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ArtifactActions from "../../src/renderer/features/chat/ArtifactActions.ts";
import {
  OPEN_IN_DEFAULT_APP,
  openInApplication,
} from "../../src/renderer/features/chat/artifacts.ts";
import { excerptPreview, selectionTitle } from "../../src/renderer/features/chat/selectionCopy.ts";
import {
  COLUMN_MAX_WIDTH,
  COLUMN_MIN_WIDTH,
  columnName,
  columnWidths,
  csvDelimiter,
  isNumeric,
  parseCsv,
  rowMatches,
} from "../../src/renderer/viewer/csv.ts";
import { columnIndex, readSlides, readWorkbook } from "../../src/renderer/viewer/ooxml.ts";

const MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG = "http://schemas.openxmlformats.org/package/2006/relationships";
const PRESENTATION = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DRAWING = "http://schemas.openxmlformats.org/drawingml/2006/main";

async function workbook(): Promise<Uint8Array> {
  const zip = new JSZip();
  zip.file(
    "xl/workbook.xml",
    `<workbook xmlns="${MAIN}" xmlns:r="${REL}"><sheets><sheet name="Выручка" sheetId="1" r:id="rId1"/><sheet name="Пусто" sheetId="2" r:id="rId2"/></sheets></workbook>`,
  );
  zip.file(
    "xl/_rels/workbook.xml.rels",
    `<Relationships xmlns="${PKG}"><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="/xl/worksheets/sheet2.xml"/></Relationships>`,
  );
  zip.file(
    "xl/sharedStrings.xml",
    `<sst xmlns="${MAIN}"><si><t>Год</t></si><si><r><t>Сум</t></r><r><t>ма</t></r></si></sst>`,
  );
  zip.file(
    "xl/worksheets/sheet1.xml",
    `<worksheet xmlns="${MAIN}"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="3"><c r="A3"><v>2021</v></c><c r="C3" t="inlineStr"><is><t>заметка</t></is></c><c r="D3" t="b"><v>1</v></c></row></sheetData></worksheet>`,
  );
  zip.file("xl/worksheets/sheet2.xml", `<worksheet xmlns="${MAIN}"><sheetData/></worksheet>`);
  return zip.generateAsync({ type: "uint8array" });
}

async function presentation(): Promise<Uint8Array> {
  const zip = new JSZip();
  zip.file(
    "ppt/presentation.xml",
    `<p:presentation xmlns:p="${PRESENTATION}" xmlns:r="${REL}"><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId1"/></p:sldIdLst></p:presentation>`,
  );
  zip.file(
    "ppt/_rels/presentation.xml.rels",
    `<Relationships xmlns="${PKG}"><Relationship Id="rId1" Target="slides/slide2.xml"/><Relationship Id="rId2" Target="slides/slide1.xml"/></Relationships>`,
  );
  const slide = (lines: string[]) =>
    `<p:sld xmlns:p="${PRESENTATION}" xmlns:a="${DRAWING}"><p:cSld><p:spTree><p:sp><p:txBody>${lines
      .map((line) => `<a:p><a:r><a:t>${line}</a:t></a:r></a:p>`)
      .join("")}<a:p/></p:txBody></p:sp></p:spTree></p:cSld></p:sld>`;
  zip.file("ppt/slides/slide1.xml", slide(["Итоги года", "Выручка выросла"]));
  zip.file("ppt/slides/slide2.xml", slide(["Планы"]));
  return zip.generateAsync({ type: "uint8array" });
}

test("csv parsing honours quotes, escaped quotes and both line endings", () => {
  const rows = parseCsv('год;"сумма; млрд"\r\n2021;"3,36"\n"""кавычки""";\n', ";");
  expect(rows).toEqual([
    ["год", "сумма; млрд"],
    ["2021", "3,36"],
    ['"кавычки"', ""],
  ]);
  expect(parseCsv("a,b", ",")).toEqual([["a", "b"]]);
  expect(parseCsv("a\nb\nc\n", ",", 2)).toHaveLength(2);
});

test("the delimiter comes from the mime type or the first line", () => {
  expect(csvDelimiter("a\tb", "text/tab-separated-values")).toBe("\t");
  expect(csvDelimiter("год;сумма;курс", "text/csv")).toBe(";");
  expect(csvDelimiter("a,b,c", "text/csv")).toBe(",");
  expect(csvDelimiter("одна колонка", "text/csv")).toBe(",");
});

test("grid helpers name columns and find matching rows", () => {
  expect([0, 25, 26, 701].map(columnName)).toEqual(["A", "Z", "AA", "ZZ"]);
  expect(columnIndex("AB12")).toBe(27);
  expect(rowMatches([["Выручка"], ["доход"], ["ВЫРУЧКА 2021"]], "выручка")).toEqual([0, 2]);
  expect(rowMatches([["a"]], " ")).toEqual([]);
});

test("a workbook reads every sheet with shared, inline and boolean cells in place", async () => {
  const sheets = await readWorkbook(await workbook());
  expect(sheets.map((sheet) => sheet.name)).toEqual(["Выручка", "Пусто"]);
  expect(sheets[0]?.rows).toEqual([
    ["Год", "Сумма"],
    [],
    [(2021).toLocaleString("ru-RU"), "", "заметка", "ИСТИНА"],
  ]);
  expect(sheets[1]?.rows).toEqual([]);
});

test("slides come back in presentation order with their text", async () => {
  const slides = await readSlides(await presentation());
  expect(slides).toEqual([
    { index: 1, paragraphs: ["Итоги года", "Выручка выросла"] },
    { index: 2, paragraphs: ["Планы"] },
  ]);
});

test("an archive without the expected parts reads as empty", async () => {
  const zip = new JSZip();
  zip.file("readme.txt", "x");
  const bytes = await zip.generateAsync({ type: "uint8array" });
  expect(await readWorkbook(bytes)).toEqual([]);
  expect(await readSlides(bytes)).toEqual([]);
});

test("artifacts open in the default application, named once per format", async () => {
  const bridge = createFakeBridge<Contract>()
    .handle("artifacts.open", () => undefined)
    .handle("artifacts.application", () => ({ name: "Word" }));
  const actions = new ArtifactActions(createIpcClient(contract, bridge));
  const id = "0199aa11-4444-7444-8444-444444444444/Отчёт.docx";

  expect(openInApplication(actions.applicationOf("docx"))).toBe(OPEN_IN_DEFAULT_APP);
  await actions.loadApplication(id, "DOCX");
  await actions.loadApplication(id, "docx");
  expect(bridge.calls.filter((call) => call.channel === "artifacts.application")).toHaveLength(1);
  expect(openInApplication(actions.applicationOf("docx"))).toBe("Открыть в Word");

  expect(await actions.open(id)).toBe(true);
  expect(bridge.calls.at(-1)).toEqual({ channel: "artifacts.open", payload: { id } });

  bridge.fail("artifacts.open", AppErrorCode.NOT_FOUND, "Файл не найден");
  expect(await actions.open(id)).toBe(false);
  expect(actions.errors[id]).toBe("Файл не найден");
});

test("an unnamed application falls back to the default label", async () => {
  const bridge = createFakeBridge<Contract>().handle("artifacts.application", () => ({}));
  const actions = new ArtifactActions(createIpcClient(contract, bridge));
  await actions.loadApplication("0199aa11-4444-7444-8444-444444444444/a.csv", "csv");
  expect(openInApplication(actions.applicationOf("csv"))).toBe(OPEN_IN_DEFAULT_APP);
});

test("selection chips name the document, page and a short excerpt", () => {
  const selection = DocumentSelectionDto.parse({
    documentId: "0199aa11-2222-7222-8222-222222222222",
    name: "Отчёт.docx",
    page: 12,
    start: 0,
    end: 10,
    text: "текст",
    untrusted: true,
  });
  expect(selectionTitle(selection)).toBe("Отчёт.docx · стр. 12");
  expect(selectionTitle({ ...selection, page: null })).toBe("Отчёт.docx");
  expect(excerptPreview("  a\n\nb  ")).toBe("a b");
  expect(excerptPreview("я".repeat(60))).toBe(`${"я".repeat(48)}…`);
});

test("every row shares one set of column widths sized to the longest cell", () => {
  const rows = [["Категория", "React + TypeScript", "x".repeat(500)], ["Сервер"], []];
  const widths = columnWidths(rows, 4);
  expect(widths).toHaveLength(4);
  expect(widths[0]).toBe(Math.round(9 * 7.2 + 20));
  expect(widths[1]).toBeGreaterThan(widths[0]!);
  expect(widths[2]).toBe(COLUMN_MAX_WIDTH);
  expect(widths[3]).toBe(COLUMN_MIN_WIDTH);
});

test("numbers are recognised for right alignment", () => {
  expect(["2021", "3,36", "-7.5", "1 234", "45%"].every(isNumeric)).toBe(true);
  expect(["Утверждено", "", "v1.2.3", "12 мая"].some(isNumeric)).toBe(false);
});
