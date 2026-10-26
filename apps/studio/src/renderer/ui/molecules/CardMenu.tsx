import { useState, type ReactNode } from "react";
import { Dropdown } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiContentCopy, mdiDeleteOutline, mdiDotsHorizontal, mdiOpenInApp } from "@mdi/js";
import IconDropdownButton from "../atoms/buttons/IconDropdownButton";
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from "./ActionMenu";
import ConfirmDialog from "./ConfirmDialog";

export const OPEN_ITEM = "Открыть";
export const DUPLICATE_ITEM = "Дублировать";

export interface CardMenuProps {
  readonly label: string;
  readonly removeLabel: string;
  readonly confirmTitle: string;
  readonly confirmContent: ReactNode;
  readonly removeDisabled?: boolean;
  readonly removeHint?: string;
  readonly onOpen: () => void;
  readonly onDuplicate?: () => void;
  readonly duplicateDisabled?: boolean;
  readonly onRemove: () => void;
}

export default function CardMenu({
  label,
  removeLabel,
  confirmTitle,
  confirmContent,
  removeDisabled = false,
  removeHint,
  onOpen,
  onDuplicate,
  duplicateDisabled = false,
  onRemove,
}: CardMenuProps) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div
      className="flex flex-none"
      onClick={(event) => {
        event.stopPropagation();
      }}
    >
      <Dropdown menuWidth={200} menuPlacement="bottom-right">
        <IconDropdownButton label={label} tooltip="top-right" path={mdiDotsHorizontal} />
        <ActionMenu label={label}>
          <ActionMenuItem icon={mdiOpenInApp} onClick={onOpen}>
            {OPEN_ITEM}
          </ActionMenuItem>
          {onDuplicate === undefined ? null : (
            <ActionMenuItem
              icon={mdiContentCopy}
              disabled={duplicateDisabled}
              onClick={onDuplicate}
            >
              {DUPLICATE_ITEM}
            </ActionMenuItem>
          )}
          <ActionMenuSeparator />
          <ActionMenuItem
            icon={mdiDeleteOutline}
            danger
            disabled={removeDisabled}
            {...(removeHint === undefined ? {} : { title: removeHint })}
            onClick={() => setConfirming(true)}
          >
            {removeLabel}
          </ActionMenuItem>
        </ActionMenu>
      </Dropdown>
      <ConfirmDialog
        open={confirming}
        title={confirmTitle}
        confirmLabel="Удалить"
        confirmTone="danger"
        onCancel={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          onRemove();
        }}
      >
        {confirmContent}
      </ConfirmDialog>
    </div>
  );
}
