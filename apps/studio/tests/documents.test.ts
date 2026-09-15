import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { deflateRawSync } from "node:zlib";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { AppErrorCode, VectorOcrConfig, type Json } from "@zvs/shared";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import {
  createImageExtractor,
  createPdfExtractor,
  DOCUMENT_JOB,
  IMAGE_EXTENSIONS,
  type OcrPort,
} from "../src/host/indexing/documents.ts";
import { createExtractorRegistry } from "../src/host/indexing/extraction.ts";
import { docxExtractor, decodeXmlText } from "../src/host/indexing/ooxml.ts";

let workspace: TemporaryDirectory;

beforeEach(() => {
  workspace = temporaryDirectory("documents-");
});
afterEach(() => {
  workspace.dispose();
});

/** A zip whose members are deflated, which is what Word and Excel actually write. */
function zip(members: readonly { name: string; body: string }[]): Buffer {
  const locals: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;
  for (const member of members) {
    const name = Buffer.from(member.name, "utf8");
    const raw = Buffer.from(member.body, "utf8");
    const stored = deflateRawSync(raw);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(stored.length, 18);
    local.writeUInt32LE(raw.length, 22);
    local.writeUInt16LE(name.length, 26);
    locals.push(local, name, stored);

    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt16LE(8, 10);
    entry.writeUInt32LE(stored.length, 20);
    entry.writeUInt32LE(raw.length, 24);
    entry.writeUInt16LE(name.length, 28);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, name);
    offset += local.length + name.length + stored.length;
  }
  const directory = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(members.length, 8);
  eocd.writeUInt16LE(members.length, 10);
  eocd.writeUInt32LE(directory.length, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, directory, eocd]);
}

const DOCX = zip([
  {
    name: "word/document.xml",
    body: `<?xml version="1.0"?><w:document><w:body>
      <w:p><w:r><w:t xml:space="preserve">Договор </w:t></w:r><w:r><w:t>№ 42</w:t></w:r></w:p>
      <w:p><w:r><w:t>Сторона A</w:t></w:r><w:tab/><w:r><w:t>ООО &quot;Ромашка&quot;</w:t></w:r></w:p>
      <w:p/>
      <w:p><w:r><w:t>Итого: 100 &lt; 200</w:t></w:r></w:p>
    </w:body></w:document>`,
  },
  { name: "[Content_Types].xml", body: "<Types/>" },
]);

test("a docx is read paragraph by paragraph, with entities decoded", async () => {
  const text = await docxExtractor({ path: "a.docx", bytes: DOCX });

  expect(text).toContain("Договор № 42");
  expect(text).toContain('Сторона A\tООО "Ромашка"');
  expect(text).toContain("Итого: 100 < 200");
});

test("xml entities, numeric and named alike, survive the round trip", () => {
  expect(decodeXmlText("&lt;a&gt; &amp; &quot;b&quot; &#1055;&#x440;")).toBe('<a> & "b" Пр');
  // An entity we do not know is left alone rather than swallowed.
  expect(decodeXmlText("&unknownthing;")).toBe("&unknownthing;");
});

test("an xlsx resolves shared strings and joins a row with tabs", async () => {
  const bytes = zip([
    {
      name: "xl/sharedStrings.xml",
      body: "<sst><si><t>Позиция</t></si><si><t>Цена</t></si><si><t>Болт</t></si></sst>",
    },
    {
      name: "xl/worksheets/sheet1.xml",
      body: `<worksheet><sheetData>
        <row><c t="s"><v>0</v></c><c t="s"><v>1</v></c></row>
        <row><c t="s"><v>2</v></c><c><v>19.5</v></c></row>
      </sheetData></worksheet>`,
    },
  ]);

  const registry = createExtractorRegistry();
  const text = await registry.extract({ path: "prices.xlsx", bytes });

  expect(text).toBe("Позиция\tЦена\nБолт\t19.5");
});

test("a pptx is read slide by slide", async () => {
  const bytes = zip([
    {
      name: "ppt/slides/slide2.xml",
      body: "<p:sld><a:p><a:r><a:t>Вторая</a:t></a:r></a:p></p:sld>",
    },
    {
      name: "ppt/slides/slide1.xml",
      body: "<p:sld><a:p><a:r><a:t>Первая</a:t></a:r></a:p></p:sld>",
    },
  ]);

  const registry = createExtractorRegistry();
  const text = await registry.extract({ path: "deck.pptx", bytes });

  // Slide 10 must not sort before slide 2, which a plain string sort would do.
  expect(text).toBe("Слайд 1\nПервая\n\nСлайд 2\nВторая");
});

test("the office formats are registered and the legacy binary ones stay deferred", () => {
  const registry = createExtractorRegistry();

  expect(registry.supports("a.docx")).toBe(true);
  expect(registry.supports("a.xlsx")).toBe(true);
  expect(registry.supports("a.pptx")).toBe(true);
  expect(registry.supports("a.doc")).toBe(false);
  expect(registry.reason("a.doc")).toContain("пока не поддерживается");
});

test("a file that is not a zip fails as an unsupported format, not a crash", async () => {
  await expect(
    docxExtractor({ path: "a.docx", bytes: Buffer.from("plain text, not a zip") }),
  ).rejects.toMatchObject({ code: AppErrorCode.UNSUPPORTED_FORMAT });
});

function ocrConfig(overrides: Partial<VectorOcrConfig> = {}): VectorOcrConfig {
  return VectorOcrConfig.parse({
    enabled: true,
    modelRef: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
    minCharsPerPage: 200,
    ...overrides,
  });
}

interface JobCall {
  job: string;
  params: Json;
}

function jobsReturning(result: Json, calls: JobCall[] = []) {
  return {
    calls,
    run: (job: string, params: Json) => {
      calls.push({ job, params });
      return Promise.resolve(result);
    },
  };
}

test("a pdf with a text layer is returned as-is, and OCR is never asked for", async () => {
  const jobs = jobsReturning({
    pages: [
      { number: 1, text: "Первая страница", chars: 15 },
      { number: 2, text: "Вторая страница", chars: 15 },
    ],
    images: [],
    notes: [],
  });
  const ocr: OcrPort = { readImage: vi.fn() };
  const extract = createPdfExtractor({ jobs, ocr, scratchDir: workspace.path });

  const text = await extract({ path: "a.pdf", bytes: new Uint8Array(), context: {} });

  expect(text).toBe("Первая страница\n\nВторая страница");
  expect(ocr.readImage).not.toHaveBeenCalled();
  // With OCR off, the sidecar is told not to bother extracting images either.
  expect(jobs.calls[0]?.job).toBe(DOCUMENT_JOB);
  expect(jobs.calls[0]?.params).toMatchObject({ minCharsPerPage: 0 });
});

test("a scanned page is replaced by what the vision model read, not merged with it", async () => {
  const scan = join(workspace.path, "page-2.jpg");
  mkdirSync(workspace.path, { recursive: true });
  writeFileSync(scan, Buffer.from("jpeg-bytes"));
  const jobs = jobsReturning({
    pages: [
      { number: 1, text: "Настоящий текст страницы один".repeat(10), chars: 290 },
      { number: 2, text: "стр. 2", chars: 6 },
    ],
    images: [{ page: 2, path: scan, mediaType: "image/jpeg" }],
    notes: [],
  });
  const readImage = vi.fn().mockResolvedValue("  Распознанный текст скана  ");
  const extract = createPdfExtractor({
    jobs,
    ocr: { readImage },
    scratchDir: workspace.path,
  });

  const text = await extract({
    path: "scan.pdf",
    bytes: new Uint8Array(),
    context: { ocr: ocrConfig({ language: "rus" }) },
  });

  expect(text).toContain("Распознанный текст скана");
  expect(text).not.toContain("стр. 2");
  expect(jobs.calls[0]?.params).toMatchObject({ minCharsPerPage: 200 });
  expect(readImage).toHaveBeenCalledWith(
    "curated:model:qwen2.5-vl-7b-instruct-q4_k_m",
    expect.objectContaining({ mediaType: "image/jpeg" }),
    "rus",
    expect.anything(),
  );
});

test("a page the model cannot read is noted, and the rest of the document survives", async () => {
  const scan = join(workspace.path, "page-2.jpg");
  writeFileSync(scan, Buffer.from("jpeg-bytes"));
  const jobs = jobsReturning({
    pages: [
      { number: 1, text: "Читаемая страница", chars: 300 },
      { number: 2, text: "", chars: 0 },
    ],
    images: [{ page: 2, path: scan, mediaType: "image/jpeg" }],
    notes: [{ page: 3, reason: "на странице нет ни текста, ни встроенного изображения" }],
  });
  const notes: string[] = [];
  const extract = createPdfExtractor({
    jobs,
    ocr: { readImage: () => Promise.reject(new Error("модель не запустилась")) },
    scratchDir: workspace.path,
  });

  const text = await extract({
    path: "scan.pdf",
    bytes: new Uint8Array(),
    context: { ocr: ocrConfig(), note: (message) => notes.push(message) },
  });

  expect(text).toBe("Читаемая страница");
  expect(notes).toHaveLength(2);
  expect(notes.some((note) => note.includes("с. 3"))).toBe(true);
  expect(notes.some((note) => note.includes("модель не запустилась"))).toBe(true);
});

test("OCR that is switched on but has no model chosen stays switched off", async () => {
  const jobs = jobsReturning({
    pages: [{ number: 1, text: "a", chars: 1 }],
    images: [],
    notes: [],
  });
  const readImage = vi.fn();
  const extract = createPdfExtractor({ jobs, ocr: { readImage }, scratchDir: workspace.path });

  await extract({
    path: "a.pdf",
    bytes: new Uint8Array(),
    context: { ocr: ocrConfig({ modelRef: "" }) },
  });

  expect(jobs.calls[0]?.params).toMatchObject({ minCharsPerPage: 0 });
  expect(readImage).not.toHaveBeenCalled();
});

test("an image file is read by the vision model, and refused when OCR is off", async () => {
  const readImage = vi.fn().mockResolvedValue("Текст с фотографии");
  const extract = createImageExtractor({
    jobs: jobsReturning({}),
    ocr: { readImage },
    scratchDir: workspace.path,
  });
  const bytes = Buffer.from("png-bytes");

  expect(IMAGE_EXTENSIONS).toContain(".png");
  expect(await extract({ path: "photo.png", bytes, context: { ocr: ocrConfig() } })).toBe(
    "Текст с фотографии",
  );
  expect(readImage.mock.calls[0]?.[1]).toMatchObject({ mediaType: "image/png" });

  // Declining is UNSUPPORTED_FORMAT, which the pipeline counts as a skip rather than a failure.
  await expect(extract({ path: "photo.png", bytes, context: {} })).rejects.toMatchObject({
    code: AppErrorCode.UNSUPPORTED_FORMAT,
  });
});
