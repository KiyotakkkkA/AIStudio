import ButtonBase, { type ButtonProps as ButtonBaseProps } from "./ButtonBase";
import useConfirmedClick, { type ConfirmProps } from "./useConfirmedClick";

export type { ButtonSize, ButtonTone } from "./ButtonBase";

export type ButtonProps = ButtonBaseProps & ConfirmProps;

export default function Button({
  needConfirm,
  modalSetup,
  onClick,
  disabled,
  children,
  ...props
}: ButtonProps) {
  const confirm = useConfirmedClick({ needConfirm, modalSetup, disabled, onClick });

  return (
    <>
      <ButtonBase {...props} disabled={disabled} onClick={confirm.onClick}>
        {children}
      </ButtonBase>
      {confirm.modal}
    </>
  );
}
