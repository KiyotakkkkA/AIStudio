import { STATUS_TONE_BACKGROUNDS, type StatusTone } from "./statusTone";

export type { StatusTone };

export interface StatusDotProps {
  readonly tone: StatusTone;
  readonly title?: string;
}

export default function StatusDot({ tone, title }: StatusDotProps) {
  return (
    <span
      className={`size-1.75 flex-none rounded-full ${STATUS_TONE_BACKGROUNDS[tone]}`}
      title={title}
    >
      {title === undefined ? null : <span className="sr-only">{title}</span>}
    </span>
  );
}
