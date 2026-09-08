import { z } from "zod";

export const SortDirection = z.enum(["asc", "desc"]);
export type SortDirection = z.infer<typeof SortDirection>;

/** Supply a feature's enum of permitted sort fields. */
export function Sort<S extends z.ZodType<string>>(field: S) {
  return z.object({ field, direction: SortDirection });
}
export type Sort<T extends string> = z.infer<ReturnType<typeof Sort<z.ZodType<T>>>>;
