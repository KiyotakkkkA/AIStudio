import type { Timestamp } from "@zvs/shared";

export const HINT_TAIL_LENGTH = 4;
export const HINT_ELLIPSIS = "…";
export const DUE_SOON_WINDOW_MS = 14 * 24 * 60 * 60 * 1000;

export const KNOWN_VALUE_PREFIXES = [
  "sk-ant-api03-",
  "sk-or-v1-",
  "osk_live_",
  "osk_test_",
  "xoxb-",
  "ghp_",
  "sk-",
] as const;

export const ROTATION_STATUSES = ["ok", "due-soon", "overdue"] as const;
export type RotationStatus = (typeof ROTATION_STATUSES)[number];

export function computeHint(value: string): string {
  const tail = value.slice(-HINT_TAIL_LENGTH);
  const prefix = KNOWN_VALUE_PREFIXES.find(
    (candidate) =>
      value.startsWith(candidate) && value.length >= candidate.length + HINT_TAIL_LENGTH,
  );
  return `${prefix ?? ""}${HINT_ELLIPSIS}${tail}`;
}

export function rotationStatus(
  rotatesAt: Timestamp | null | undefined,
  now: Timestamp,
  window: number = DUE_SOON_WINDOW_MS,
): RotationStatus {
  if (rotatesAt === null || rotatesAt === undefined) return "ok";
  if (rotatesAt <= now) return "overdue";
  return rotatesAt - now <= window ? "due-soon" : "ok";
}

export function rotationDueAt(
  rotationDays: number | null | undefined,
  from: Timestamp,
): Timestamp | null {
  if (rotationDays === null || rotationDays === undefined) return null;
  if (!Number.isInteger(rotationDays) || rotationDays <= 0) return null;
  return from + rotationDays * 24 * 60 * 60 * 1000;
}
