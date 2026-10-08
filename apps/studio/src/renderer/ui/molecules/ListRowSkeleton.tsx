import { Skeleton } from "@kiyotakkkka/zvs-uikit-lib";

export default function ListRowSkeleton() {
  return (
    <div className="flex gap-3 rounded-card border border-main-750 bg-main-900 px-3.5 py-3">
      <Skeleton className="size-9 flex-none rounded-control" />
      <div className="flex min-w-0 flex-1 flex-col gap-1.75">
        <Skeleton className="h-3 w-[60%] rounded-sm" />
        <Skeleton className="h-2.5 w-[45%] rounded-sm" />
        <Skeleton className="h-2.5 w-[75%] rounded-sm" />
      </div>
    </div>
  );
}
