import { Switcher } from "@kiyotakkkka/zvs-uikit-lib";

export interface SegmentedControlOption<T extends string> {
  readonly value: T;
  readonly label: string;
}

export interface SegmentedControlProps<T extends string> {
  readonly value: T;
  readonly options: readonly SegmentedControlOption<T>[];
  readonly onChange: (value: T) => void;
  readonly label: string;
}

export default function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  label,
}: SegmentedControlProps<T>) {
  return (
    <Switcher
      value={value}
      label={label}
      options={options.map((option) => ({ value: option.value, label: option.label }))}
      onChange={(next) => {
        onChange(next as T);
      }}
      rounded=""
      className="gap-1 rounded-lg border-main-700 bg-main-800 p-1"
      classNames={{ tab: "rounded-[5px] px-[14px] py-[5px] text-[12px]" }}
    />
  );
}
