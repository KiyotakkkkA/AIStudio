import { Skeleton } from "@kiyotakkkka/zvs-uikit-lib";

export default function ListRowSkeleton() {
  return (
    <div className="flex gap-[11px] rounded-[10px] border border-main-700 bg-main-800 p-[12px]">
      <Skeleton className="size-[32px] flex-none rounded-[8px]" />
      <div className="flex min-w-0 flex-1 flex-col gap-[7px]">
        <Skeleton className="h-[12px] w-[60%] rounded-[4px]" />
        <Skeleton className="h-[10px] w-[45%] rounded-[4px]" />
        <Skeleton className="h-[10px] w-[75%] rounded-[4px]" />
      </div>
    </div>
  );
}
