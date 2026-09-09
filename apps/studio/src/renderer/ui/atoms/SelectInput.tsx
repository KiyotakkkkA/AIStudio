import { Select, type SelectOption } from "@kiyotakkkka/zvs-uikit-lib";

export interface SelectInputProps {
  readonly value: string;
  readonly options: readonly SelectOption[];
  readonly onChange: (value: string) => void;
  readonly label: string;
  readonly placeholder?: string;
  readonly invalid?: boolean;
  readonly disabled?: boolean;
}

export default function SelectInput({
  value,
  options,
  onChange,
  label,
  placeholder,
  invalid = false,
  disabled = false,
}: SelectInputProps) {
  const trigger = [
    "h-[34px] w-full gap-[8px] rounded-[6px] bg-main-900 px-[10px] py-0 text-[12.5px] text-main-100",
    invalid ? "border-err" : "border-main-600 hover:border-main-500",
  ].join(" ");

  return (
    <Select
      value={value}
      options={[...options]}
      onChange={onChange}
      placeholder={placeholder}
      disabled={disabled}
      className="w-full"
    >
      <Select.Trigger className={trigger} rounded="" />
      <Select.Menu label={label} rounded="rounded-md">
        <Select.Options rounded="rounded-md" className="text-[12.5px]" />
      </Select.Menu>
    </Select>
  );
}
