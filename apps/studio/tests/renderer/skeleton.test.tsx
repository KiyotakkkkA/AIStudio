import { expect, test } from "vitest";
import { skeletonAnimated } from "../../src/renderer/ui/atoms/Skeleton.tsx";
import { awaitingData, lineWidth } from "../../src/renderer/ui/molecules/LoadingSkeleton.tsx";
import { FX_LEVELS } from "../../src/renderer/ui/motion/levels.ts";

test("skeletons shimmer only while both motion and effects are on", () => {
  for (const motion of FX_LEVELS)
    for (const effects of FX_LEVELS)
      expect(skeletonAnimated({ motion, effects })).toBe(motion !== "none" && effects !== "none");
});

test("skeleton lines vary their widths and repeat after the set", () => {
  expect(lineWidth(0)).not.toBe(lineWidth(1));
  expect(lineWidth(6)).toBe(lineWidth(0));
});

test("a page waits for data until its first load settles, and never through an error", () => {
  expect(awaitingData({ loaded: false, error: null })).toBe(true);
  expect(awaitingData({ loaded: true, error: null })).toBe(false);
  expect(awaitingData({ loaded: false, error: "Сбой" })).toBe(false);
});
