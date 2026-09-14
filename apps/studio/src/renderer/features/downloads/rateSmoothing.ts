export interface RateSample {
  readonly bytes: number;
  readonly at: number;
}

export interface RateState {
  readonly bytesPerSecond: number;
  readonly sample: RateSample;
}

export const RATE_WINDOW_MS = 3_000;

export function smoothRate(
  previous: RateState | undefined,
  sample: RateSample,
  windowMs: number = RATE_WINDOW_MS,
): RateState {
  if (previous === undefined) return { bytesPerSecond: 0, sample };
  const elapsed = sample.at - previous.sample.at;
  if (elapsed <= 0) return previous;
  const moved = sample.bytes - previous.sample.bytes;
  if (moved < 0) return { bytesPerSecond: 0, sample };
  const instant = (moved * 1000) / elapsed;
  const weight = 1 - Math.exp(-elapsed / windowMs);
  return {
    bytesPerSecond: previous.bytesPerSecond + (instant - previous.bytesPerSecond) * weight,
    sample,
  };
}

export function estimateEta(
  bytesDone: number,
  sizeBytes: number,
  bytesPerSecond: number,
): number | undefined {
  const remaining = sizeBytes - bytesDone;
  if (remaining <= 0 || bytesPerSecond <= 0) return undefined;
  return Math.round((remaining / bytesPerSecond) * 1000);
}
