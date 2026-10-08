import { InputSlider } from "@kiyotakkkka/zvs-uikit-lib";
import type { ReactNode } from "react";

export interface SliderProps {
  readonly id?: string;
  readonly label: string;
  readonly value: number;
  readonly min: number;
  readonly max: number;
  readonly step?: number;
  readonly disabled?: boolean;
  readonly readout?: ReactNode;
  readonly note?: string;
  readonly onChange: (value: number) => void;
}

export default function Slider({
  id,
  label,
  value,
  min,
  max,
  step = 1,
  disabled = false,
  readout,
  note,
  onChange,
}: SliderProps) {
  return (
    <div className={`min-w-0 ${disabled ? "opacity-50" : ""}`}>
      <div className="mb-2.25 flex items-baseline justify-between gap-2">
        <label htmlFor={id} className="text-[12px] font-medium text-main-300">
          {label}
        </label>
        {readout === undefined ? null : (
          <span className="rounded-pill bg-accent-soft px-1.5 py-px font-mono text-[11.5px] text-accent-medium">
            {readout}
          </span>
        )}
      </div>
      <InputSlider
        id={id}
        aria-label={label}
        value={value}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        showValue={false}
        onChange={onChange}
        className="w-full"
        classNames={{
          track: "h-1.5 rounded-full bg-main-600/70",
          fill: "h-1.5 rounded-full bg-accent-dark",
          thumb: "size-4 rounded-full border-2 border-accent-dark bg-main-900 shadow-card",
        }}
      />
      {note === undefined ? null : <p className="mt-2 text-[11.5px] text-main-500">{note}</p>}
    </div>
  );
}
