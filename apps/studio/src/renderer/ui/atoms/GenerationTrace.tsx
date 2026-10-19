const WAVE_PATH = "M240 584C312 312 432 312 512 512S712 712 784 440";

export interface GenerationTraceProps {
  readonly size?: number;
  readonly className?: string;
}

export default function GenerationTrace({ size = 16, className = "" }: GenerationTraceProps) {
  return (
    <svg
      viewBox="192 192 640 640"
      width={size}
      height={size}
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`flex-none ${className}`}
    >
      <path d={WAVE_PATH} strokeWidth={64} opacity={0.22} />
      <path d={WAVE_PATH} strokeWidth={76} pathLength={100} className="zvs-trace" />
    </svg>
  );
}
