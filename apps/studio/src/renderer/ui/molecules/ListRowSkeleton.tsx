import { Skeleton } from "@kiyotakkkka/zvs-uikit-lib";

export default function ListRowSkeleton() {
  return (
    <div className="flex gap-2.75 rounded-card border border-main-700 bg-main-800 p-3">
      <Skeleton className="size-8 flex-none rounded-lg" />
      <div className="flex min-w-0 flex-1 flex-col gap-1.75">
        <Skeleton className="h-3 w-[60%] rounded-sm" />
        <Skeleton className="h-2.5 w-[45%] rounded-sm" />
        <Skeleton className="h-2.5 w-[75%] rounded-sm" />
      </div>
    </div>
  );
}
