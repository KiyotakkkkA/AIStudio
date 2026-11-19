import { useState } from "react";

interface ProfileAvatarProps {
  readonly initials: string;
  readonly url: string | null;
  readonly className: string;
}

export default function ProfileAvatar({ initials, url, className }: ProfileAvatarProps) {
  const [failedUrl, setFailedUrl] = useState<string | null>(null);

  return (
    <span
      aria-hidden="true"
      className={`flex flex-none items-center justify-center overflow-hidden rounded-full bg-accent-soft font-semibold text-accent-medium ${className}`}
    >
      {url !== null && url !== failedUrl ? (
        <img src={url} alt="" className="size-full" onError={() => setFailedUrl(url)} />
      ) : (
        initials
      )}
    </span>
  );
}
