import { z } from "zod";
import { StreamId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { Json } from "../primitives/json.js";
import { defineContract } from "./defineContract.js";

export const SettingKey = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[a-z][a-z0-9]*(\.[a-z0-9]+)*$/);
export type SettingKey = z.infer<typeof SettingKey>;

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
  "settings.get": {
    input: z.object({ key: SettingKey }),
    output: z.object({
      key: SettingKey,
      value: Json.optional(),
      updatedAt: Timestamp.optional(),
    }),
  },
  "settings.set": {
    input: z.object({ key: SettingKey, value: Json }),
    output: z.object({ key: SettingKey, value: Json, updatedAt: Timestamp }),
  },
});

export type Contract = typeof contract;
