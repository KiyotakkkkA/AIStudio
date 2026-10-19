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
      className={`w-110 max-w-[92vw] overflow-hidden rounded-card border bg-main-900 p-0 shadow-pop ${tone === "danger" ? "border-err-border" : "border-line"}`}
    >
      <div className="px-5 pt-5">
        <h2
          className={`text-[15px] font-semibold tracking-[-0.01em] ${tone === "danger" ? "text-err" : "text-main-50"}`}
        >
          {title}
        </h2>
        <div className="mt-2 text-[13px] leading-relaxed text-main-400">{content}</div>
      </div>
      <div className="mt-5 flex justify-end gap-2 bg-main-850 px-5 py-3">
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
