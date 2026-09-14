import { InputCheckSlided } from "@kiyotakkkka/zvs-uikit-lib";

export interface ToggleProps {
  readonly checked: boolean;
  readonly label: string;
  readonly onChange: (checked: boolean) => void;
  readonly id?: string;
  readonly disabled?: boolean;
}

export default function Toggle({ checked, label, onChange, id, disabled = false }: ToggleProps) {
  return (
    <InputCheckSlided
      ref={
        id === undefined
          ? undefined
          : (input) => {
              if (input !== null) input.id = id;
            }
      }
      checked={checked}
      disabled={disabled}
      onChange={onChange}
      className="flex items-center gap-2.25 text-left disabled:opacity-60"
      variant="tertiary"
    >
      <span className="text-sm">{label}</span>
    </InputCheckSlided>
  );
}
