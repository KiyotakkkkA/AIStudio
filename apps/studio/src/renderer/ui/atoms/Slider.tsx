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
        <label htmlFor={id} className="text-[11px] font-medium text-main-400">
          {label}
        </label>
        {readout === undefined ? null : (
          <span className="font-mono text-[11.5px] text-accent-medium">{readout}</span>
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
          track: "h-1 rounded-[2px] bg-main-700",
          fill: "h-1 rounded-[2px] bg-accent-dark",
          thumb: "size-3.5 rounded-full border-2 border-main-900 bg-accent-dark",
        }}
      />
      {note === undefined ? null : <p className="mt-1.5 text-[11px] text-main-500">{note}</p>}
    </div>
  );
}
