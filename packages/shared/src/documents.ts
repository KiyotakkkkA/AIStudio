import { z } from "zod";
import { ArtifactId } from "./artifacts.js";
import { ChatCitationDto } from "./chat.js";
import { DocumentId } from "./primitives/branded.js";

export const DOCUMENT_PATH_MAX = 32767;
export const DOCUMENT_TEXT_PAGE_MAX = 262_144;
export const DOCUMENT_BYTES_PAGE_MAX = 4_194_304;
export const DOCUMENT_BYTES_DATA_MAX = Math.ceil(DOCUMENT_BYTES_PAGE_MAX / 3) * 4;

const Count = z.number().int().nonnegative();
const Positive = z.number().int().positive();

export const DocumentKind = z.enum(["pdf", "image", "markdown", "code", "csv", "text", "office"]);
export type DocumentKind = z.infer<typeof DocumentKind>;

export const DocumentOfficeFormat = z.enum(["docx", "xlsx", "pptx"]);
export type DocumentOfficeFormat = z.infer<typeof DocumentOfficeFormat>;

export const DocumentTextEncoding = z.enum(["utf-8", "utf-16le", "utf-16be", "windows-1251"]);
export type DocumentTextEncoding = z.infer<typeof DocumentTextEncoding>;

export const DocumentRef = z.discriminatedUnion("source", [
  z.object({ source: z.literal("path"), path: z.string().min(1).max(DOCUMENT_PATH_MAX) }),
  z.object({
    source: z.literal("citation"),
    citation: ChatCitationDto.extend({ documentId: z.string().min(1) }),
  }),
  z.object({ source: z.literal("storeDocument"), documentId: DocumentId }),
  z.object({ source: z.literal("artifact"), artifactId: ArtifactId }),
]);
export type DocumentRef = z.infer<typeof DocumentRef>;

export const DocumentDto = z
  .object({
    id: DocumentId,
    contentHash: z.string().min(1).max(128),
    name: z.string().min(1).max(1024),
    sourcePath: z.string().min(1).max(DOCUMENT_PATH_MAX),
    kind: DocumentKind,
    mimeType: z.string().min(1).max(128),
    officeFormat: DocumentOfficeFormat.optional(),
    language: z.string().min(1).max(64).optional(),
    sizeBytes: Count,
    pageCount: Positive.optional(),
    partCount: Positive,
    initialPage: Positive.optional(),
    open: z.boolean(),
    openedAt: Count,
  })
  .refine((dto) => (dto.officeFormat !== undefined) === (dto.kind === "office"), {
    path: ["officeFormat"],
    message: "kind",
  });
export type DocumentDto = z.infer<typeof DocumentDto>;

export const DocumentHandle = z.object({ id: DocumentId });
export type DocumentHandle = z.infer<typeof DocumentHandle>;

export const DocumentPageInput = z.object({ id: DocumentId, index: Count });
export type DocumentPageInput = z.infer<typeof DocumentPageInput>;

export const DocumentPageDto = z
  .discriminatedUnion("content", [
    z.object({
      content: z.literal("bytes"),
      index: Count,
      partCount: Positive,
      offset: Count,
      totalBytes: Count,
      data: z.string().max(DOCUMENT_BYTES_DATA_MAX),
    }),
    z.object({
      content: z.literal("text"),
      index: Count,
      partCount: Positive,
      firstLine: Positive,
      encoding: DocumentTextEncoding,
      text: z.string().max(DOCUMENT_TEXT_PAGE_MAX),
      untrusted: z.literal(true),
    }),
  ])
  .refine((page) => page.index < page.partCount, { path: ["index"] })
  .refine((page) => page.content !== "bytes" || page.offset <= page.totalBytes, {
    path: ["offset"],
  });
export type DocumentPageDto = z.infer<typeof DocumentPageDto>;

export const DocumentSaveResult = z.object({
  path: z.string().min(1).max(DOCUMENT_PATH_MAX).optional(),
});
export type DocumentSaveResult = z.infer<typeof DocumentSaveResult>;
