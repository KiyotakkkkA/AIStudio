import { z } from "zod";

export const Timestamp = z.number().int().nonnegative();
export type Timestamp = z.infer<typeof Timestamp>;

/** Host services should pass their injected millisecond clock. */
export function timestampNow(clock: () => number = Date.now): Timestamp {
  return Timestamp.parse(clock());
}
