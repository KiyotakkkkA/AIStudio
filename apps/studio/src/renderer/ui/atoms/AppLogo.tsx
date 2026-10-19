import { useId } from "react";

export interface AppLogoProps {
  readonly size?: number;
  readonly className?: string;
}

export default function AppLogo({ size = 32, className = "" }: AppLogoProps) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  return (
    <svg
      viewBox="64 64 896 896"
      width={size}
      height={size}
      aria-hidden="true"
      className={`flex-none ${className}`}
    >
      <defs>
        <linearGradient id={`${id}bg`} x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stopColor="#f59a55" />
          <stop offset="0.5" stopColor="#d4621f" />
          <stop offset="1" stopColor="#8a360e" />
        </linearGradient>
        <radialGradient id={`${id}sheen`} cx="0.25" cy="0.12" r="0.8">
          <stop offset="0" stopColor="#ffe2c2" stopOpacity="0.6" />
          <stop offset="0.55" stopColor="#ffe2c2" stopOpacity="0" />
        </radialGradient>
        <radialGradient id={`${id}ember`} cx="0.5" cy="0.5" r="0.5">
          <stop offset="0" stopColor="#fff6e8" />
          <stop offset="0.3" stopColor="#ffd09a" stopOpacity="0.85" />
          <stop offset="1" stopColor="#ff9a4a" stopOpacity="0" />
        </radialGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="216" fill={`url(#${id}bg)`} />
      <rect x="64" y="64" width="896" height="896" rx="216" fill={`url(#${id}sheen)`} />
      <path
        d="M240 584C312 312 432 312 512 512S712 712 784 440"
        fill="none"
        stroke="#fffaf3"
        strokeWidth={88}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="784" cy="440" r="150" fill={`url(#${id}ember)`} />
      <circle cx="784" cy="440" r="32" fill="#fffaf3" />
    </svg>
  );
}
