import type { ProviderStatus, Timestamp } from "@zvs/shared";
import { DUE_SOON_WINDOW_MS, rotationStatus } from "./secretPolicy.ts";
import type { ProbeOutcome } from "./probeOutcome.ts";

export const DAY_MS = 24 * 60 * 60 * 1000;

export const CREDENTIAL_SOURCES = ["secret", "account"] as const;
type CredentialSource = (typeof CREDENTIAL_SOURCES)[number];

export interface CredentialLifetime {
  readonly source: CredentialSource;
  readonly expiresAt: Timestamp | null;
}

export interface StatusDerivation {
  readonly status: ProviderStatus;
  readonly detail: string | null;
}

export interface DeriveStatusInput {
  readonly outcome: ProbeOutcome;
  readonly lifetimes?: readonly CredentialLifetime[];
  readonly now: Timestamp;
  readonly window?: number;
}

const LIFETIME_COPY: Readonly<
  Record<CredentialSource, { overdue: string; dueSoon: (days: number) => string }>
> = {
  secret: {
    overdue: "Учётные данные просрочены",
    dueSoon: (days) => `Истекает через ${days} дн.`,
  },
  account: {
    overdue: "Сессия просрочена — войдите заново",
    dueSoon: (days) => `Сессия истекает через ${days} дн.`,
  },
};

const FAILURE_COPY: Readonly<Record<string, string>> = {
  "auth-failed": "Ключ отклонён — исправьте учётные данные",
  "account-not-linked": "Аккаунт не привязан",
  "session-expired": "Сессия истекла — войдите заново",
  "rate-limited": "Провайдер ограничил частоту запросов",
};

export function deriveStatus(input: DeriveStatusInput): StatusDerivation {
  const failure = probeFailure(input.outcome);
  if (failure !== null) return failure;
  const window = input.window ?? DUE_SOON_WINDOW_MS;
  for (const lifetime of input.lifetimes ?? []) {
    const warning = lifetimeWarning(lifetime, input.now, window);
    if (warning !== null) return warning;
  }
  return { status: "ok", detail: null };
}

function probeFailure(outcome: ProbeOutcome): StatusDerivation | null {
  switch (outcome.kind) {
    case "ok":
    case "ok-empty":
      return null;
    case "unreachable":
    case "error":
      return { status: "failed", detail: outcome.detail };
    default:
      return { status: "failed", detail: FAILURE_COPY[outcome.kind] ?? null };
  }
}

function lifetimeWarning(
  lifetime: CredentialLifetime,
  now: Timestamp,
  window: number,
): StatusDerivation | null {
  const copy = LIFETIME_COPY[lifetime.source];
  switch (rotationStatus(lifetime.expiresAt, now, window)) {
    case "overdue":
      return { status: "degraded", detail: copy.overdue };
    case "due-soon":
      return { status: "degraded", detail: copy.dueSoon(daysUntil(lifetime.expiresAt, now)) };
    default:
      return null;
  }
}

export function daysUntil(expiresAt: Timestamp | null, now: Timestamp): number {
  if (expiresAt === null) return 0;
  return Math.max(0, Math.ceil((expiresAt - now) / DAY_MS));
}
