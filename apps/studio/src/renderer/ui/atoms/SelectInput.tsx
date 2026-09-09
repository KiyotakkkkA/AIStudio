import { mdiChevronDown } from "@mdi/js";
import type { ReactNode } from "react";
import Icon from "./Icon";

export interface SelectInputOption {
  readonly value: string;
  readonly label: string;
}

export interface SelectInputProps {
  readonly value: string;
  readonly options: readonly SelectInputOption[];
  readonly onChange: (value: string) => void;
  readonly id?: string;
  readonly invalid?: boolean;
  readonly disabled?: boolean;
  readonly leading?: ReactNode;
  readonly "aria-label"?: string;
}

export default function SelectInput({
  value,
  options,
  onChange,
  id,
  invalid = false,
  disabled = false,
  leading,
  "aria-label": ariaLabel,
}: SelectInputProps) {
  return (
    <div
      className={`flex h-[34px] items-center gap-[8px] rounded-[6px] border bg-main-900 px-[10px] text-[12.5px] text-main-100 focus-within:border-accent-dark ${
        invalid ? "border-err" : "border-main-600"
      } ${disabled ? "opacity-60" : ""}`}
    >
      {leading}
      <select
        id={id}
        aria-label={ariaLabel}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        className="min-w-0 flex-1 appearance-none bg-transparent text-main-100 outline-none"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value} className="bg-main-900 text-main-100">
            {option.label}
          </option>
        ))}
      </select>
      <Icon path={mdiChevronDown} size={15} className="flex-none text-main-500" />
    </div>
  );
}
