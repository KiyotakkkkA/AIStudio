import { AppErrorCode, isAppError, type ErrorDetails } from "@zvs/shared";
import type { DiscoveredModel } from "../drivers/ai/ports.ts";

export type ProbeOutcome =
  | {
      readonly kind: "ok";
      readonly latencyMs: number;
      readonly models: readonly DiscoveredModel[];
      readonly live: boolean;
    }
  | { readonly kind: "ok-empty"; readonly latencyMs: number }
  | { readonly kind: "auth-failed" }
  | { readonly kind: "account-not-linked" }
  | { readonly kind: "session-expired" }
  | { readonly kind: "unreachable"; readonly detail: string }
  | { readonly kind: "rate-limited"; readonly retryAfter?: number }
  | { readonly kind: "error"; readonly code: AppErrorCode; readonly detail: string };

export type ProbeOutcomeKind = ProbeOutcome["kind"];

export function succeeded(outcome: ProbeOutcome): boolean {
  return outcome.kind === "ok" || outcome.kind === "ok-empty";
}

export function discoveryOutcome(
  latencyMs: number,
  models: readonly DiscoveredModel[],
  live: boolean,
): ProbeOutcome {
  if (models.length === 0) return { kind: "ok-empty", latencyMs };
  return { kind: "ok", latencyMs, models, live };
}

export function failureOutcome(error: unknown): ProbeOutcome {
  if (!isAppError(error)) {
    return { kind: "error", code: AppErrorCode.UNKNOWN, detail: "Не удалось выполнить проверку" };
  }
  switch (error.code) {
    case AppErrorCode.PROVIDER_AUTH_FAILED:
      return { kind: "auth-failed" };
    case AppErrorCode.PROVIDER_SESSION_EXPIRED:
      return notLinked(error.details)
        ? { kind: "account-not-linked" }
        : { kind: "session-expired" };
    case AppErrorCode.PROVIDER_UNREACHABLE:
      return { kind: "unreachable", detail: "Провайдер недоступен или не ответил вовремя" };
    case AppErrorCode.RATE_LIMITED: {
      const retryAfter = retryAfterOf(error.details);
      return retryAfter === undefined
        ? { kind: "rate-limited" }
        : { kind: "rate-limited", retryAfter };
    }
    default:
      return { kind: "error", code: error.code, detail: "Не удалось выполнить проверку" };
  }
}

function notLinked(details: ErrorDetails | undefined): boolean {
  return details?.["reason"] === "account-not-linked";
}

function retryAfterOf(details: ErrorDetails | undefined): number | undefined {
  const value = details?.["retryAfter"];
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? Math.round(value)
    : undefined;
}
