import type { DeviceProfileDto } from "@zvs/shared";
import type { RustCorePort } from "../drivers/rust/ports.ts";
import { DeviceProbe } from "../platform/device.ts";

export interface SystemServiceOptions {
  device?: DeviceProbe;
}

export class SystemService {
  private readonly device: DeviceProbe;

  constructor(
    private readonly rust: RustCorePort,
    options: SystemServiceOptions = {},
  ) {
    this.device = options.device ?? new DeviceProbe();
  }

  async nativePing(text: string): Promise<{ count: number }> {
    const chunks = await this.rust.chunk(text);
    return { count: chunks.length };
  }

  deviceProfile(refresh = false): Promise<DeviceProfileDto> {
    return this.device.profile(refresh);
  }
}
