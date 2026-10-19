import { Switcher } from "@kiyotakkkka/zvs-uikit-lib";

interface SegmentedControlOption<T extends string> {
  readonly value: T;
  readonly label: string;
}

export interface SegmentedControlProps<T extends string> {
  readonly value: T;
  readonly options: readonly SegmentedControlOption<T>[];
  readonly onChange: (value: T) => void;
  readonly label: string;
  readonly ghost?: boolean;
}

export default function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  label,
  ghost = false,
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
      className={`gap-0.5 rounded-control ${ghost ? "border-transparent bg-transparent p-0" : "border-transparent bg-main-700 p-0.75"}`}
      classNames={{
        tab: "rounded-[6px] px-3 py-1 text-[12px] font-medium hover:bg-transparent aria-checked:bg-main-900 aria-checked:text-main-50 aria-checked:shadow-[0_1px_2px_rgb(40_25_10/0.1)] aria-checked:hover:bg-main-900",
      }}
    />
  );
}
