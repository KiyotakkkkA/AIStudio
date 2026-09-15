export interface RateSample {
  readonly value: number;
  readonly at: number;
}

export interface RateState {
  readonly perSecond: number;
  readonly sample: RateSample;
}

export const RATE_WINDOW_MS = 3_000;

/**
 * Exponentially smoothed rate of progress, in whatever unit the counter is in — bytes for a
 * transfer, chunks for an index. A raw instant rate swings wildly when a report lands early or
 * late, and an ETA built on it is unreadable.
 */
export function smoothRate(
  previous: RateState | undefined,
  sample: RateSample,
  windowMs: number = RATE_WINDOW_MS,
): RateState {
  if (previous === undefined) return { perSecond: 0, sample };
  const elapsed = sample.at - previous.sample.at;
  if (elapsed <= 0) return previous;
  const moved = sample.value - previous.sample.value;
  if (moved < 0) return { perSecond: 0, sample };
  const instant = (moved * 1000) / elapsed;
  const weight = 1 - Math.exp(-elapsed / windowMs);
  return {
    perSecond: previous.perSecond + (instant - previous.perSecond) * weight,
    sample,
  };
}

export function estimateEta(done: number, total: number, perSecond: number): number | undefined {
  const remaining = total - done;
  if (remaining <= 0 || perSecond <= 0) return undefined;
  return Math.round((remaining / perSecond) * 1000);
}
