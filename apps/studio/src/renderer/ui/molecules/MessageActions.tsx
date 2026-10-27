import { useEffect, useState } from "react";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import {
  mdiCheck,
  mdiContentCopy,
  mdiDeleteOutline,
  mdiPencilOutline,
  mdiPin,
  mdiPinOutline,
  mdiRefresh,
  mdiSourceBranch,
  mdiSourceBranchPlus,
} from "@mdi/js";
import IconButton from "../atoms/buttons/IconButton";
import IconDropdownButton from "../atoms/buttons/IconDropdownButton";
import { ActionMenu, ActionMenuItem } from "./ActionMenu";

export interface MessageBranchAction {
  readonly label: string;
  readonly newDialogLabel: string;
  readonly disabled?: boolean;
  readonly onNewDialog: () => void;
}

export default function MessageActions({
  content,
  isUser,
  onEdit,
  onRefresh,
  onDelete,
  pinned = false,
  onPin,
  branch,
}: {
  readonly content: string;
  readonly isUser?: boolean;
  readonly onEdit?: () => void;
  readonly onRefresh?: () => void;
  readonly onDelete?: () => void;
  readonly pinned?: boolean;
  readonly onPin?: () => void;
  readonly branch?: MessageBranchAction;
}) {
  const [copied, setCopied] = useState(false);
  const tooltip = isUser ? "bottom-left" : "top-right";

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
        label={copied ? "Скопировано" : "Копировать"}
        tooltip={tooltip}
        path={copied ? mdiCheck : mdiContentCopy}
        onClick={() => void handleCopy()}
      />
      {isUser && onRefresh && (
        <>
          <IconButton
            aria-label="Повторить сообщение"
            label="Повторить"
            tooltip={tooltip}
            path={mdiRefresh}
            iconSize={16}
            onClick={onRefresh}
          />{" "}
        </>
      )}
      {onPin && (
        <IconButton
          aria-label={pinned ? "Открепить сообщение" : "Закрепить сообщение"}
          aria-pressed={pinned}
          label={pinned ? "Открепить" : "Закрепить"}
          tooltip={tooltip}
          path={pinned ? mdiPin : mdiPinOutline}
          {...(pinned
            ? { toneClassName: "text-accent-medium hover:bg-main-700 hover:text-accent-light" }
            : {})}
          onClick={onPin}
        />
      )}
      {branch && (
        <Dropdown menuWidth={260} menuPlacement="bottom-left" disabled={branch.disabled}>
          <IconDropdownButton
            label={branch.label}
            tooltip={tooltip}
            path={mdiSourceBranch}
            iconSize={15}
            size="sm"
            rounded=""
            className="rounded-control"
            disabled={branch.disabled}
            toneClassName="text-main-400 hover:bg-main-700 hover:text-main-50"
          />
          <ActionMenu label={branch.label}>
            <ActionMenuItem icon={mdiSourceBranchPlus} onClick={branch.onNewDialog}>
              {branch.newDialogLabel}
            </ActionMenuItem>
          </ActionMenu>
        </Dropdown>
      )}
      {onEdit && (
        <IconButton
          aria-label="Редактировать сообщение"
          label="Редактировать"
          tooltip={tooltip}
          path={mdiPencilOutline}
          onClick={onEdit}
        />
      )}
      {onDelete && (
        <IconButton
          aria-label="Удалить сообщение"
          label="Удалить"
          tooltip={tooltip}
          path={mdiDeleteOutline}
          className="hover:text-err"
          onClick={onDelete}
        />
      )}
    </div>
  );
}
