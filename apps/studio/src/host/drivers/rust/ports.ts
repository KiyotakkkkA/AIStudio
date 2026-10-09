export interface ChunkConfig {
  size: number;
  overlap: number;
}

export interface TextChunk {
  text: string;
  byteStart: number;
  byteEnd: number;
  tokenCount: number;
}

export interface TextSegment {
  text: string;
  section: string[];
  page?: number;
}

export interface SectionChunk extends TextChunk {
  section: string[];
  page: number | null;
}

export interface RustCorePort {
  chunk(text: string, config?: ChunkConfig): Promise<TextChunk[]>;
  chunkSegments(segments: readonly TextSegment[], config: ChunkConfig): Promise<SectionChunk[]>;
  hash(bytes: Uint8Array): Promise<string>;
}
