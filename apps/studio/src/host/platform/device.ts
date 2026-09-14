import { arch, cpus, freemem, platform, totalmem } from "node:os";
import type { DeviceGpuDto, DeviceProfileDto, DeviceTier, Timestamp } from "@zvs/shared";

const GIB = 1024 ** 3;

/** What the probe needs from the machine, so a test can describe one without owning it. */
export interface DeviceReadings {
  platform: string;
  arch: string;
  cpuModel: string;
  cpuCores: number;
  totalMemoryBytes: number;
  freeMemoryBytes: number;
}

export type GpuProbe = () => Promise<DeviceGpuDto | null>;
export type FreeDiskProbe = () => Promise<number>;

export interface DeviceProbeOptions {
  readings?: () => DeviceReadings;
  gpu?: GpuProbe;
  freeDisk?: FreeDiskProbe;
  clock?: () => number;
}

export function osReadings(): DeviceReadings {
  const processors = cpus();
  return {
    platform: platform(),
    arch: arch(),
    cpuModel: processors[0]?.model.trim() ?? "unknown",
    cpuCores: processors.length,
    totalMemoryBytes: totalmem(),
    freeMemoryBytes: freemem(),
  };
}

/**
 * Three buckets, decided by the two numbers that actually gate a local model: how much RAM
 * there is to load weights into and how many cores there are to run them on. A discrete GPU
 * lifts a machine one bucket, because it is what makes a vision model usable rather than
 * merely possible.
 */
export function deviceTier(readings: DeviceReadings, gpu: DeviceGpuDto | null): DeviceTier {
  const memoryGib = readings.totalMemoryBytes / GIB;
  const base: DeviceTier =
    memoryGib >= 30 && readings.cpuCores >= 12
      ? "high"
      : memoryGib >= 14 && readings.cpuCores >= 6
        ? "medium"
        : "low";
  if (!gpu?.discrete) return base;
  return base === "low" ? "medium" : "high";
}

/**
 * Measured on demand and cached, because `cpus()` walks every core and the GPU probe crosses
 * into Chromium. `refresh` is the caller saying the machine may have changed underneath it.
 */
export class DeviceProbe {
  private cached: DeviceProfileDto | undefined;
  private pending: Promise<DeviceProfileDto> | undefined;

  constructor(private readonly options: DeviceProbeOptions = {}) {}

  async profile(refresh = false): Promise<DeviceProfileDto> {
    if (!refresh && this.cached) return this.cached;
    this.pending ??= this.measure().finally(() => {
      this.pending = undefined;
    });
    return this.pending;
  }

  private async measure(): Promise<DeviceProfileDto> {
    const readings = (this.options.readings ?? osReadings)();
    const gpu = await settle(this.options.gpu, null);
    const freeDiskBytes = await settle(this.options.freeDisk, 0);
    const profile: DeviceProfileDto = {
      ...readings,
      freeDiskBytes: Math.max(0, Math.trunc(freeDiskBytes)),
      gpu,
      tier: deviceTier(readings, gpu),
      measuredAt: (this.options.clock ?? Date.now)() as Timestamp,
    };
    this.cached = profile;
    return profile;
  }
}

/** A probe that cannot answer must not cost the caller its profile. */
async function settle<T>(probe: (() => Promise<T>) | undefined, fallback: T): Promise<T> {
  if (!probe) return fallback;
  try {
    return await probe();
  } catch {
    return fallback;
  }
}
