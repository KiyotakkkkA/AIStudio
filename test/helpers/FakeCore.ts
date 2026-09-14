import { createHash } from "node:crypto";
import type {
  ChunkConfig,
  RustCorePort,
  TextChunk,
} from "../../apps/studio/src/host/drivers/rust/ports.ts";
import { FakeVectorCore } from "./FakeVectorCore.ts";

export class FakeCore extends FakeVectorCore implements RustCorePort {
  async chunk(text: string, config: ChunkConfig = { size: 4, overlap: 1 }): Promise<TextChunk[]> {
    const words = text.split(/\s+/).filter((word) => word.length > 0);
    const stride = Math.max(1, config.size - config.overlap);
    const chunks: TextChunk[] = [];
    for (let start = 0; start < words.length; start += stride) {
      const slice = words.slice(start, start + config.size);
      if (slice.length === 0) break;
      chunks.push({
        text: slice.join(" "),
        byteStart: start,
        byteEnd: start + slice.length,
        tokenCount: slice.length,
      });
      if (start + config.size >= words.length) break;
    }
    return chunks;
  }
  async hash(bytes: Uint8Array): Promise<string> {
    return createHash("sha256").update(bytes).digest("hex");
  }
}
