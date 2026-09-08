import { z } from "zod";
import { AppErrorCode } from "./AppErrorCode.js";

const ErrorDetails = z.record(z.string(), z.json());
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

/** Messages shown to users are Russian; stacks remain in host logs. */
export function err(
  code: AppErrorCode,
  message: string,
  details?: z.infer<typeof ErrorDetails>,
): z.infer<typeof Failure> {
  return Failure.parse({
    ok: false,
    error: { code, message, ...(details === undefined ? {} : { details }) },
  });
}

/** Narrow a validated result; this does not validate untrusted input. */
export function isOk<T>(result: Result<T>): result is Extract<Result<T>, { ok: true }> {
  return result.ok;
}
