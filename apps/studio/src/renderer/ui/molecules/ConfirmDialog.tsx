import { Modal } from "@kiyotakkkka/zvs-uikit-lib";
import type { ReactNode } from "react";
import Button, { type ButtonTone } from "../atoms/Button";

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
      className="w-[420px] max-w-[92vw] rounded-[10px] border border-main-700 bg-main-900 p-[18px]"
    >
      <h2 className="text-[14px] font-semibold text-main-50">{title}</h2>
      <div className="mt-[10px] text-[12px] leading-[1.6] text-main-400">{children}</div>
      <div className="mt-[18px] flex justify-end gap-[8px]">
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
