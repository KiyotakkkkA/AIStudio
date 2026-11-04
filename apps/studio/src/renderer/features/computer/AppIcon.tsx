import { useEffect, useMemo } from "react";
import { mdiApplicationOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";

export function iconBlob(icon: string): Blob | null {
  try {
    const bytes = Uint8Array.from(atob(icon), (char) => char.charCodeAt(0));
    return new Blob([bytes], { type: "image/png" });
  } catch {
    return null;
  }
}

export default function AppIcon({
  icon,
  size = 28,
}: {
  readonly icon: string | null | undefined;
  readonly size?: number;
}) {
  const url = useMemo(() => {
    if (icon === null || icon === undefined || icon.length === 0) return null;
    const blob = iconBlob(icon);
    return blob === null ? null : URL.createObjectURL(blob);
  }, [icon]);
  useEffect(
    () => () => {
      if (url !== null) URL.revokeObjectURL(url);
    },
    [url],
  );
  if (url === null)
    return (
      <span
        aria-hidden="true"
        className="flex shrink-0 items-center justify-center rounded-control bg-main-800 text-main-400"
        style={{ width: size, height: size }}
      >
        <Icon path={mdiApplicationOutline} size={Math.round(size * 0.6)} />
      </span>
    );
  return (
    <img
      src={url}
      alt=""
      aria-hidden="true"
      className="shrink-0 rounded-[6px]"
      style={{ width: size, height: size }}
    />
  );
}
