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
      className="flex items-center gap-2.5 text-left disabled:opacity-60"
      classNames={{
        control: "h-5 w-9",
        thumb:
          "size-3.5 translate-x-0.75 shadow-[0_1px_2px_rgb(0_0_0/0.25)] peer-checked:translate-x-4.5",
      }}
      variant="tertiary"
    >
      <span className="text-[13px] text-main-200">{label}</span>
    </InputCheckSlided>
  );
}
