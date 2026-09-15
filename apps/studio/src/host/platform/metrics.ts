import { execFile } from "node:child_process";
import { cpus, freemem, totalmem } from "node:os";
import { promisify } from "node:util";
import type { ResourceSampleDto, Timestamp } from "@zvs/shared";
import type { Logger } from "./logger.ts";

const run = promisify(execFile);

interface CpuTotals {
  idle: number;
  total: number;
}

export interface GpuReading {
  readonly name: string;
  readonly utilisationPercent: number | null;
  readonly memoryUsedBytes: number | null;
  readonly memoryTotalBytes: number | null;
}

export type GpuSampler = (signal: AbortSignal) => Promise<GpuReading | null>;

export interface ResourceMonitorOptions {
  cpuTotals?: () => CpuTotals;
  memory?: () => { freeBytes: number; totalBytes: number };
  processMemory?: () => number;
  gpu?: GpuSampler;
  clock?: () => number;
  gpuTtlMs?: number;
  logger?: Logger;
}

export function osCpuTotals(): CpuTotals {
  let idle = 0;
  let total = 0;
  for (const core of cpus()) {
    idle += core.times.idle;
    total += core.times.user + core.times.nice + core.times.sys + core.times.idle + core.times.irq;
  }
  return { idle, total };
}

const MIB = 1024 * 1024;

/**
 * NVIDIA is the only vendor with a counter we can read without shipping a driver library, so
 * it is the only one sampled. Everyone else gets `null` — a missing reading is shown as "н/д",
 * never as zero, because a zero would read as "the GPU is idle" rather than "we cannot see it".
 */
export function nvidiaSmiSampler(
  logger?: Logger,
  exec: typeof run = run,
): GpuSampler & { disabled: () => boolean } {
  let unavailable = false;
  const sampler = async (signal: AbortSignal): Promise<GpuReading | null> => {
    if (unavailable) return null;
    try {
      const { stdout } = await exec(
        "nvidia-smi",
        [
          "--query-gpu=name,utilization.gpu,memory.used,memory.total",
          "--format=csv,noheader,nounits",
        ],
        { signal, timeout: 4_000, windowsHide: true },
      );
      const line = stdout.split("\n").find((candidate) => candidate.trim() !== "");
      if (line === undefined) return null;
      const [name, utilisation, used, total] = line.split(",").map((part) => part.trim());
      return {
        name: name ?? "NVIDIA",
        utilisationPercent: numberOrNull(utilisation),
        memoryUsedBytes: scaled(numberOrNull(used), MIB),
        memoryTotalBytes: scaled(numberOrNull(total), MIB),
      };
    } catch (error: unknown) {
      // Absent on most machines. Probe once, then stop paying for the spawn on every tick.
      unavailable = true;
      logger?.log("debug", "system", "No NVIDIA counters on this machine", {
        error: String(error),
      });
      return null;
    }
  };
  return Object.assign(sampler, { disabled: () => unavailable });
}

/**
 * CPU load is a delta between two readings, so the monitor is stateful and one instance is
 * shared by everything that wants a sample. The first sample after construction compares
 * against boot totals, which is close enough to be useful and converges within one tick.
 */
export class ResourceMonitor {
  private previous: CpuTotals;
  private gpuReading: GpuReading | null = null;
  private gpuReadAt = 0;
  private gpuPending: Promise<GpuReading | null> | undefined;

  constructor(private readonly options: ResourceMonitorOptions = {}) {
    this.previous = this.totals();
  }

  async sample(signal?: AbortSignal): Promise<ResourceSampleDto> {
    const gpu = await this.readGpu(signal);
    const memory = (this.options.memory ?? osMemory)();
    return {
      at: this.now() as Timestamp,
      cpuPercent: this.cpuPercent(),
      memoryUsedBytes: Math.max(0, memory.totalBytes - memory.freeBytes),
      memoryTotalBytes: memory.totalBytes,
      processMemoryBytes: (this.options.processMemory ?? processMemory)(),
      gpuName: gpu?.name ?? null,
      gpuPercent: gpu?.utilisationPercent ?? null,
      vramUsedBytes: gpu?.memoryUsedBytes ?? null,
      vramTotalBytes: gpu?.memoryTotalBytes ?? null,
    };
  }

  /**
   * Samples on an interval until the returned function is called. Ticks never overlap: a slow
   * GPU probe delays the next sample instead of queueing another one behind it.
   */
  watch(intervalMs: number, onSample: (sample: ResourceSampleDto) => void): () => void {
    let stopped = false;
    const controller = new AbortController();
    const tick = async (): Promise<void> => {
      if (stopped) return;
      try {
        const sample = await this.sample(controller.signal);
        if (!stopped) onSample(sample);
      } catch {
        // A sample that cannot be taken is not worth failing the job it was decorating.
      }
      if (!stopped) timer = setTimeout(() => void tick(), intervalMs);
    };
    let timer = setTimeout(() => void tick(), intervalMs);
    return () => {
      stopped = true;
      clearTimeout(timer);
      controller.abort();
    };
  }

  private cpuPercent(): number {
    const current = this.totals();
    const idle = current.idle - this.previous.idle;
    const total = current.total - this.previous.total;
    this.previous = current;
    if (total <= 0) return 0;
    return clamp(((total - idle) / total) * 100);
  }

  private async readGpu(signal?: AbortSignal): Promise<GpuReading | null> {
    const sampler = this.options.gpu;
    if (sampler === undefined) return null;
    const ttl = this.options.gpuTtlMs ?? 1_000;
    if (this.gpuReadAt !== 0 && this.now() - this.gpuReadAt < ttl) return this.gpuReading;
    this.gpuPending ??= sampler(signal ?? AbortSignal.timeout(5_000))
      .then((reading) => {
        this.gpuReading = reading;
        this.gpuReadAt = this.now();
        return reading;
      })
      .catch(() => null)
      .finally(() => {
        this.gpuPending = undefined;
      });
    return this.gpuPending;
  }

  private totals(): CpuTotals {
    return (this.options.cpuTotals ?? osCpuTotals)();
  }
  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
}

function osMemory(): { freeBytes: number; totalBytes: number } {
  return { freeBytes: freemem(), totalBytes: totalmem() };
}

function processMemory(): number {
  return process.memoryUsage.rss();
}

function numberOrNull(raw: string | undefined): number | null {
  if (raw === undefined) return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

function scaled(value: number | null, factor: number): number | null {
  return value === null ? null : Math.round(value * factor);
}

function clamp(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value * 10) / 10));
}
