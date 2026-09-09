export type StatusTone = "ok" | "warn" | "err" | "idle" | "accent";

const TONES: Record<StatusTone, string> = {
  ok: "bg-ok",
  warn: "bg-warn",
  err: "bg-err",
  idle: "bg-main-500",
  accent: "bg-accent-dark",
};

export interface StatusDotProps {
  readonly tone: StatusTone;
  readonly title?: string;
}

export default function StatusDot({ tone, title }: StatusDotProps) {
  return (
    <span className={`size-[7px] flex-none rounded-full ${TONES[tone]}`} title={title}>
      {title === undefined ? null : <span className="sr-only">{title}</span>}
    </span>
  );
}
