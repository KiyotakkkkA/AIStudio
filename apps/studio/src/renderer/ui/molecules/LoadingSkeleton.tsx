import type { ReactNode } from "react";
import Skeleton from "../atoms/Skeleton";

const LINE_WIDTHS = ["w-[92%]", "w-[78%]", "w-[86%]", "w-[64%]", "w-[88%]", "w-[52%]"] as const;

export const PAGE_LOADING = "Загрузка…";

export function lineWidth(index: number): string {
  return LINE_WIDTHS[index % LINE_WIDTHS.length] ?? "w-full";
}

export function awaitingData(source: {
  readonly loaded: boolean;
  readonly error: string | null;
}): boolean {
  return !source.loaded && source.error === null;
}

export function LoadingStatus({
  label,
  className = "",
  children,
}: {
  readonly label: string;
  readonly className?: string;
  readonly children: ReactNode;
}) {
  return (
    <div role="status" aria-label={label} aria-busy="true" className={className}>
      {children}
      <span className="sr-only">{label}</span>
    </div>
  );
}

export function SkeletonLines({
  count = 3,
  className = "",
}: {
  readonly count?: number;
  readonly className?: string;
}) {
  return (
    <div className={`flex flex-col gap-2 ${className}`}>
      {Array.from({ length: count }, (_, index) => (
        <Skeleton key={index} rounded="sm" className={`h-2.75 ${lineWidth(index)}`} />
      ))}
    </div>
  );
}

export function DocumentSkeleton({
  label,
  paragraphs = 3,
  className = "",
}: {
  readonly label: string;
  readonly paragraphs?: number;
  readonly className?: string;
}) {
  return (
    <LoadingStatus label={label} className={`flex flex-col gap-5 ${className}`}>
      <Skeleton rounded="sm" className="h-4 w-[42%]" />
      {Array.from({ length: paragraphs }, (_, index) => (
        <SkeletonLines key={index} count={index === paragraphs - 1 ? 2 : 4} />
      ))}
    </LoadingStatus>
  );
}

export function DetailSkeleton({
  label,
  sections = 2,
  className = "",
}: {
  readonly label: string;
  readonly sections?: number;
  readonly className?: string;
}) {
  return (
    <LoadingStatus label={label} className={`flex flex-col gap-6 ${className}`}>
      <div className="flex items-center gap-3">
        <Skeleton rounded="lg" className="size-10" />
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <Skeleton rounded="sm" className="h-3.5 w-[46%]" />
          <Skeleton rounded="sm" className="h-2.5 w-[30%]" />
        </div>
      </div>
      {Array.from({ length: sections }, (_, index) => (
        <div key={index} className="flex flex-col gap-3">
          <Skeleton rounded="sm" className="h-2.5 w-24" />
          <Skeleton rounded="xl" tone="subtle" className="h-20 w-full" />
          <SkeletonLines count={2} />
        </div>
      ))}
    </LoadingStatus>
  );
}

export function InlineSkeleton({
  label,
  className = "w-16",
}: {
  readonly label: string;
  readonly className?: string;
}) {
  return (
    <span role="status" aria-label={label} className="inline-flex items-center align-middle">
      <Skeleton rounded="full" className={`inline-block h-2.5 ${className}`} />
      <span className="sr-only">{label}</span>
    </span>
  );
}
