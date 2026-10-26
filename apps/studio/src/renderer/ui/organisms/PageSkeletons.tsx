import Skeleton from "../atoms/Skeleton";
import ListRowSkeleton from "../molecules/ListRowSkeleton";
import {
  LoadingStatus,
  PAGE_LOADING,
  SkeletonLines,
  lineWidth,
} from "../molecules/LoadingSkeleton";

function range(count: number): number[] {
  return Array.from({ length: count }, (_, index) => index);
}

export function ListSkeleton({
  rows = 5,
  label = PAGE_LOADING,
}: {
  readonly rows?: number;
  readonly label?: string;
}) {
  return (
    <LoadingStatus label={label} className="flex flex-col gap-2">
      {range(rows).map((index) => (
        <ListRowSkeleton key={index} />
      ))}
    </LoadingStatus>
  );
}

export function TableSkeleton({
  rows = 8,
  columns = 2,
  label = PAGE_LOADING,
  grouped = false,
}: {
  readonly rows?: number;
  readonly columns?: number;
  readonly label?: string;
  readonly grouped?: boolean;
}) {
  return (
    <LoadingStatus label={label} className="flex flex-col">
      {range(rows).map((index) => (
        <div key={index}>
          {grouped && index % 4 === 0 ? (
            <div className="flex items-center gap-2.5 border-b border-line bg-main-850 px-5 py-2.5">
              <Skeleton rounded="sm" className="h-2.5 w-28" />
              <span className="flex-1" />
              <Skeleton rounded="full" className="h-4 w-7" />
            </div>
          ) : null}
          <div className="flex items-center gap-3 border-b border-line px-5 py-3">
            <Skeleton rounded="md" className="size-7 flex-none" />
            <div className="flex min-w-0 flex-1 flex-col gap-1.75">
              <Skeleton rounded="sm" className={`h-3 ${lineWidth(index + 1)} max-w-80`} />
              <Skeleton rounded="sm" className="h-2.5 w-[38%] max-w-52" />
            </div>
            {range(columns).map((column) => (
              <Skeleton
                key={column}
                rounded="full"
                className={`h-5 flex-none ${column === 0 ? "w-20" : "w-14"}`}
              />
            ))}
          </div>
        </div>
      ))}
    </LoadingStatus>
  );
}

export function CardListSkeleton({
  rows = 4,
  label = PAGE_LOADING,
}: {
  readonly rows?: number;
  readonly label?: string;
}) {
  return (
    <LoadingStatus label={label} className="flex flex-col gap-2">
      {range(rows).map((index) => (
        <div
          key={index}
          className="flex items-center gap-3 rounded-card border border-line bg-main-850 px-4 py-3 shadow-card"
        >
          <Skeleton rounded="full" className="size-2.5 flex-none" />
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            <Skeleton rounded="sm" className={`h-3.5 ${lineWidth(index)} max-w-120`} />
            <Skeleton rounded="sm" className="h-2.5 w-[34%] max-w-56" />
          </div>
          <Skeleton rounded="lg" className="size-8 flex-none" />
        </div>
      ))}
    </LoadingStatus>
  );
}

export function PaneSkeleton({
  label = PAGE_LOADING,
  cards = 2,
}: {
  readonly label?: string;
  readonly cards?: number;
}) {
  return (
    <LoadingStatus label={label} className="flex min-w-0 flex-1 flex-col">
      <div className="flex h-11.5 flex-none items-center gap-2 border-b border-line px-4.5">
        <Skeleton rounded="sm" className="h-3.5 w-48" />
        <span className="flex-1" />
        <Skeleton rounded="lg" className="size-7" />
        <Skeleton rounded="lg" className="h-7 w-24" />
      </div>
      <div className="flex flex-col gap-3.5 px-4.5 py-4">
        {range(cards).map((index) => (
          <div
            key={index}
            className="flex flex-col gap-3.5 rounded-card border border-line bg-main-900 px-4 py-4"
          >
            <Skeleton rounded="sm" className="h-3 w-36" />
            <div className="grid grid-cols-2 gap-3">
              <Skeleton rounded="lg" tone="subtle" className="h-9" />
              <Skeleton rounded="lg" tone="subtle" className="h-9" />
            </div>
            <Skeleton rounded="lg" tone="subtle" className={index === 0 ? "h-24" : "h-14"} />
            <SkeletonLines count={2} />
          </div>
        ))}
      </div>
    </LoadingStatus>
  );
}

export function FilterRowsSkeleton({
  rows = 5,
  label = PAGE_LOADING,
}: {
  readonly rows?: number;
  readonly label?: string;
}) {
  return (
    <LoadingStatus label={label} className="flex flex-col gap-0.5">
      {range(rows).map((index) => (
        <div key={index} className="flex h-8 items-center gap-2 px-2">
          <Skeleton rounded="full" className="size-2 flex-none" />
          <Skeleton rounded="sm" className={`h-2.5 ${lineWidth(index + 3)} max-w-28`} />
          <span className="flex-1" />
          <Skeleton rounded="full" className="h-4 w-6 flex-none" />
        </div>
      ))}
    </LoadingStatus>
  );
}

export function RailSkeleton({
  rows = 6,
  label = PAGE_LOADING,
}: {
  readonly rows?: number;
  readonly label?: string;
}) {
  return (
    <LoadingStatus label={label} className="flex flex-col gap-3.5 px-3.5 py-4">
      <Skeleton rounded="sm" className="h-3.5 w-[62%]" />
      <SkeletonLines count={2} />
      {range(rows).map((index) => (
        <div key={index} className="flex items-center gap-2">
          <Skeleton rounded="sm" className={`h-2.5 ${lineWidth(index + 2)}`} />
          <span className="flex-1" />
          <Skeleton rounded="full" className="h-4 w-6 flex-none" />
        </div>
      ))}
    </LoadingStatus>
  );
}
