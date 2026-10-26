import { Skeleton as KitSkeleton, type SkeletonProps } from "@kiyotakkkka/zvs-uikit-lib";
import { useFx, type FxLevels } from "../motion/fx";

export function skeletonAnimated({ motion, effects }: FxLevels): boolean {
  return motion !== "none" && effects !== "none";
}

export default function Skeleton({ animated, ...props }: SkeletonProps) {
  const fx = useFx();
  return <KitSkeleton animated={(animated ?? true) && skeletonAnimated(fx)} {...props} />;
}
