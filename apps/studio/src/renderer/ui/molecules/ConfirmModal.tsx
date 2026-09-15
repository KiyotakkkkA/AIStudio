import { Modal } from "@kiyotakkkka/zvs-uikit-lib";
import type { ReactNode } from "react";
import ButtonBase, { type ButtonTone } from "../atoms/buttons/ButtonBase";

export interface ConfirmModalSetup {
  readonly title: string;
  readonly content: ReactNode;
  readonly tone?: ButtonTone;
  readonly confirmLabel?: string;
  readonly cancelLabel?: string;
}

export interface ConfirmModalProps extends ConfirmModalSetup {
  readonly open: boolean;
  readonly disabled?: boolean;
  readonly onConfirm: () => void;
  readonly onCancel: () => void;
}

export default function ConfirmModal({
  open,
  title,
  content,
  tone = "danger",
  confirmLabel = "Подтвердить",
  cancelLabel = "Отмена",
  disabled,
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  return (
    <Modal
      open={open}
      onClose={onCancel}
      label={title}
      className={`w-105 max-w-[92vw] rounded-card border bg-main-900 p-4.5 ${tone === "danger" ? "border-err-border" : "border-main-750"}`}
    >
      <h2
        className={`text-[14px] font-semibold ${tone === "danger" ? "text-err" : "text-main-50"}`}
      >
        {title}
      </h2>
      <div className="mt-2.5 text-[12px] leading-[1.6] text-main-400">{content}</div>
      <div className="mt-4.5 flex justify-end gap-2">
        <ButtonBase type="button" onClick={onCancel}>
          {cancelLabel}
        </ButtonBase>
        <ButtonBase type="button" tone={tone} disabled={disabled} onClick={onConfirm}>
          {confirmLabel}
        </ButtonBase>
      </div>
    </Modal>
  );
}
