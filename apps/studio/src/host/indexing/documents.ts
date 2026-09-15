import { readFile, rm } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import { AppError, AppErrorCode, type Json, type VectorOcrConfig } from "@zvs/shared";
import type { SidecarJobsPort } from "../drivers/sidecar/SidecarDriver.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { ExtractionInput, Extractor } from "./extraction.ts";

export const DOCUMENT_JOB = "job.document.extract";

/** What the vision model is asked to read. Implemented by `RuntimeService.readImage`. */
export interface OcrPort {
  readImage(
    modelRef: string,
    image: { readonly mediaType: string; readonly bytes: Uint8Array },
    language: VectorOcrConfig["language"],
    signal: AbortSignal,
  ): Promise<string>;
}

export interface DocumentExtractorOptions {
  jobs: SidecarJobsPort;
  ocr?: OcrPort;
  /** Where page images are written while a document is being read, then deleted. */
  scratchDir: string;
  logger?: Logger;
}

interface ExtractedPage {
  number: number;
  text: string;
  chars: number;
}

interface ExtractedImage {
  page: number;
  path: string;
  mediaType: string;
}

interface ExtractedNote {
  page: number;
  reason: string;
}

interface DocumentReport {
  pages: readonly ExtractedPage[];
  images: readonly ExtractedImage[];
  notes: readonly ExtractedNote[];
}

/**
 * PDF text through the sidecar, then OCR for the pages that have none.
 *
 * Parsing runs in Rust, in the child process, because a PDF is the least trustworthy input the
 * app accepts: the parsers that survive real-world files do so by panicking, and a panic there
 * costs one job rather than the window. See `crates/zvs-core/src/pdf.rs`.
 */
export function createPdfExtractor(options: DocumentExtractorOptions): Extractor {
  return async (input: ExtractionInput): Promise<string> => {
    const ocr = usableOcr(input, options);
    const scratch = ocr === undefined ? undefined : join(options.scratchDir, createId());
    try {
      const report = await runJob(options.jobs, {
        path: input.path,
        minCharsPerPage: ocr?.config.minCharsPerPage ?? 0,
        ...(scratch === undefined ? {} : { imagesDir: scratch }),
      });
      for (const note of report.notes)
        input.context?.note?.(`${basename(input.path)}, с. ${String(note.page)}: ${note.reason}`);

      const recognised = await recogniseAll(report, ocr, input, options);
      return assemble(report.pages, recognised);
    } finally {
      if (scratch !== undefined) await rm(scratch, { recursive: true, force: true });
    }
  };
}

/**
 * A standalone image is a page with no text layer at all, so it takes the same path as a scanned
 * PDF page — straight to the vision model, with no extraction step in front of it.
 */
export function createImageExtractor(options: DocumentExtractorOptions): Extractor {
  return async (input: ExtractionInput): Promise<string> => {
    const ocr = usableOcr(input, options);
    if (ocr === undefined)
      throw new AppError(
        AppErrorCode.UNSUPPORTED_FORMAT,
        "распознавание выключено — включите OCR в настройках хранилища",
      );
    const mediaType = IMAGE_MEDIA_TYPES[extname(input.path).toLowerCase()];
    if (mediaType === undefined)
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Этот формат изображения не читается");
    const text = await ocr.port.readImage(
      ocr.config.modelRef,
      { mediaType, bytes: input.bytes },
      ocr.config.language,
      input.context?.signal ?? new AbortController().signal,
    );
    options.logger?.log("debug", "indexing", "Read an image with the vision model", {
      path: input.path,
      chars: text.length,
    });
    return text;
  };
}

const IMAGE_MEDIA_TYPES: Record<string, string | undefined> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".webp": "image/webp",
  ".bmp": "image/bmp",
  ".gif": "image/gif",
};

export const IMAGE_EXTENSIONS = Object.keys(IMAGE_MEDIA_TYPES);

interface UsableOcr {
  readonly port: OcrPort;
  readonly config: VectorOcrConfig;
}

/** OCR runs only when it is switched on, wired to a model, and there is something to run it. */
function usableOcr(
  input: ExtractionInput,
  options: DocumentExtractorOptions,
): UsableOcr | undefined {
  const config = input.context?.ocr;
  if (options.ocr === undefined || config === undefined) return undefined;
  if (!config.enabled || config.modelRef === "") return undefined;
  return { port: options.ocr, config };
}

async function runJob(
  jobs: SidecarJobsPort,
  params: Record<string, Json>,
): Promise<DocumentReport> {
  const result = await jobs.run(DOCUMENT_JOB, params as Json);
  if (result === null || typeof result !== "object" || Array.isArray(result))
    throw new AppError(AppErrorCode.VALIDATION_FAILED, "Не удалось прочитать документ");
  const report = result as unknown as Partial<DocumentReport>;
  return {
    pages: Array.isArray(report.pages) ? report.pages : [],
    images: Array.isArray(report.images) ? report.images : [],
    notes: Array.isArray(report.notes) ? report.notes : [],
  };
}

/**
 * Pages are read one at a time rather than in parallel: the vision model holds the whole GPU for
 * the duration of a page, so a second request would queue behind the first anyway — and doing it
 * in order keeps cancellation immediate.
 */
async function recogniseAll(
  report: DocumentReport,
  ocr: UsableOcr | undefined,
  input: ExtractionInput,
  options: DocumentExtractorOptions,
): Promise<Map<number, string>> {
  const recognised = new Map<number, string>();
  if (ocr === undefined) return recognised;
  const signal = input.context?.signal;
  for (const image of report.images) {
    signal?.throwIfAborted();
    try {
      const bytes = await readFile(image.path);
      const text = await ocr.port.readImage(
        ocr.config.modelRef,
        { mediaType: image.mediaType, bytes },
        ocr.config.language,
        signal ?? new AbortController().signal,
      );
      if (text.trim() !== "") recognised.set(image.page, text.trim());
    } catch (error: unknown) {
      if (signal?.aborted === true) throw error;
      // One unreadable page must not cost the document the pages that did read.
      input.context?.note?.(
        `${basename(input.path)}, с. ${String(image.page)}: распознать не удалось — ${describe(error)}`,
      );
      options.logger?.log("warn", "indexing", "Could not OCR a page", {
        path: input.path,
        page: image.page,
        error: String(error),
      });
    }
  }
  return recognised;
}

/**
 * The recognised text replaces a page's own text rather than joining it: a scanned page's "text
 * layer" is usually a stray header or page number, and keeping both would index it twice.
 */
function assemble(
  pages: readonly ExtractedPage[],
  recognised: ReadonlyMap<number, string>,
): string {
  return pages
    .map((page) => recognised.get(page.number) ?? page.text)
    .filter((text) => text.trim() !== "")
    .join("\n\n");
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
