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

export interface RustCorePort {
  chunk(text: string, config?: ChunkConfig): Promise<TextChunk[]>;
  hash(bytes: Uint8Array): Promise<string>;
}
