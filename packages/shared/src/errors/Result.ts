import { z } from "zod";
import { AppErrorCode } from "./AppErrorCode.js";

/** Failure details stay JSON-only so they survive structured clone across IPC. */
export const ErrorDetails = z.record(z.string(), z.json());
export type ErrorDetails = z.infer<typeof ErrorDetails>;
const Failure = z.object({
  ok: z.literal(false),
  error: z.object({
    code: z.enum(AppErrorCode),
    message: z.string(),
    details: ErrorDetails.optional(),
  }),
});

export function Result<S extends z.ZodType>(data: S) {
  return z.discriminatedUnion("ok", [z.object({ ok: z.literal(true), data }), Failure]);
}
export type Result<T> = z.infer<ReturnType<typeof Result<z.ZodType<T>>>>;

export function ok<T>(data: T): Extract<Result<T>, { ok: true }> {
  return { ok: true, data };
}

export function err(
  code: AppErrorCode,
  message: string,
  details?: ErrorDetails,
): z.infer<typeof Failure> {
  return Failure.parse({
    ok: false,
    error: { code, message, ...(details === undefined ? {} : { details }) },
  });
}

export function isOk<T>(result: Result<T>): result is Extract<Result<T>, { ok: true }> {
  return result.ok;
}
