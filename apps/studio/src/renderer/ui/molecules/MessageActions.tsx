import { useEffect, useState } from "react";
import { mdiCheck, mdiContentCopy, mdiDeleteOutline, mdiPencilOutline, mdiRefresh } from "@mdi/js";
import IconButton from "../atoms/IconButton";

export default function MessageActions({
  content,
  isUser,
  onEdit,
  onRefresh,
}: {
  readonly content: string;
  readonly isUser?: boolean;
  readonly onEdit?: () => void;
  readonly onRefresh?: () => void;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return;
    }

    const timeoutId = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timeoutId);
  }, [copied]);

  const handleCopy = async () => {
    try {
      await navigator.clipboard?.writeText(content);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="flex items-center gap-0.5 text-main-400">
      <IconButton
        aria-label={copied ? "Сообщение скопировано" : "Копировать сообщение"}
        title={copied ? "Скопировано" : "Копировать"}
        path={copied ? mdiCheck : mdiContentCopy}
        onClick={() => void handleCopy()}
      />
      {isUser && (
        <>
          <IconButton
            aria-label="Повторить сообщение"
            title="Повторить"
            path={mdiRefresh}
            iconSize={16}
            onClick={onRefresh}
          />{" "}
          <IconButton
            aria-label="Редактировать сообщение"
            title="Редактировать"
            path={mdiPencilOutline}
            onClick={onEdit}
          />
          <IconButton
            aria-label="Удалить сообщение"
            title="Удалить"
            path={mdiDeleteOutline}
            className="hover:text-err"
          />
        </>
      )}
    </div>
  );
}
