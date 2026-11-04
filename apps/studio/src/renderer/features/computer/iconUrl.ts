import { useEffect, useState } from "react";

function iconBlob(icon: string): Blob | null {
  try {
    const bytes = Uint8Array.from(atob(icon), (char) => char.charCodeAt(0));
    return new Blob([bytes], { type: "image/png" });
  } catch {
    return null;
  }
}

export function useIconUrl(icon: string | null | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    const blob = icon === null || icon === undefined || icon.length === 0 ? null : iconBlob(icon);
    if (blob === null) {
      setUrl(null);
      return;
    }
    const next = URL.createObjectURL(blob);
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [icon]);
  return url;
}
