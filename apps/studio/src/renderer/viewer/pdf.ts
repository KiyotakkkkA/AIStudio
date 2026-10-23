import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

let worker: Worker | null = null;

export function pdfLibrary(): typeof pdfjs {
  if (worker === null) {
    worker = new Worker(workerUrl, { type: "module" });
    pdfjs.GlobalWorkerOptions.workerPort = worker;
  }
  return pdfjs;
}

export type PdfDocument = pdfjs.PDFDocumentProxy;
export type PdfPage = pdfjs.PDFPageProxy;
