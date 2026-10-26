import Skeleton from "../ui/atoms/Skeleton";
import ListRowSkeleton from "../ui/molecules/ListRowSkeleton";
import { LoadingStatus } from "../ui/molecules/LoadingSkeleton";

export const ROUTE_LOADING = "Загрузка раздела…";

export default function RouteFallback() {
  return (
    <LoadingStatus label={ROUTE_LOADING} className="flex min-h-0 flex-1 flex-col gap-5 px-6 py-5">
      <div className="flex items-center gap-3">
        <Skeleton rounded="lg" className="size-9" />
        <div className="flex flex-col gap-2">
          <Skeleton rounded="sm" className="h-3.5 w-40" />
          <Skeleton rounded="sm" className="h-2.5 w-24" />
        </div>
      </div>
      <div className="flex flex-col gap-2.5">
        <ListRowSkeleton />
        <ListRowSkeleton />
        <ListRowSkeleton />
      </div>
    </LoadingStatus>
  );
}
