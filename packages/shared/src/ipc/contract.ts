import { z } from "zod";
import { Timestamp } from "../primitives/time.js";
import { defineContract } from "./defineContract.js";

export const contract = defineContract({
  "system.ping": {
    input: z.object({ sentAt: Timestamp }),
    output: z.object({
      pong: z.literal(true),
      hostTime: Timestamp,
      roundTripHint: z.number(),
    }),
  },
});

export type Contract = typeof contract;
