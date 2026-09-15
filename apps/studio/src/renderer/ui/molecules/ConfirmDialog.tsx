import { Modal } from "@kiyotakkkka/zvs-uikit-lib";
import type { ReactNode } from "react";
import Button, { type ButtonTone } from "../atoms/buttons/Button";

export interface ConfirmDialogProps {
  readonly open: boolean;
  readonly title: string;
  readonly children: ReactNode;
  readonly confirmLabel?: string;
  readonly confirmTone?: ButtonTone;
  readonly cancelLabel?: string;
  readonly onConfirm?: () => void;
  readonly onCancel: () => void;
}

export default function ConfirmDialog({
  open,
  title,
  children,
  confirmLabel,
  confirmTone = "primary",
  cancelLabel = "Отмена",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <Modal
      open={open}
      onClose={onCancel}
      label={title}
      className="w-105 max-w-[92vw] rounded-card border border-main-750 bg-main-900 p-4.5"
    >
      <h2 className="text-[14px] font-semibold text-main-50">{title}</h2>
      <div className="mt-2.5 text-[12px] leading-[1.6] text-main-400">{children}</div>
      <div className="mt-4.5 flex justify-end gap-2">
        <Button type="button" tone="secondary" onClick={onCancel}>
          {cancelLabel}
        </Button>
        {onConfirm === undefined || confirmLabel === undefined ? null : (
          <Button type="button" tone={confirmTone} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        )}
      </div>
    </Modal>
  );
}
