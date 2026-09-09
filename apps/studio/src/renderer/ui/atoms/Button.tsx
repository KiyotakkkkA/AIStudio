import { useRef, useState } from "react";
import ConfirmModal, { type ConfirmModalSetup } from "../molecules/ConfirmModal";
import ButtonBase, { type ButtonProps as ButtonBaseProps } from "./ButtonBase";

export type { ButtonTone } from "./ButtonBase";

export type ButtonProps = ButtonBaseProps &
  (
    | { readonly needConfirm: true; readonly modalSetup: ConfirmModalSetup }
    | { readonly needConfirm?: false; readonly modalSetup?: ConfirmModalSetup }
  );

export default function Button({
  needConfirm = false,
  modalSetup,
  onClick,
  disabled,
  children,
  ...props
}: ButtonProps) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const confirmed = useRef(false);

  return (
    <>
      <ButtonBase
        {...props}
        disabled={disabled}
        onClick={(event) => {
          if (needConfirm && !confirmed.current) {
            event.preventDefault();
            event.stopPropagation();
            trigger.current = event.currentTarget;
            setOpen(true);
            return;
          }
          onClick?.(event);
        }}
      >
        {children}
      </ButtonBase>
      {needConfirm && modalSetup ? (
        <ConfirmModal
          {...modalSetup}
          open={open}
          disabled={disabled}
          onCancel={() => setOpen(false)}
          onConfirm={() => {
            setOpen(false);
            if (disabled || !trigger.current?.isConnected) return;
            // Replay a native click so submit buttons and onClick keep their normal behavior.
            confirmed.current = true;
            try {
              trigger.current.click();
            } finally {
              confirmed.current = false;
            }
          }}
        />
      ) : null}
    </>
  );
}
