import { z } from "zod";
import { StreamId } from "../primitives/branded.js";
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
  "system.demoStream": {
    input: z.object({ steps: z.number().int().min(1).max(100) }),
    output: z.object({ streamId: StreamId }),
  },
});

export type Contract = typeof contract;
