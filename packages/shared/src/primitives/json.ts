import { z } from "zod";

export const Json = z.json();
export type Json = z.infer<typeof Json>;
