import { useId } from "react";

const WAVE_PATH = "M240 584C312 312 432 312 512 512S712 712 784 440";

export interface AssistantAvatarProps {
  readonly size?: number;
  readonly live?: boolean;
  readonly className?: string;
}

export default function AssistantAvatar({
  size = 28,
  live = false,
  className = "",
}: AssistantAvatarProps) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  return (
    <svg
      viewBox="64 64 896 896"
      width={size}
      height={size}
      aria-hidden="true"
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`flex-none ${className}`}
    >
      <defs>
        <radialGradient id={`${id}orb`} cx="0.32" cy="0.26" r="0.86">
          <stop offset="0" stopColor="#ffbf86" />
          <stop offset="0.45" stopColor="#e2702c" />
          <stop offset="1" stopColor="#7c300b" />
        </radialGradient>
        <radialGradient id={`${id}ember`} cx="0.5" cy="0.5" r="0.5">
          <stop offset="0" stopColor="#fff6e8" />
          <stop offset="0.35" stopColor="#ffd09a" stopOpacity="0.8" />
          <stop offset="1" stopColor="#ff9a4a" stopOpacity="0" />
        </radialGradient>
      </defs>
      <circle cx="512" cy="512" r="448" fill={`url(#${id}orb)`} />
      <path d={WAVE_PATH} stroke="#fffaf3" strokeOpacity={0.32} strokeWidth={72} />
      {live ? (
        <path
          d={WAVE_PATH}
          pathLength={100}
          stroke="#fffaf3"
          strokeWidth={80}
          className="zvs-trace"
        />
      ) : (
        <>
          <path d="M240 584C312 312 432 312 512 512" stroke="#fffaf3" strokeWidth={72} />
          <circle cx="512" cy="512" r="120" fill={`url(#${id}ember)`} />
          <circle cx="512" cy="512" r="30" fill="#fffaf3" />
        </>
      )}
    </svg>
  );
}
