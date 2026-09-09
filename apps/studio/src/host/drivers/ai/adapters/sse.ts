export const SSE_DONE = "[DONE]";

export async function* sseData(chunks: AsyncIterable<Uint8Array>): AsyncIterable<string> {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of chunks) {
    buffer += decoder.decode(chunk, { stream: true });
    let boundary = nextBoundary(buffer);
    while (boundary !== null) {
      const frame = buffer.slice(0, boundary.index);
      buffer = buffer.slice(boundary.index + boundary.length);
      const payload = frameData(frame);
      if (payload !== null) {
        if (payload === SSE_DONE) return;
        yield payload;
      }
      boundary = nextBoundary(buffer);
    }
  }
  buffer += decoder.decode();
  const payload = frameData(buffer);
  if (payload !== null && payload !== SSE_DONE) yield payload;
}

function nextBoundary(buffer: string): { index: number; length: number } | null {
  const double = buffer.indexOf("\n\n");
  const windows = buffer.indexOf("\r\n\r\n");
  if (windows !== -1 && (double === -1 || windows < double)) return { index: windows, length: 4 };
  if (double !== -1) return { index: double, length: 2 };
  return null;
}

function frameData(frame: string): string | null {
  const parts: string[] = [];
  for (const raw of frame.split(/\r?\n/)) {
    const line = raw.trimEnd();
    if (!line.startsWith("data:")) continue;
    parts.push(line.slice(5).replace(/^ /, ""));
  }
  if (parts.length === 0) return null;
  const joined = parts.join("\n");
  return joined.length === 0 ? null : joined;
}
