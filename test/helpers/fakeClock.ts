export const CLOCK_ORIGIN = 1_700_000_000_000;

export interface FakeClock {
  /** Callable so it drops straight into a `clock: () => number` service dependency. */
  (): number;
  now(): number;
  /** Moves the clock forward and returns the new time. */
  advance(ms: number): number;
  /** Jumps the clock to an absolute time and returns it. */
  set(ms: number): number;
}

export function createFakeClock(start: number = CLOCK_ORIGIN): FakeClock {
  let current = start;
  const clock = (): number => current;
  return Object.assign(clock, {
    now: (): number => current,
    advance(ms: number): number {
      current += ms;
      return current;
    },
    set(ms: number): number {
      current = ms;
      return current;
    },
  });
}
