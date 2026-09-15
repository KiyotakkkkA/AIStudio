import type { DeviceProfileDto, ResourceSampleDto } from "@zvs/shared";
import type { RustCorePort } from "../drivers/rust/ports.ts";
import { DeviceProbe } from "../platform/device.ts";
import { ResourceMonitor } from "../platform/metrics.ts";

export interface SystemServiceOptions {
  device?: DeviceProbe;
  resources?: ResourceMonitor;
}

export class SystemService {
  private readonly device: DeviceProbe;
  private readonly resources: ResourceMonitor;

  constructor(
    private readonly rust: RustCorePort,
    options: SystemServiceOptions = {},
  ) {
    this.device = options.device ?? new DeviceProbe();
    this.resources = options.resources ?? new ResourceMonitor();
  }

  async nativePing(text: string): Promise<{ count: number }> {
    const chunks = await this.rust.chunk(text);
    return { count: chunks.length };
  }

  deviceProfile(refresh = false): Promise<DeviceProfileDto> {
    return this.device.profile(refresh);
  }

  /** One reading, for a page that wants to show the machine before any job is running. */
  resourceSample(): Promise<ResourceSampleDto> {
    return this.resources.sample();
  }
}
