import { z } from "zod";
import { DeltaKind } from "../ai.js";
import { AppErrorCode } from "../errors/AppErrorCode.js";
import { StreamId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const EVENT_CHANNEL = "zvs:events";

export const EventSeq = z.number().int().nonnegative();
export type EventSeq = z.infer<typeof EventSeq>;

const LoosePayload = z.record(z.string(), z.json());

export const StepEventDto = LoosePayload;
export type StepEventDto = z.infer<typeof StepEventDto>;

export const LogLineDto = LoosePayload;
export type LogLineDto = z.infer<typeof LogLineDto>;

export const ApprovalRequestDto = LoosePayload;
export type ApprovalRequestDto = z.infer<typeof ApprovalRequestDto>;

export const RunOutcomeDto = z.object({
  status: z.enum(["ok", "cancelled", "failed"]),
  code: z.enum(AppErrorCode).optional(),
  message: z.string().optional(),
});
export type RunOutcomeDto = z.infer<typeof RunOutcomeDto>;

const envelope = { streamId: StreamId, seq: EventSeq, ts: Timestamp };

export const HostEvent = z.discriminatedUnion("type", [
  z.object({
    ...envelope,
    type: z.literal("token"),
    delta: z.string(),
    kind: DeltaKind.optional(),
  }),
  z.object({
    ...envelope,
    type: z.literal("progress"),
    done: z.number().int().nonnegative(),
    total: z.number().int().nonnegative(),
  }),
  z.object({ ...envelope, type: z.literal("step"), step: StepEventDto }),
  z.object({ ...envelope, type: z.literal("log"), line: LogLineDto }),
  z.object({ ...envelope, type: z.literal("approval"), request: ApprovalRequestDto }),
  z.object({ ...envelope, type: z.literal("end"), outcome: RunOutcomeDto }),
]);
export type HostEvent = z.infer<typeof HostEvent>;

export type HostEventType = HostEvent["type"];
export type HostEventOf<T extends HostEventType> = Extract<HostEvent, { type: T }>;

type WithoutEnvelope<T> = T extends unknown ? Omit<T, "streamId" | "seq" | "ts"> : never;
export type HostEventDraft = WithoutEnvelope<HostEvent>;
