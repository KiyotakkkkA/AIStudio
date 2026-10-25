import { useEffect, useState } from "react";
import GenerationTrace from "../atoms/GenerationTrace";

export interface GenerationStatusProps {
  readonly label: string;
  readonly since?: number | null;
}

export default function GenerationStatus({ label, since = null }: GenerationStatusProps) {
  const [mountedAt] = useState(() => Date.now());
  const startedAt = since ?? mountedAt;
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const intervalId = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(intervalId);
  }, []);

  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000));

  return (
    <p role="status" className="flex h-6 items-center gap-2 text-[12.5px] font-medium">
      <GenerationTrace size={16} className="text-accent-dark" />
      <span className="zvs-shimmer-text">{label}…</span>
      {seconds > 0 ? (
        <span aria-hidden="true" className="font-mono text-[11px] text-main-500 tabular-nums">
          {seconds} с
        </span>
      ) : null}
    </p>
  );
}
