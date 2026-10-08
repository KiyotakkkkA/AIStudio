import { Select, type PositionAnchor, type SelectOption } from "@kiyotakkkka/zvs-uikit-lib";

export interface SelectInputProps {
  readonly value: string;
  readonly options: readonly SelectOption[];
  readonly onChange: (value: string) => void;
  readonly label: string;
  readonly placeholder?: string;
  readonly invalid?: boolean;
  readonly disabled?: boolean;
  readonly menuPlacement?: PositionAnchor;
}

export default function SelectInput({
  value,
  options,
  onChange,
  label,
  placeholder,
  invalid = false,
  disabled = false,
  menuPlacement = "bottom-left",
}: SelectInputProps) {
  const trigger = [
    "h-9 w-full gap-2 rounded-control bg-main-900 px-3 py-0 text-[13px] text-main-100",
    invalid ? "border-err" : "border-main-750 hover:border-main-600",
  ].join(" ");

  return (
    <Select
      value={value}
      options={[...options]}
      onChange={onChange}
      placeholder={placeholder}
      disabled={disabled}
      className="w-full"
      menuPlacement={menuPlacement}
    >
      <Select.Trigger className={trigger} rounded="" />
      <Select.Menu
        label={label}
        rounded="rounded-xl"
        className="border-main-750 bg-main-900 p-1 shadow-pop"
      >
        <Select.Options rounded="rounded-md" className="text-[13px]" />
      </Select.Menu>
    </Select>
  );
}
