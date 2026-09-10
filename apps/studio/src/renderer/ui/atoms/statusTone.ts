export type StatusTone = "ok" | "warn" | "err" | "idle" | "accent";

export const STATUS_TONE_BACKGROUNDS: Record<StatusTone, string> = {
  ok: "bg-ok",
  warn: "bg-warn",
  err: "bg-err",
  idle: "bg-main-500",
  accent: "bg-accent-dark",
};

export const STATUS_TONE_TEXT: Record<StatusTone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  err: "text-err",
  idle: "text-main-400",
  accent: "text-accent-medium",
};
