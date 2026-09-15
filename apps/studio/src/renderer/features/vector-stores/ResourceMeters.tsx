import type { ResourceSampleDto } from "@zvs/shared";
import { formatBytes } from "../downloads/downloadPresentation";

interface Meter {
  readonly label: string;
  /** `null` means "we cannot see this counter", which is rendered as «н/д», never as zero. */
  readonly percent: number | null;
  readonly detail: string;
}

/** Amber past 80 %, red past 95 %: the point at which the machine is the bottleneck. */
function barColour(percent: number): string {
  if (percent >= 95) return "bg-err";
  if (percent >= 80) return "bg-warn";
  return "bg-accent-dark";
}

export function metersOf(sample: ResourceSampleDto): readonly Meter[] {
  const memoryPercent =
    sample.memoryTotalBytes > 0 ? (sample.memoryUsedBytes / sample.memoryTotalBytes) * 100 : 0;
  const vramPercent =
    sample.vramUsedBytes !== null && sample.vramTotalBytes !== null && sample.vramTotalBytes > 0
      ? (sample.vramUsedBytes / sample.vramTotalBytes) * 100
      : null;
  return [
    { label: "CPU", percent: sample.cpuPercent, detail: `${sample.cpuPercent.toFixed(0)} %` },
    {
      label: "RAM",
      percent: memoryPercent,
      detail: `${formatBytes(sample.memoryUsedBytes)} из ${formatBytes(sample.memoryTotalBytes)} · процесс ${formatBytes(sample.processMemoryBytes)}`,
    },
    {
      label: "GPU",
      percent: sample.gpuPercent,
      detail:
        sample.gpuPercent === null
          ? (sample.gpuName ?? "счётчик недоступен")
          : `${sample.gpuPercent.toFixed(0)} % · ${sample.gpuName ?? "GPU"}`,
    },
    {
      label: "VRAM",
      percent: vramPercent,
      detail:
        vramPercent === null
          ? "счётчик недоступен"
          : `${formatBytes(sample.vramUsedBytes ?? 0)} из ${formatBytes(sample.vramTotalBytes ?? 0)}`,
    },
  ];
}

/**
 * What the machine is doing while the index runs. Shown here rather than on a global status bar
 * because this is the only screen that starts work heavy enough to matter, and because the
 * question it answers — "is my GPU actually being used?" — is asked about *this* job.
 */
export default function ResourceMeters({ sample }: { readonly sample: ResourceSampleDto }) {
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4">
      {metersOf(sample).map((meter) => (
        <div key={meter.label} className="flex min-w-0 flex-col gap-1">
          <div className="flex items-baseline justify-between gap-2">
            <span className="text-[10.5px] font-semibold tracking-[0.06em] text-main-400 uppercase">
              {meter.label}
            </span>
            <span className="font-mono text-[10.5px] text-main-500">
              {meter.percent === null ? "н/д" : `${meter.percent.toFixed(0)} %`}
            </span>
          </div>
          <div
            role="meter"
            aria-label={meter.label}
            aria-valuenow={meter.percent ?? undefined}
            aria-valuemin={0}
            aria-valuemax={100}
            className="h-1 rounded bg-main-700"
          >
            {meter.percent === null ? null : (
              <div
                className={`h-1 rounded ${barColour(meter.percent)}`}
                style={{ width: `${String(Math.min(100, Math.max(0, meter.percent)))}%` }}
              />
            )}
          </div>
          <span className="truncate font-mono text-[10px] text-main-600" title={meter.detail}>
            {meter.detail}
          </span>
        </div>
      ))}
    </div>
  );
}
