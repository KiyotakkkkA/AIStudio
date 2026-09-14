import type { DiskUsageDto } from "@zvs/shared";
import {
  DISK_CATEGORY_COLORS,
  DISK_CATEGORY_LABELS,
  DISK_CATEGORY_ORDER,
  formatBytes,
} from "./downloadPresentation";

export interface DiskBreakdownProps {
  readonly disk: DiskUsageDto | null;
}

export default function DiskBreakdown({ disk }: DiskBreakdownProps) {
  if (disk === null) {
    return (
      <div className="rounded-card border border-main-750 bg-main-900 p-3 text-[11px] text-main-500">
        Занятое место ещё не измерено.
      </div>
    );
  }
  const total = Math.max(disk.totalBytes, 1);
  const measured = DISK_CATEGORY_ORDER.map((category) => ({
    category,
    bytes: disk.categories.find((entry) => entry.category === category)?.bytes ?? 0,
  })).filter((entry) => entry.bytes > 0);

  return (
    <div className="flex flex-col gap-2 rounded-card border border-main-750 bg-main-900 p-3">
      <div className="flex justify-between text-[11px] text-main-400">
        <span>Диск</span>
        <span className="font-mono">
          {formatBytes(disk.usedBytes)} / {formatBytes(disk.totalBytes)}
        </span>
      </div>
      <div className="flex h-1.5 overflow-hidden rounded-sm bg-main-700">
        {measured.map((entry) => (
          <span
            key={entry.category}
            className={DISK_CATEGORY_COLORS[entry.category]}
            style={{ width: `${String((entry.bytes / total) * 100)}%` }}
          />
        ))}
      </div>
      <div className="flex flex-col gap-1 text-[10.5px] text-main-500">
        {measured.length === 0 ? (
          <span>Загруженных артефактов пока нет.</span>
        ) : (
          measured.map((entry) => (
            <span key={entry.category} className="flex items-center gap-1.5">
              <span
                className={`size-1.75 flex-none rounded-full ${DISK_CATEGORY_COLORS[entry.category]}`}
              />
              {DISK_CATEGORY_LABELS[entry.category]} {formatBytes(entry.bytes)}
            </span>
          ))
        )}
        <span className="mt-0.5 font-mono text-main-600">
          свободно {formatBytes(disk.freeBytes)}
        </span>
      </div>
    </div>
  );
}
