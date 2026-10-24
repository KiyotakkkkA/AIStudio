import { useId, useRef, type KeyboardEvent } from "react";
import { LayoutGroup, motion } from "motion/react";
import { useTransition } from "../motion/fx";

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

const STEPS: Record<string, (index: number, count: number) => number> = {
  ArrowLeft: (index, count) => (index === 0 ? count - 1 : index - 1),
  ArrowUp: (index, count) => (index === 0 ? count - 1 : index - 1),
  ArrowRight: (index, count) => (index === count - 1 ? 0 : index + 1),
  ArrowDown: (index, count) => (index === count - 1 ? 0 : index + 1),
  Home: () => 0,
  End: (_, count) => count - 1,
};

export default function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
  label,
  ghost = false,
}: SegmentedControlProps<T>) {
  const group = useId();
  const transition = useTransition("layout");
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const found = options.findIndex((option) => option.value === value);
  const current = found === -1 ? 0 : found;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = STEPS[event.key];
    if (step === undefined || options.length === 0) return;
    event.preventDefault();
    const next = step(current, options.length);
    buttons.current[next]?.focus();
    buttons.current[next]?.click();
  };

  return (
    <LayoutGroup id={group}>
      <div
        role="radiogroup"
        aria-label={label}
        onKeyDown={onKeyDown}
        className={`inline-flex w-fit items-center gap-0.5 rounded-control ${ghost ? "bg-transparent p-0" : "bg-main-700 p-0.75"}`}
      >
        {options.map((option, index) => {
          const checked = option.value === value;
          return (
            <button
              key={option.value}
              ref={(element) => {
                buttons.current[index] = element;
              }}
              type="button"
              role="radio"
              aria-checked={checked}
              tabIndex={index === current ? 0 : -1}
              onClick={() => onChange(option.value)}
              className={`relative cursor-pointer rounded-[6px] px-3 py-1 text-[12px] font-medium ${
                checked ? "text-main-50" : "text-main-300 hover:text-main-100"
              }`}
            >
              {checked ? (
                <motion.span
                  layoutId="segment"
                  transition={transition}
                  aria-hidden="true"
                  className="absolute inset-0 rounded-[6px] bg-main-900 fx-on:shadow-[0_1px_2px_rgb(40_25_10/0.1)]"
                />
              ) : null}
              <span className="relative">{option.label}</span>
            </button>
          );
        })}
      </div>
    </LayoutGroup>
  );
}
