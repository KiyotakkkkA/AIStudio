import { useRef, useState, type MouseEvent, type MouseEventHandler, type ReactNode } from "react";
import ConfirmModal, { type ConfirmModalSetup } from "../../molecules/ConfirmModal";

export type ConfirmProps =
  | { readonly needConfirm: true; readonly modalSetup: ConfirmModalSetup }
  | { readonly needConfirm?: false; readonly modalSetup?: ConfirmModalSetup };

export default function useConfirmedClick({
  needConfirm = false,
  modalSetup,
  disabled,
  onClick,
}: {
  readonly needConfirm?: boolean;
  readonly modalSetup?: ConfirmModalSetup;
  readonly disabled?: boolean;
  readonly onClick?: MouseEventHandler<HTMLButtonElement>;
}): {
  readonly onClick: MouseEventHandler<HTMLButtonElement>;
  readonly modal: ReactNode;
} {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const confirmed = useRef(false);

  return {
    onClick: (event: MouseEvent<HTMLButtonElement>) => {
      if (needConfirm && !confirmed.current) {
        event.preventDefault();
        event.stopPropagation();
        trigger.current = event.currentTarget;
        setOpen(true);
        return;
      }
      onClick?.(event);
    },
    modal:
      needConfirm && modalSetup ? (
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
      ) : null,
  };
}
