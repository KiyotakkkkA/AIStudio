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
    <div
      role="radiogroup"
      aria-label={label}
      className="flex w-fit gap-[4px] rounded-[8px] border border-main-700 bg-main-800 p-[4px]"
    >
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(option.value)}
            className={`rounded-[5px] px-[14px] py-[5px] text-[12px] ${
              active ? "bg-main-600 font-semibold text-main-50" : "text-main-400"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
