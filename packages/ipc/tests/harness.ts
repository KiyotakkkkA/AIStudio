import { z } from "zod";
import { defineContract } from "@zvs/shared";

export const testContract = defineContract({
  "system.ping": {
    input: z.object({ sentAt: z.number().int().nonnegative() }),
    output: z.object({ pong: z.literal(true), hostTime: z.number().int().nonnegative() }),
  },
  "system.echo": {
    input: z.object({ text: z.string() }),
    output: z.object({ text: z.string() }),
  },
});
