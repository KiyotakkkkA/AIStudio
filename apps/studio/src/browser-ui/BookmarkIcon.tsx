import { useEffect, useState } from "react";
import { loadBookmarkIcon } from "./loadBookmarkIcon";

export default function BookmarkIcon({
  id,
  label,
}: {
  readonly id: string;
  readonly label: string;
}) {
  const [source, setSource] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    let url: string | null = null;
    setSource(null);
    void loadBookmarkIcon(id, window.zvs.call).then((blob) => {
      if (disposed || blob === null) return;
      url = URL.createObjectURL(blob);
      setSource(url);
    });
    return () => {
      disposed = true;
      if (url !== null) URL.revokeObjectURL(url);
    };
  }, [id]);

  return (
    <span className="bookmark-icon" aria-hidden="true">
      {source === null ? (
        label.charAt(0).toLocaleUpperCase("ru-RU")
      ) : (
        <img src={source} alt="" width={24} height={24} onError={() => setSource(null)} />
      )}
    </span>
  );
}
