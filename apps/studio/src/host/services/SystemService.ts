import type { RustCorePort } from "../drivers/rust/ports.ts";

export class SystemService {
  constructor(private readonly rust: RustCorePort) {}

  async nativePing(text: string): Promise<{ count: number }> {
    const chunks = await this.rust.chunk(text);
    return { count: chunks.length };
  }
}
