import { app } from "electron";
import type { DeviceGpuDto } from "@zvs/shared";
import type { GpuProbe } from "./device.ts";
import type { Logger } from "./logger.ts";

const VENDOR_NAMES: Record<number, string> = {
  0x10de: "NVIDIA",
  0x1002: "AMD",
  0x8086: "Intel",
  0x106b: "Apple",
};

interface GpuDevice {
  active?: boolean;
  vendorId?: number;
  deviceId?: number;
  driverVersion?: string;
}

interface GpuInfo {
  gpuDevice?: GpuDevice[];
  auxAttributes?: { glRenderer?: string; glVendor?: string };
}

/**
 * Chromium already enumerated the adapters, so ask it rather than shelling out to a
 * platform tool. It reports no VRAM, and `discrete` is a heuristic: a dedicated vendor, or a
 * second adapter on a hybrid laptop. Both are honest enough to pick a default with; nothing
 * here is presented to the user as a measurement.
 */
export function electronGpuProbe(logger?: Logger): GpuProbe {
  return async (): Promise<DeviceGpuDto | null> => {
    let info: GpuInfo;
    try {
      info = (await app.getGPUInfo("complete")) as GpuInfo;
    } catch (error: unknown) {
      logger?.log("warn", "system", "Could not read the GPU information", {
        error: String(error),
      });
      return null;
    }
    const devices = info.gpuDevice ?? [];
    const device = devices.find((candidate) => candidate.active === true) ?? devices[0];
    if (!device) return null;
    const vendor =
      (device.vendorId === undefined ? undefined : VENDOR_NAMES[device.vendorId]) ??
      info.auxAttributes?.glVendor ??
      "unknown";
    return {
      vendor,
      model: info.auxAttributes?.glRenderer ?? "unknown",
      vramBytes: null,
      discrete: device.vendorId === 0x10de || device.vendorId === 0x1002 || devices.length > 1,
    };
  };
}
