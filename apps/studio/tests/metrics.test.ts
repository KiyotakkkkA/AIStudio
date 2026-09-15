import { expect, test, vi } from "vitest";
import {
  ResourceMonitor,
  nvidiaSmiSampler,
  type GpuReading,
} from "../src/host/platform/metrics.ts";

const GIB = 1024 ** 3;

function monitorOver(
  totals: readonly { idle: number; total: number }[],
  gpu?: () => Promise<GpuReading | null>,
): ResourceMonitor {
  let index = 0;
  return new ResourceMonitor({
    cpuTotals: () => totals[Math.min(index++, totals.length - 1)]!,
    memory: () => ({ freeBytes: 4 * GIB, totalBytes: 16 * GIB }),
    processMemory: () => 512 * 1024 * 1024,
    ...(gpu === undefined ? {} : { gpu: () => gpu() }),
    clock: () => 1_000,
    gpuTtlMs: 0,
  });
}

test("cpu load is the delta between two readings, not a snapshot", async () => {
  // First reading is consumed by the constructor; the sample below compares against it.
  const monitor = monitorOver([
    { idle: 1000, total: 2000 },
    { idle: 1100, total: 2400 },
  ]);

  const sample = await monitor.sample();

  // 400 ticks passed, 100 of them idle: the machine was busy three quarters of the time.
  expect(sample.cpuPercent).toBe(75);
  expect(sample.memoryUsedBytes).toBe(12 * GIB);
  expect(sample.memoryTotalBytes).toBe(16 * GIB);
  expect(sample.processMemoryBytes).toBe(512 * 1024 * 1024);
});

test("a machine with no readable GPU reports null, never zero", async () => {
  const sample = await monitorOver([{ idle: 1, total: 2 }]).sample();

  expect(sample.gpuName).toBeNull();
  expect(sample.gpuPercent).toBeNull();
  expect(sample.vramUsedBytes).toBeNull();
  expect(sample.vramTotalBytes).toBeNull();
});

test("a GPU reading is carried through in bytes", async () => {
  const monitor = monitorOver([{ idle: 1, total: 2 }], () =>
    Promise.resolve({
      name: "NVIDIA GeForce RTX 4070",
      utilisationPercent: 62,
      memoryUsedBytes: 3 * 1024 * 1024 * 1024,
      memoryTotalBytes: 12 * 1024 * 1024 * 1024,
    }),
  );

  const sample = await monitor.sample();

  expect(sample.gpuName).toBe("NVIDIA GeForce RTX 4070");
  expect(sample.gpuPercent).toBe(62);
  expect(sample.vramUsedBytes).toBe(3 * GIB);
});

test("a GPU probe that throws costs the sample nothing", async () => {
  const monitor = monitorOver([{ idle: 1, total: 2 }], () => Promise.reject(new Error("no smi")));

  const sample = await monitor.sample();

  expect(sample.gpuPercent).toBeNull();
  expect(sample.cpuPercent).toBeGreaterThanOrEqual(0);
});

test("nvidia-smi output is parsed into megabyte-scaled bytes", async () => {
  const exec = vi.fn().mockResolvedValue({
    stdout: "NVIDIA GeForce RTX 4070, 47, 3072, 12282\n",
    stderr: "",
  });
  const sampler = nvidiaSmiSampler(undefined, exec as never);

  const reading = await sampler(AbortSignal.timeout(1_000));

  expect(reading).toEqual({
    name: "NVIDIA GeForce RTX 4070",
    utilisationPercent: 47,
    memoryUsedBytes: 3072 * 1024 * 1024,
    memoryTotalBytes: 12282 * 1024 * 1024,
  });
});

test("a machine without nvidia-smi is probed once, then left alone", async () => {
  const exec = vi.fn().mockRejectedValue(new Error("ENOENT"));
  const sampler = nvidiaSmiSampler(undefined, exec as never);

  expect(await sampler(AbortSignal.timeout(1_000))).toBeNull();
  expect(await sampler(AbortSignal.timeout(1_000))).toBeNull();

  expect(exec).toHaveBeenCalledTimes(1);
  expect(sampler.disabled()).toBe(true);
});

test("watching stops sampling when the returned function is called", async () => {
  vi.useFakeTimers();
  try {
    const monitor = monitorOver([{ idle: 1, total: 2 }]);
    const samples: number[] = [];
    const stop = monitor.watch(10, (sample) => samples.push(sample.cpuPercent));

    await vi.advanceTimersByTimeAsync(35);
    const taken = samples.length;
    expect(taken).toBeGreaterThan(0);

    stop();
    await vi.advanceTimersByTimeAsync(100);
    expect(samples.length).toBe(taken);
  } finally {
    vi.useRealTimers();
  }
});
