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
      className="w-110 max-w-[92vw] overflow-hidden rounded-card border bg-main-900 p-0 shadow-pop border-main-750"
    >
      <div className="px-5 pt-5">
        <h2 className="text-[15px] font-semibold tracking-[-0.01em] text-main-50">{title}</h2>
        <div className="mt-2 text-[13px] leading-relaxed text-main-400">{children}</div>
      </div>
      <div className="mt-5 flex justify-end gap-2 border-t border-main-750 bg-main-850 px-5 py-3">
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
