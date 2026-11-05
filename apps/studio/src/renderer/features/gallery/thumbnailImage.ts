const THUMBNAIL_WIDTH = 480;

export async function pdfFirstPage(bytes: Uint8Array): Promise<Blob | null> {
  const { pdfLibrary } = await import("../../viewer/pdf");
  const task = pdfLibrary().getDocument({ data: bytes, useSystemFonts: true });
  const document = await task.promise;
  try {
    const page = await document.getPage(1);
    const scale = THUMBNAIL_WIDTH / page.getViewport({ scale: 1 }).width;
    const viewport = page.getViewport({ scale });
    const canvas = window.document.createElement("canvas");
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    const context = canvas.getContext("2d");
    if (context === null) return null;
    await page.render({ canvas, canvasContext: context, viewport }).promise;
    return await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  } finally {
    void task.destroy();
  }
}
