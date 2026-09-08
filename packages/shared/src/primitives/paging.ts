import { z } from "zod";

export const PageRequest = z.object({
  limit: z.number().int().positive(),
  cursor: z.string().min(1).optional(),
});
export type PageRequest = z.infer<typeof PageRequest>;

export function Page<S extends z.ZodType>(item: S) {
  return z.object({
    items: z.array(item),
    nextCursor: z.string().min(1).optional(),
  });
}
export type Page<T> = z.infer<ReturnType<typeof Page<z.ZodType<T>>>>;
