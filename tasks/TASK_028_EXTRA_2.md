---
task: TASK_028_EXTRA_2
title: PDF and Office extraction, and OCR through the local vision model
phase: 5 — Sidecar and long-running work
status: done
depends_on: [TASK_028, TASK_028_EXTRA_1]
unblocks: [TASK_042, TASK_043]
estimate: 5 h
---

# TASK_028_EXTRA_2 — PDF and Office extraction, and OCR through the local vision model

## Context

TASK_028 shipped the extractor registry with `.pdf`, `.docx`, `.xlsx` and `.pptx` on a **deferred**
list: a file in one of those formats was walked, counted and skipped with «формат .pdf пока не
поддерживается». For most real corpora that is most of the corpus.

Two of the pieces this needs already existed and were waiting to be joined up. `VectorOcrConfig`
has carried `enabled`, `modelRef`, `language` and `minCharsPerPage` since TASK_021, with the
Advanced panel on the Vector Stores page already rendering all four — the schema comment says
outright that OCR is «зрительно-языковая модель, которая превращает сканы в текст». The curated
catalogue has shipped Qwen2.5-VL 7B since TASK_029. TASK_028_EXTRA_1 then added the engine that
can run it. What was missing was the extraction itself and the wiring between them.

## Why

Retrieval over a document corpus that cannot read documents is a demo. This is the task that makes
the Documents tab worth pointing at a real folder.

## Read first

- `TASK_028` Handoff — the extractor registry and the hook it left for exactly this
- `TASK_028_EXTRA_1` Handoff — the llama.cpp runtime OCR runs on
- `/docs/PASSPORT.md` §10 — why OCR and document conversion are sidecar work

## Scope

**In scope**

- PDF text extraction, per page, in Rust, reached through the sidecar
- `.docx` / `.xlsx` / `.pptx` extraction on the host
- OCR for scanned PDF pages and for standalone image files, through the local vision model
- The projector (`mmproj`) that a vision model needs, in the curated catalogue

**Out of scope**

- The legacy binary Office formats (`.doc`, `.xls`, `.ppt`) and OpenDocument (`.odt`, `.ods`)
- PDF **rasterisation**. A scanned page's own image is lifted out of the file; a page that is
  neither text nor an embedded image is reported, not rendered.
- The document viewer (TASK_042) — this is ingestion only

## ❓ DECISION (answered 2026-09-15)

**Q: Rust or TypeScript?** Answered: **Rust for PDF, TypeScript for OOXML** — see the deviations
below for why the line falls there.

## Steps

1. `zvs-core/src/pdf.rs` over `pdf-extract`: per-page text, and the embedded image of any page
   whose text layer is too thin to be real. Everything behind `catch_unwind`.
2. `zvs-jobd/src/document.rs`: `job.document.extract`, parsing on a blocking task so progress and
   cancellation keep being served, writing page images to a directory the caller names.
3. `platform/archive.ts`: factor the zip reader over a `ByteSource` so a `.docx` in memory and a
   400 MB engine archive on disk share one implementation.
4. `indexing/ooxml.ts`: `.docx`, `.xlsx`, `.pptx`.
5. `indexing/documents.ts`: the PDF extractor (sidecar → OCR → assemble) and the image extractor.
6. `ExtractionContext` so an extractor can see the store's OCR settings, the run's signal, and a
   way to add a note to the report.
7. The `mmproj` projector in the curated catalogue, and out of the OCR model picker.

## Verify

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo test
pnpm vitest run
pnpm typecheck && pnpm lint && pnpm lint:tailwind && pnpm build
```

Manual check: index a folder holding a born-digital PDF, a scan, a `.docx` and a `.xlsx`. With OCR
off the scan is skipped with a reason; with OCR on and Qwen2.5-VL installed its text is searchable.

## Definition of done

- [x] A born-digital PDF is indexed, page by page, with Cyrillic intact
- [x] `.docx`, `.xlsx` and `.pptx` are indexed
- [x] A scanned page is recognised by the vision model and indexed
- [x] A standalone image is indexed when OCR is on and skipped, not failed, when it is off
- [x] A malformed PDF fails one file, not the run, and never the window
- [x] Tests and gates pass

## Handoff

**Built:**

*Rust* — `zvs-core/src/pdf.rs`: `extract_pages` returns per-page text with a character count,
`page_images` lifts the embedded image out of the pages named by the caller, and `extract`
combines them against a `min_chars` threshold. Both entry points run inside `catch_unwind`, so a
parser panic becomes a `NATIVE_ERROR` rather than taking the process down. Text is normalised —
trailing space dropped, runs of blank lines collapsed — so the character count means something.
`zvs-jobd/src/document.rs` adds `job.document.extract`: parsing happens on `spawn_blocking`,
images are written to the caller's directory and reported by path, and the page budget
(`DEFAULT_MAX_IMAGES`, 50) bounds how much of a long scan goes to OCR. One new dependency,
`pdf-extract 0.12.0`, which re-exports `lopdf` — so the object model comes from one crate, not two.

*Host* — `platform/archive.ts` grew a `ByteSource` seam: `readZipEntries(bytes, predicate)` reads
named members out of a zip held in memory, and the engine-archive extractor now goes through the
same central-directory parser it always did. `indexing/ooxml.ts` reads `.docx` (paragraphs from
`w:p`, tab stops rewritten as text runs so they survive), `.xlsx` (shared strings resolved, a row
per line, tab-separated) and `.pptx` (a heading and the `a:t` runs per slide), all by scanning
tags — these are machine-generated parts with no arbitrary nesting, which is what makes that
honest rather than a shortcut. `indexing/documents.ts` holds `createPdfExtractor` (sidecar → OCR
the thin pages → assemble) and `createImageExtractor`. `ExtractionInput` gained a `context` with
the store's `ocr` config, the run's `signal`, and a `note` callback that writes into the run
report. `RuntimeService.readImage` starts a second llama-server in a `vision` role with `--mmproj`
and posts the image as a data URI to `/v1/chat/completions`.

*Catalogue* — `mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf` (853 MB, sha256 verified against the
Hugging Face API). `autofill.ocrCandidates` now excludes projectors, so the OCR picker offers
models rather than halves of one.

*Tests* — `documents.test.ts` (11 cases: OOXML round trips, entity decoding, slide ordering, the
OCR decision, a page the model could not read, the OCR-off skip), five Rust cases in `pdf.rs` and
three in `document.rs`.

**Deviated from plan:**

- **The Rust/TypeScript line is drawn by risk, not by language preference.** PDF went to Rust in
  the sidecar because a PDF is the least trustworthy input the app accepts — broken xref tables and
  hostile font programs are ordinary, and the parsers that survive them do so by panicking. OOXML
  went to TypeScript because a `.docx` is a zip with XML in it, the zip reader was already written
  for the engine archives, and pushing it into Rust would have bought a second `zip` dependency
  and a protocol round trip to gain nothing.
- **Embedded images, not rasterisation.** A scanner writes one full-page JPEG per page, so the
  pixels OCR needs are already in the file. Pulling them out needs no rasteriser; rendering a page
  would have meant pdfium or mupdf, a native library per platform. The cost is that a page drawn
  with vectors and no text layer cannot be OCR'd — it gets a note saying so. Only `DCTDecode` and
  `JPXDecode` pass through, because those are already complete image files; a `FlateDecode` raw
  bitmap or a `CCITTFaxDecode` fax image would have to be re-encoded first.
- **OCR is the vision model, not an OCR engine.** Tesseract needs a system library, and the
  pure-Rust alternatives are weak on Cyrillic. Qwen2.5-VL was already in the catalogue, reads
  Russian and handwriting, and runs on the engine TASK_028_EXTRA_1 installs — so OCR ships without
  a single new binary.
- **Recognised text replaces a page's own text, rather than joining it.** A scanned page's "text
  layer" is usually a stray header the scanner's software added; keeping both indexes it twice.
- **An extractor that declines is a skip, not a failure.** `UNSUPPORTED_FORMAT` out of an extractor
  is now caught by `ingest` and counted as skipped with the extractor's own message, so a picture
  in a folder of notes with OCR off reads as «распознавание выключено» rather than an error the
  user has to investigate. An empty extraction is likewise a skip, not a document with no vectors.

**Next session should know:**

- **Vision and embedding servers run side by side.** `--embeddings` puts llama.cpp in a pooling
  mode that cannot generate, so one process cannot do both; `RuntimeService` keys its servers by
  `runtimeId:role` and indexing a folder of scans holds two models in VRAM at once. On an 8 GB card
  with a 7B vision model that is tight — the idle timeout (5 min) is what keeps it survivable.
- **The projector is found by name, not configured.** `projectorFor` looks for `mmproj*.gguf`
  beside the model and, with several present, picks the one sharing the most of the model's name.
  Both files must be downloaded; the catalogue entry says so in its description.
- **OCR is serial by design.** The vision model holds the GPU for a whole page, so a second request
  would queue anyway, and going in order keeps cancellation immediate. Expect single-digit seconds
  per page on a GPU and far worse on CPU — `minCharsPerPage` is the setting that keeps this from
  running on pages that do not need it.
- TASK_042/TASK_043 inherit a working PDF path. What is left for them is rendering (the viewer) and
  the Office → PDF conversion that `.doc`/`.xls`/`.ppt` still need.
- `pnpm format:check` still reports the drizzle-generated `meta/*.json`, as it did before.
