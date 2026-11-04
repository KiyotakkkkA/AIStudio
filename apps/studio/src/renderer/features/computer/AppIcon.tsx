import { useState } from "react";
import { mdiApplicationOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { useIconUrl } from "./iconUrl";

export default function AppIcon({
  icon,
  size = 28,
}: {
  readonly icon: string | null | undefined;
  readonly size?: number;
}) {
  const url = useIconUrl(icon);
  const [failed, setFailed] = useState<string | null>(null);
  if (url === null || failed === url)
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
      onError={() => setFailed(url)}
    />
  );
}
